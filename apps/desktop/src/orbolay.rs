//! Projects existing authenticated Serein state into the credential-free local bridge.
use client_core::{State, auth::AuthState, voice::Phase};
use model::{Id, MemberSlot};
use orbolay_bridge::{Bridge, Notification, Session, Snapshot, VoiceState};

pub fn snapshot(state: &State, ui: &ui::MessagingUi, fixture: bool) -> Snapshot {
	let Some(own) = state
		.user
		.as_ref()
		.filter(|_| !fixture && !state.demo && state.auth == AuthState::Authenticated)
	else {
		return Snapshot::default();
	};
	let mut snapshot = Snapshot {
		session: Some(Session {
			generation: state.generation,
			user_id: own.id.0,
		}),
		..Default::default()
	};
	let Some(call) = state
		.voice
		.active
		.as_ref()
		.filter(|call| call.phase != Phase::Failed && state.can_view(call.channel))
	else {
		return snapshot;
	};
	snapshot.channel_id = Some(call.channel.to_string());
	for p in call.participants.iter().filter(|p| p.user.0 != 0).take(64) {
		let member = state
			.voice
			.roster
			.iter()
			.find(|e| {
				Some(e.guild) == call.guild
					&& e.channel == call.channel
					&& e.participant.user == p.user
			})
			.and_then(|e| e.member.as_ref())
			.or_else(|| {
				state
					.members
					.as_ref()
					.filter(|m| m.guild == call.guild)
					.and_then(|m| {
						m.slots.iter().flatten().find_map(|slot| match slot {
							MemberSlot::Person(m) if m.user.id == p.user => Some(m),
							_ => None,
						})
					})
			});
		let user = member
			.map(|m| &m.user)
			.or_else(|| state.user.as_ref().filter(|u| u.id == p.user))
			.or_else(|| {
				state
					.channels
					.iter()
					.find(|c| c.id == call.channel)
					.and_then(|c| c.recipients.iter().find(|u| u.id == p.user))
			})
			.or_else(|| state.friend(p.user));
		let own = p.user == own.id;
		let mute = p.muted || p.server_muted || (own && (call.self_muted() || call.server_muted));
		let deaf =
			p.deafened || p.server_deafened || (own && (call.deafened || call.server_deafened));
		snapshot.states.push(VoiceState {
			user_id: p.user.to_string(),
			username: member
				.and_then(|m| m.nick.as_deref())
				.or_else(|| user.map(|u| state.user_display_name(u)))
				.map_or_else(|| format!("User {}", p.user), str::to_owned),
			avatar_url: user
				.and_then(|u| u.avatar.as_deref())
				.unwrap_or_default()
				.to_owned(),
			channel_id: call.channel.to_string(),
			mute,
			deaf,
			streaming: p.streaming || (own && ui.screen.busy),
			speaking: !mute && !deaf && ui.voice_speaking.contains(&p.user),
		});
	}
	// The local account can precede its voice-state echo while a call is joining.
	if !snapshot
		.states
		.iter()
		.any(|p| p.user_id == own.id.to_string())
		&& snapshot.states.len() < 64
	{
		snapshot.states.push(VoiceState {
			user_id: own.id.to_string(),
			username: state.user_display_name(own).to_owned(),
			avatar_url: own.avatar.clone().unwrap_or_default(),
			channel_id: call.channel.to_string(),
			mute: call.self_muted() || call.server_muted,
			deaf: call.deafened || call.server_deafened,
			streaming: ui.screen.busy,
			speaking: !call.self_muted()
				&& !call.server_muted
				&& ui.voice_speaking.contains(&own.id),
		});
	}
	snapshot
}

pub fn notify(
	bridge: &mut Bridge,
	state: &State,
	notification: &client_core::notifications::Notification,
) {
	let icon = notification
		.avatar_key
		.strip_prefix("default-")
		.filter(|index| index.parse::<u8>().is_ok_and(|n| n < 6))
		.map(|index| format!("https://cdn.discordapp.com/embed/avatars/{index}.png"))
		.or_else(|| {
			notification
				.avatar_key
				.split_once('-')
				.and_then(|(id, hash)| {
					(id.parse::<Id>().is_ok() && model::valid_avatar_hash(hash)).then(|| {
						format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128")
					})
				})
		})
		.unwrap_or_default();
	bridge.notify(Notification {
		title: notification.sender.clone(),
		body: notification.preview.clone(),
		icon,
		guild_id: state
			.channels
			.iter()
			.find(|c| c.id == notification.channel)
			.and_then(|c| c.guild)
			.map(|id| id.to_string()),
		channel_id: notification.channel.to_string(),
		message_id: notification.message.to_string(),
	});
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn orbolay_projects_real_voice_state_and_never_exports_offline_or_logged_out_sessions() {
		let mut state = test_support::voice_demo_state();
		let mut ui = ui::MessagingUi::default();
		assert!(snapshot(&state, &ui, false).session.is_none());
		// State-only test: no gateway, sockets, media worker or credential access.
		state.demo = false;
		state.auth = AuthState::Authenticated;
		ui.voice_speaking.push(Id(1));
		ui.voice_speaking.push(Id(2));
		let projected = snapshot(&state, &ui, false);
		assert_eq!(projected.states.len(), 3);
		assert!(
			projected
				.states
				.iter()
				.find(|s| s.user_id == "1")
				.unwrap()
				.speaking
		);
		let muted = projected.states.iter().find(|s| s.user_id == "2").unwrap();
		assert!(muted.mute && muted.deaf && !muted.speaking);
		state.voice.active.as_mut().unwrap().deafened = true;
		let projected = snapshot(&state, &ui, false);
		let own = projected.states.iter().find(|s| s.user_id == "1").unwrap();
		assert!(own.mute && own.deaf && !own.speaking);
		assert!(snapshot(&state, &ui, true).session.is_none());
		state.auth = AuthState::Unauthenticated;
		assert!(snapshot(&state, &ui, false).session.is_none());
	}
}
