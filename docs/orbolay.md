# Native Orbolay integration

This build connects directly from Serein to `ws://127.0.0.1:6888`. No Discord desktop
client, Vencord, Python helper, extra service, or new third-party Rust dependency is
required. Open Orbolay, choose **Connection → Connection Mode → websocket**, then
open this Serein build and join a voice channel.

The adapter projects `State.voice.active.participants` (maintained by the Discord
gateway voice reducers), the native voice worker's `MessagingUi.voice_speaking`,
and the filtered `State.take_notification()` queue. User/server mute and deafen
flags are combined; names come from the channel's guild member data or DM recipients.
Unknown identities temporarily display their actual user ID until member data arrives.
Membership changes remove departed users and add new ones; switching/leaving channels
replaces/clears the overlay. Login, voice and message handling remain Serein's native
Discord implementation and do not use Discord IPC.

Notifications follow Serein's existing settings, DND, conversation mutes and message
filtering. They are suppressed for the focused conversation at its latest message.
Enable desktop notifications in Serein's Notifications settings. Overlay notifications
do not require a Windows toast shortcut. Messages received while the overlay is offline
are dropped rather than replayed. The bridge retains at most 4096 recent channel/message
ID pairs for duplicate suppression; the client-core activity tracker also filters replays.

The unofficial protocol was checked against:

- `SpikeHD/vc-orbolay-bridge` commit `422b14d6a5bd80b9d7bdb2e34c19a07c3c626807`, `index.tsx`.
- `SpikeHD/Orbolay` commit `a44e7946d022296a91df1bc6c6ca25fa646f7597`,
  `crates/orbolay-transport/src/websocket/mod.rs` and `orbolay-core/src/payloads/ws.rs`.

Commands are `REGISTER_CONFIG`, `STREAMER_MODE`, `CHANNEL_JOINED`, `CHANNEL_LEFT`,
`VOICE_STATE_UPDATE`, and `MESSAGE_NOTIFICATION`, with the original camelCase keys.
`avatarUrl` carries an avatar hash, matching the official bridge. Speaking transitions
use sparse updates so Orbolay runs its speaking grace timer. Full mute/deafen updates
preserve those indicators. Overlay-to-client action buttons are not implemented.

One task uses Serein's existing Tokio runtime. A single watch slot coalesces roster
updates (at most 64 participants). Notifications use a nonblocking 32-item queue with
16 KiB content limits. Outbound JSON is capped at 128 KiB; inbound frames/messages are
capped at 4 KiB. Connect/write operations have three-second timeouts; failed connections
retry every five seconds and republish the latest roster. The task has no idle polling
loop; a 30-second WebSocket ping keeps the connection active. Logout clears the overlay,
queued notifications are scoped to the login generation, and dropping the bridge aborts
its task. Synthetic/demo sessions never publish account data.

Only display state, IDs, avatar hashes/Discord CDN icon URLs and allowed notification
previews enter the loopback transport. Tokens, voice session secrets, raw gateway
payloads and message history never enter this module. Orbolay can fetch avatars from
Discord's public CDN as it does with its official bridge. No bridge data is sent to
external servers. Live Discord voice, microphone and in-game acceptance require an
owner-operated session; offline tests alone do not establish those results.

Build with the pinned Rust 1.99.1 MSVC toolchain, C++ Build Tools, Windows SDK and CMake:

```powershell
cargo test --locked -p orbolay-bridge
cargo test --locked -p serein --bin serein orbolay
cargo xtask check
cargo xtask package
```

The Windows executable is staged at `dist/serein.exe`. Keep the adjacent resources and
licenses. Close other Serein copies before launching it; Serein enforces one instance
per data directory. Installing an upstream Serein update replaces these custom changes,
so rebuild the integration when updating instead of applying an upstream binary update.
