//! Explicit synthetic acceptance probe for a running local Orbolay, never a Discord session.
use orbolay_bridge::{Bridge, Notification, Session, Snapshot, VoiceState};
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let mut bridge = Bridge::start(&tokio::runtime::Handle::current());
	let mut snapshot = Snapshot {
		session: Some(Session {
			generation: 1,
			user_id: 1,
		}),
		channel_id: Some("10".into()),
		states: vec![VoiceState {
			user_id: "1".into(),
			username: "Serein Rust Smoke Test (synthetic)".into(),
			avatar_url: String::new(),
			channel_id: "10".into(),
			deaf: false,
			mute: false,
			streaming: false,
			speaking: false,
		}],
	};
	bridge.publish(snapshot.clone());
	tokio::time::timeout(Duration::from_secs(10), async {
		while !bridge.is_connected() {
			tokio::time::sleep(Duration::from_millis(25)).await;
		}
	})
	.await
	.expect("Orbolay is not accepting WebSockets on 127.0.0.1:6888");
	println!("Connected to local Orbolay; sending only synthetic state.");
	for speaking in [true, false] {
		snapshot.states[0].speaking = speaking;
		bridge.publish(snapshot.clone());
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
	for _ in 0..2 {
		bridge.notify(Notification {
			title: "Serein native bridge smoke test".into(),
			body: "Synthetic notification; no Discord data or token was used.".into(),
			icon: String::new(),
			guild_id: None,
			channel_id: "10".into(),
			message_id: "100".into(),
		});
	}
	tokio::time::sleep(Duration::from_millis(500)).await;
	bridge.publish(Snapshot::default());
	tokio::time::sleep(Duration::from_millis(500)).await;
	println!(
		"Probe completed; inspect Orbolay's receiver log for one notification and channel clearing."
	);
}
