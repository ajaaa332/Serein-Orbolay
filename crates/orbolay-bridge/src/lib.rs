//! Credential-free adapter for Orbolay's unofficial, loopback-only bridge protocol.
//! Wire contract: SpikeHD/vc-orbolay-bridge and Orbolay's orbolay-transport websocket module.
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
	collections::VecDeque,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
	time::Duration,
};
use tokio::{
	net::TcpStream,
	sync::{mpsc, watch},
	task::JoinHandle,
	time::{MissedTickBehavior, interval, timeout},
};
use tokio_tungstenite::{
	WebSocketStream,
	tungstenite::{Message, protocol::WebSocketConfig},
};

const ENDPOINT: &str = "ws://127.0.0.1:6888";
const MAX_PARTICIPANTS: usize = 64;
const MAX_WIRE_BYTES: usize = 128 * 1024;
const MAX_NOTIFICATION_BYTES: usize = 16 * 1024;
const QUEUED_NOTIFICATIONS: usize = 32;
const RECENT_IDS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
	pub generation: u64,
	pub user_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceState {
	pub user_id: String,
	pub username: String,
	/// Discord avatar hash, as required by Orbolay (not a complete URL).
	pub avatar_url: String,
	pub channel_id: String,
	pub deaf: bool,
	pub mute: bool,
	pub streaming: bool,
	pub speaking: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
	pub session: Option<Session>,
	pub channel_id: Option<String>,
	pub states: Vec<VoiceState>,
}
impl Snapshot {
	fn valid(&self) -> bool {
		self.states.len() <= MAX_PARTICIPANTS
			&& self.channel_id.as_deref().is_none_or(valid_id)
			&& self.states.iter().all(|s| {
				valid_id(&s.user_id)
					&& Some(&s.channel_id) == self.channel_id.as_ref()
					&& s.username.len() <= 1024
					&& s.avatar_url.len() <= 128
			}) && (self.session.is_some() || (self.channel_id.is_none() && self.states.is_empty()))
	}
}
fn valid_id(id: &str) -> bool {
	id.len() <= 20
		&& id.bytes().all(|b| b.is_ascii_digit())
		&& id.parse::<u64>().is_ok_and(|id| id != 0)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
	pub title: String,
	pub body: String,
	pub icon: String,
	pub guild_id: Option<String>,
	pub channel_id: String,
	pub message_id: String,
}
struct Alert {
	session: Session,
	message: Notification,
}

/// One Tokio task on the application's existing runtime. No polling thread or message replay.
pub struct Bridge {
	snapshot: watch::Sender<Snapshot>,
	alerts: mpsc::Sender<Alert>,
	connected: Arc<AtomicBool>,
	seen: VecDeque<(String, String)>,
	session: Option<Session>,
	task: JoinHandle<()>,
}
impl Bridge {
	pub fn is_connected(&self) -> bool {
		self.connected.load(Ordering::Acquire)
	}
	pub fn start(runtime: &tokio::runtime::Handle) -> Self {
		let (snapshot, changes) = watch::channel(Snapshot::default());
		let (alerts, incoming) = mpsc::channel(QUEUED_NOTIFICATIONS);
		let connected = Arc::new(AtomicBool::new(false));
		let task = runtime.spawn(worker(
			changes,
			incoming,
			connected.clone(),
			ENDPOINT.to_owned(),
			Duration::from_secs(5),
		));
		Self {
			snapshot,
			alerts,
			connected,
			seen: VecDeque::new(),
			session: None,
			task,
		}
	}
	pub fn publish(&mut self, snapshot: Snapshot) {
		if !snapshot.valid() {
			return;
		}
		if self.session != snapshot.session {
			self.seen.clear();
			self.session = snapshot.session;
		}
		self.snapshot.send_if_modified(|current| {
			if *current == snapshot {
				false
			} else {
				*current = snapshot;
				true
			}
		});
	}
	pub fn notify(&mut self, message: Notification) {
		let Some(session) = self.session else { return };
		if !valid_id(&message.channel_id)
			|| !valid_id(&message.message_id)
			|| message.guild_id.as_deref().is_some_and(|id| !valid_id(id))
			|| message.title.len() + message.body.len() + message.icon.len()
				> MAX_NOTIFICATION_BYTES
		{
			return;
		}
		let id = (message.channel_id.clone(), message.message_id.clone());
		if self.seen.contains(&id) {
			return;
		}
		if self.seen.len() == RECENT_IDS {
			self.seen.pop_front();
		}
		self.seen.push_back(id);
		if self.is_connected() {
			let _ = self.alerts.try_send(Alert { session, message });
		}
	}
}
impl Drop for Bridge {
	fn drop(&mut self) {
		self.task.abort();
	}
}

type Socket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;
async fn send(socket: &mut Socket, value: Value) -> Result<(), ()> {
	let text = value.to_string();
	if text.len() > MAX_WIRE_BYTES {
		return Err(());
	}
	timeout(
		Duration::from_secs(3),
		socket.send(Message::Text(text.into())),
	)
	.await
	.map_err(|_| ())?
	.map_err(|_| ())
}

/// Roster replacement only for channel/session changes; otherwise emit changed users and departures.
fn updates(previous: Option<&Snapshot>, current: &Snapshot) -> Vec<Value> {
	let mut output = Vec::new();
	let reset = previous
		.is_none_or(|old| old.session != current.session || old.channel_id != current.channel_id);
	if reset {
		if let Some(session) = current.session {
			output.push(json!({"cmd":"REGISTER_CONFIG", "userId":session.user_id.to_string()}));
			output.push(json!({"cmd":"STREAMER_MODE", "enabled":false}));
		}
		output.push(if current.channel_id.is_some() {
			json!({"cmd":"CHANNEL_JOINED", "states":current.states})
		} else {
			json!({"cmd":"CHANNEL_LEFT"})
		});
		return output;
	}
	let previous = previous.expect("non-reset requires previous snapshot");
	for old in &previous.states {
		if !current.states.iter().any(|s| s.user_id == old.user_id) {
			output.push(
				json!({"cmd":"VOICE_STATE_UPDATE", "state":{"userId":old.user_id,"channelId":null}}),
			);
		}
	}
	for state in &current.states {
		if !previous.states.contains(state) {
			if let Some(old) = previous
				.states
				.iter()
				.find(|old| old.user_id == state.user_id)
			{
				let mut comparable = old.clone();
				comparable.speaking = state.speaking;
				if comparable == *state {
					// Orbolay starts its speaking grace timer only for this sparse payload.
					output.push(json!({"cmd":"VOICE_STATE_UPDATE", "state":{"userId":state.user_id,"speaking":state.speaking}}));
					continue;
				}
			}
			output.push(json!({"cmd":"VOICE_STATE_UPDATE", "state":state}));
			if !state.speaking && !state.mute && !state.deaf {
				output.push(
					json!({"cmd":"VOICE_STATE_UPDATE", "state":{"userId":state.user_id,"speaking":false}}),
				);
			}
		}
	}
	output
}

async fn worker(
	mut changes: watch::Receiver<Snapshot>,
	mut alerts: mpsc::Receiver<Alert>,
	connected: Arc<AtomicBool>,
	endpoint: String,
	retry: Duration,
) {
	loop {
		connected.store(false, Ordering::Release);
		while alerts.try_recv().is_ok() {}
		while changes.borrow().session.is_none() {
			if changes.changed().await.is_err() {
				return;
			}
		}
		let config = WebSocketConfig::default()
			.max_message_size(Some(4096))
			.max_frame_size(Some(4096));
		if let Ok(Ok((mut socket, _))) = timeout(
			Duration::from_secs(3),
			tokio_tungstenite::connect_async_with_config(&endpoint, Some(config), true),
		)
		.await
		{
			let mut previous = None;
			let mut heartbeat = interval(Duration::from_secs(30));
			heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
			'connection: loop {
				let current = changes.borrow_and_update().clone();
				for value in updates(previous.as_ref(), &current) {
					if send(&mut socket, value).await.is_err() {
						break 'connection;
					}
				}
				if current.session.is_none() {
					let _ = timeout(Duration::from_secs(1), socket.close(None)).await;
					break;
				}
				previous = Some(current.clone());
				connected.store(true, Ordering::Release);
				tokio::select! {
					changed = changes.changed() => { if changed.is_err() { return; } }
					alert = alerts.recv() => {
						let Some(alert) = alert else { return };
						// Never deliver queued content across logout/account switches.
						let session = changes.borrow().session;
						if Some(alert.session) == session && session == current.session
							&& send(&mut socket, json!({"cmd":"MESSAGE_NOTIFICATION","message":alert.message})).await.is_err() { break; }
					}
					message = socket.next() => match message {
						None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
						Some(Ok(Message::Ping(_))) if !matches!(timeout(Duration::from_secs(3), socket.flush()).await, Ok(Ok(()))) => break,
						_ => {} // Overlay commands never execute arbitrary client actions.
					},
					_ = heartbeat.tick() => {
						if !matches!(timeout(Duration::from_secs(3), socket.send(Message::Ping(Vec::new().into()))).await, Ok(Ok(()))) { break; }
					}
				}
			}
		}
		connected.store(false, Ordering::Release);
		while alerts.try_recv().is_ok() {}
		// State changes while offline coalesce in the watch slot; speaking must not
		// cause a reconnect attempt on every UI wake when Orbolay is absent.
		tokio::time::sleep(retry).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn snapshot() -> Snapshot {
		Snapshot {
			session: Some(Session {
				generation: 1,
				user_id: 1,
			}),
			channel_id: Some("10".into()),
			states: vec![VoiceState {
				user_id: "1".into(),
				username: "Offline fixture".into(),
				avatar_url: String::new(),
				channel_id: "10".into(),
				mute: false,
				deaf: false,
				streaming: false,
				speaking: false,
			}],
		}
	}
	fn notification(id: u64) -> Notification {
		Notification {
			title: "Fixture".into(),
			body: "Synthetic message".into(),
			icon: String::new(),
			guild_id: None,
			channel_id: "10".into(),
			message_id: id.to_string(),
		}
	}
	#[test]
	fn reconciles_speaking_departures_and_channel_switches() {
		let old = snapshot();
		let mut current = old.clone();
		current.states[0].speaking = true;
		assert_eq!(
			updates(Some(&old), &current),
			vec![json!({"cmd":"VOICE_STATE_UPDATE", "state":{"userId":"1","speaking":true}})]
		);
		assert_eq!(
			updates(Some(&current), &old),
			vec![json!({"cmd":"VOICE_STATE_UPDATE", "state":{"userId":"1","speaking":false}})]
		);
		assert!(updates(Some(&current), &current).is_empty());
		current.states.clear();
		assert_eq!(
			updates(Some(&old), &current),
			vec![json!({"cmd":"VOICE_STATE_UPDATE", "state":{"userId":"1","channelId":null}})]
		);
		assert_eq!(
			updates(Some(&old), &Snapshot::default()),
			vec![json!({"cmd":"CHANNEL_LEFT"})]
		);
		current.channel_id = Some("11".into());
		let events = updates(Some(&old), &current);
		assert_eq!(events[0], json!({"cmd":"REGISTER_CONFIG","userId":"1"}));
		assert_eq!(events[2], json!({"cmd":"CHANNEL_JOINED","states":[]}));
	}
	#[tokio::test]
	async fn bounds_notifications_deduplicates_and_drops_offline_content() {
		let (sender, _) = watch::channel(Snapshot::default());
		let (alerts, mut incoming) = mpsc::channel(QUEUED_NOTIFICATIONS);
		let mut bridge = Bridge {
			snapshot: sender,
			alerts,
			connected: Arc::new(AtomicBool::new(true)),
			seen: VecDeque::new(),
			session: None,
			task: tokio::spawn(std::future::pending()),
		};
		bridge.publish(snapshot());
		bridge.notify(notification(1));
		bridge.notify(notification(1));
		assert!(incoming.try_recv().is_ok());
		assert!(incoming.try_recv().is_err());
		let mut oversized = notification(2);
		oversized.body = "x".repeat(MAX_NOTIFICATION_BYTES + 1);
		bridge.notify(oversized);
		assert!(incoming.try_recv().is_err());
		let mut oversized = notification(2);
		oversized.message_id = "0".repeat(MAX_NOTIFICATION_BYTES);
		bridge.notify(oversized);
		assert!(incoming.try_recv().is_err());
		bridge.connected.store(false, Ordering::Release);
		bridge.notify(notification(3));
		bridge.connected.store(true, Ordering::Release);
		bridge.notify(notification(3));
		assert!(incoming.try_recv().is_err());
		for id in 4..100 {
			bridge.notify(notification(id));
		}
		assert_eq!(incoming.len(), QUEUED_NOTIFICATIONS);
		bridge.publish(Snapshot::default());
		assert!(bridge.seen.is_empty());
		bridge.notify(notification(100));
		assert_eq!(incoming.len(), QUEUED_NOTIFICATIONS);
		let mut invalid = snapshot();
		invalid.states[0].channel_id = "999".into();
		bridge.publish(invalid);
		assert!(bridge.snapshot.borrow().session.is_none());
	}
	async fn read_json(socket: &mut WebSocketStream<TcpStream>) -> Value {
		loop {
			match socket.next().await.unwrap().unwrap() {
				Message::Text(text) => return serde_json::from_str(&text).unwrap(),
				Message::Ping(_) => socket.flush().await.unwrap(),
				_ => {}
			}
		}
	}
	#[tokio::test]
	async fn loopback_transport_resynchronizes_after_restart_and_clears_on_logout() {
		timeout(Duration::from_secs(10), async {
			let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
			let endpoint = format!("ws://{}", listener.local_addr().unwrap());
			let initial = snapshot();
			let (sender, changes) = watch::channel(initial.clone());
			let (_alerts, incoming) = mpsc::channel(QUEUED_NOTIFICATIONS);
			let task = tokio::spawn(worker(
				changes,
				incoming,
				Arc::new(AtomicBool::new(false)),
				endpoint,
				Duration::ZERO,
			));
			let (tcp, _) = listener.accept().await.unwrap();
			let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
			assert_eq!(
				read_json(&mut socket).await,
				json!({"cmd":"REGISTER_CONFIG","userId":"1"})
			);
			assert_eq!(read_json(&mut socket).await["cmd"], "STREAMER_MODE");
			assert_eq!(
				read_json(&mut socket).await,
				json!({"cmd":"CHANNEL_JOINED","states":initial.states})
			);
			let mut speaking = initial.clone();
			speaking.states[0].speaking = true;
			sender.send_replace(speaking.clone());
			assert_eq!(read_json(&mut socket).await["state"]["speaking"], true);
			socket.close(None).await.unwrap();
			drop(socket);
			let (tcp, _) = listener.accept().await.unwrap();
			let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
			assert_eq!(read_json(&mut socket).await["cmd"], "REGISTER_CONFIG");
			assert_eq!(read_json(&mut socket).await["cmd"], "STREAMER_MODE");
			assert_eq!(
				read_json(&mut socket).await,
				json!({"cmd":"CHANNEL_JOINED","states":speaking.states})
			);
			sender.send_replace(Snapshot::default());
			assert_eq!(read_json(&mut socket).await, json!({"cmd":"CHANNEL_LEFT"}));
			task.abort();
		})
		.await
		.expect("loopback worker stalled");
	}
}
