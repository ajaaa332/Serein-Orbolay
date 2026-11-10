# Serein + Orbolay

**A native Rust Discord client with an in-game overlay for voice activity and message notifications.**

This Windows-focused fork connects [Serein](https://github.com/ViceVerse-cz/Serein) directly to [Orbolay](https://github.com/SpikeHD/Orbolay). Join a Discord voice channel in Serein and let Orbolay display the participants, speaking activity, and mute/deafen indicators over your game. Eligible incoming messages appear as overlay notifications.

The bridge is compiled into Serein. You run Serein and Orbolay together; no Python script, separate bridge application, Vencord plugin, or official Discord desktop client is required.

**[Download the Windows build](https://github.com/ajaaa332/Serein-Orbolay/releases/tag/orbolay-v0.1.0)** · **[Integration source](https://github.com/ajaaa332/Serein-Orbolay/tree/feat/orbolay-native-bridge)** · **[Report an issue](https://github.com/ajaaa332/Serein-Orbolay/issues)**

> This is an independent community fork maintained by **ajaaa332**. The original Serein client is developed by ViceVerse-cz and its contributors; Orbolay and its official bridge are developed by SpikeHD and their contributors. This fork adds the native Serein-to-Orbolay integration.

## Contents

- [What works](#what-works)
- [Screenshot](#screenshot)
- [Download and install](#download-and-install)
- [Everyday use](#everyday-use)
- [How the integration works](#how-the-integration-works)
- [Notifications and privacy](#notifications-and-privacy)
- [Resource use](#resource-use)
- [Known limitations](#known-limitations)
- [Troubleshooting](#troubleshooting)
- [Build from source](#build-from-source)
- [Testing and current status](#testing-and-current-status)
- [AI-assisted development](#ai-assisted-development)
- [Contributing and reporting problems](#contributing-and-reporting-problems)
- [Credits and licenses](#credits-and-licenses)

## What works

| Feature | Behavior |
| --- | --- |
| Voice participants | Displays the users in your active Serein voice channel. |
| Speaking activity | Updates the overlay from Serein's native voice activity state. |
| Mute and deafen indicators | Includes local and server mute/deafen state. |
| Joining and leaving | Updates membership when people join or leave, and clears or replaces the roster when you leave or switch channels. |
| Message notifications | Forwards eligible incoming notification previews using Serein's existing notification rules. |
| Reconnect | Retries the local connection every five seconds and sends the latest voice roster when Orbolay returns. |
| Duplicate suppression | Tracks recent channel/message ID pairs to suppress repeated alerts. |
| Native integration | Runs inside Serein using its existing Rust async runtime. |
| Independent Discord connection | Uses Serein's own connection to Discord; the official desktop client can stay closed. |

**Use Serein to mute, deafen, leave calls, and navigate conversations.** Orbolay's overlay action buttons are not connected back to Serein in this release.

## Screenshot

<!-- Add the owner's supplied screenshot here once it is available. Do not substitute an upstream or synthetic screenshot for a live integration capture. -->

A screenshot of this fork running with Orbolay will be added here.

## Download and install

The published integration package targets **Windows x64**. You do not need development tools to use it.

1. Open the [release page](https://github.com/ajaaa332/Serein-Orbolay/releases/tag/orbolay-v0.1.0).
2. Under **Assets**, download **Serein-Orbolay-Windows-x64.zip**. GitHub's automatically generated source archives are for development, not running the application.
3. Extract the entire ZIP to a folder you can keep, such as `Documents\Serein-Orbolay`.
4. Quit any other Serein copy completely with **Ctrl+Q** or **Quit** in its tray menu. Closing the window can leave the old copy running.
5. Open the included **orbolay.exe**. In its connection settings, select **WebSocket** mode and port **6888**.
6. Open **serein.exe** from the same extracted folder and sign in using Serein's normal login flow.
7. Join a voice channel. Enable notifications in Serein's Notifications settings if you want message previews in the overlay.

The local endpoint is fixed in this integration:

```text
ws://127.0.0.1:6888
```

Keep the resources and license files beside the executables. The release is a portable package rather than a Windows installer. It includes the official, unmodified **Orbolay v4.3.1** Windows executable.

Serein uses Microsoft's WebView2 runtime for its login window. Development tools such as Rust, Cargo, Visual Studio Build Tools, Node.js, and Python are not needed to run the downloaded package.

### Release files

| Asset | Purpose |
| --- | --- |
| `Serein-Orbolay-Windows-x64.zip` | Ready-to-run Serein and Orbolay, launch helper, resources, instructions, and licenses. |
| `Serein-Orbolay-source.zip` | Complete modified Serein source for the tested release. |
| `Serein-Orbolay.patch` | Integration patch against the original Serein base commit. |
| `Build-Integration.ps1` | Windows rebuild helper for machines with the required toolchain installed. |
| `SHA256SUMS.txt` | Checksums for the release downloads. |

To inspect a downloaded file's SHA-256 checksum in PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\Serein-Orbolay-Windows-x64.zip
```

Compare the result with the matching entry in `SHA256SUMS.txt`.

## Everyday use

1. Open Orbolay.
2. Open the modified Serein.
3. Join a Discord voice channel in Serein.
4. Open your game and use Orbolay's overlay.
5. Receive eligible message notifications while you play.

The included **Start.cmd** opens both applications for convenience. Use it when both are closed. Orbolay's default overlay shortcut is **Ctrl + backtick**; use your configured shortcut if you changed it.

If Orbolay restarts, leave Serein running. The bridge reconnects automatically and restores the current voice roster. Messages received while the overlay is disconnected are not replayed later.

## How the integration works

Serein already handles Discord authentication, messages, voice membership, and voice transport. This fork reads that existing application state and turns it into the WebSocket messages Orbolay expects.

```mermaid
flowchart LR
    Discord[Discord services] --> Serein[Serein native client]
    Serein --> Adapter[Built-in Rust bridge]
    Adapter -->|Local WebSocket: 127.0.0.1:6888| Orbolay[Orbolay]
    Orbolay --> Overlay[In-game overlay]
```

### Voice state

The adapter reads the active voice channel's participants, resolves available display names and avatar hashes, combines mute/deafen flags, and uses the native voice worker's speaking state. It sends membership and state changes to Orbolay rather than generating synthetic users.

If a participant's name has not arrived yet, the overlay can temporarily show their actual user ID. Leaving a channel clears the roster; switching channels replaces it. Pure speaking changes use sparse updates to preserve Orbolay's speaking grace timer.

### Message notifications

The integration uses Serein's existing filtered notification queue. It forwards a sender/title, preview text, optional Discord CDN icon, and the message/channel identifiers Orbolay expects. It does not scan or export the stored conversation history.

### Protocol and reconnect

The payloads were implemented by inspecting [Orbolay's official bridge](https://github.com/SpikeHD/vc-orbolay-bridge) and Orbolay's receiver source. The bridge uses these commands:

| Command | Purpose |
| --- | --- |
| `REGISTER_CONFIG` | Associates the connection with the current user. |
| `STREAMER_MODE` | Sends the bridge's normal display configuration. |
| `CHANNEL_JOINED` | Publishes the current channel roster. |
| `VOICE_STATE_UPDATE` | Updates participants and their voice state. |
| `CHANNEL_LEFT` | Clears the voice channel display. |
| `MESSAGE_NOTIFICATION` | Delivers an eligible notification preview. |

Connection/write operations have three-second timeouts. Failed connections retry every five seconds, and a connected socket sends a keepalive ping every 30 seconds. Reconnection sends the latest roster rather than replaying every missed voice update. Login generations keep queued alerts from crossing account changes; logout clears the overlay state.

For the implementation details and inspected upstream revisions, see [the integration notes](docs/orbolay.md).

## Notifications and privacy

Notification previews follow Serein's existing preferences. A message may intentionally produce no overlay alert when:

- Notifications are disabled in Serein.
- Do Not Disturb or a conversation mute suppresses the alert.
- You are focused on that conversation at its latest message.
- Orbolay is offline, the bounded queue is full, or the preview exceeds the size limit.

Overlay alerts do not require registering a Windows toast shortcut.

**The added bridge connects only to the local loopback address.** It does not send Discord tokens, voice session secrets, raw gateway payloads, or message history to Orbolay. It does send the selected display information and eligible message previews to the local overlay, so those previews can be visible on your screen or in a recording.

Serein still connects to Discord to function, and Orbolay can fetch avatars from Discord's CDN. The local bridge does not introduce an external relay server. Serein's account storage behavior is described in the [storage policy](docs/storage-policy.md).

Serein is an unofficial Discord client and is not endorsed by Discord. Review the upstream [compatibility information](docs/discord-compatibility.md) and [authentication guide](docs/authentication.md) before using it.

## Resource use

The integration reuses Serein's Tokio runtime and runs one async transport task. Voice snapshots are coalesced so newer state replaces older pending state, and notification submission does not block the UI.

| Bound | Limit |
| --- | --- |
| Voice participants per snapshot | 64 |
| Pending notification queue | 32 items |
| Notification title/body/icon content | 16 KiB combined |
| Recent notification IDs for deduplication | 4,096 channel/message pairs |
| Outbound JSON payload | 128 KiB |
| Inbound WebSocket frame/message | 4 KiB |

These are implementation bounds, not a measured total RAM or CPU figure. The overlay integration has not had a controlled before/after runtime benchmark. Upstream Serein benchmarks should not be treated as measurements of this Windows fork with Orbolay running.

## Known limitations

- **Windows x64 is the packaged and validated platform.** This fork does not currently publish tested integration builds for macOS, Linux, or Windows ARM64.
- **Overlay action buttons are not implemented.** Use Serein's own controls.
- **Notifications are previews, not a complete message feed.** Existing filtering applies, and offline notifications are dropped.
- **The current snapshot has a 64-participant cap.** Very large channels are not fully represented.
- **Game rendering depends on Orbolay.** The maintainer has reported working voice and notifications; this is not a claim that every game or rendering mode was tested.
- **The portable build is unsigned.** No signed Windows installer is provided by this fork.
- **Upstream binary updates replace the integration.** Use this fork's releases or rebuild its source when updating.

## Troubleshooting

| Problem | Things to check |
| --- | --- |
| No participants appear | Confirm Orbolay uses WebSocket mode on port 6888, launch this modified Serein, and join a voice channel. |
| Launching the new executable opens an old Serein window | Fully quit the existing Serein instance first, including its tray process. Then launch the extracted executable again. |
| Voice works but notifications do not | Enable Serein notifications, check DND and conversation mutes, and test while another conversation or application is focused. |
| The overlay stops after restarting Orbolay | Wait at least five seconds, then confirm its connection mode and port are still correct. |
| A participant temporarily appears as a numeric ID | Serein has not resolved that user's profile yet. The bridge uses available member/recipient data as it arrives. |
| Mute/deafen buttons in the overlay do nothing | Those buttons are not wired back to Serein. Use Serein's controls. |
| The overlay is absent in one game | Check Orbolay's overlay configuration and that game's compatibility in the upstream Orbolay project. |
| The bridge disappeared after an update | An upstream Serein executable may have replaced the custom build. Download this fork's release again. |

For a connection diagnostic, PowerShell can check whether anything is listening on the expected port:

```powershell
Test-NetConnection 127.0.0.1 -Port 6888
```

A successful TCP check shows that the port is reachable; it does not by itself prove the WebSocket protocol or in-game rendering works.

## Build from source

### Windows prerequisites

- Git, if cloning the repository.
- The pinned **Rust 1.99.1 MSVC** toolchain with Cargo.
- Microsoft C++ Build Tools, an x64 MSVC toolchain, Windows SDK, and CMake.
- WebView2 runtime for Serein's login window.

The native bridge does not add a Node.js or Python runtime requirement.

### Rebuild the released source

For the exact published version, download `Serein-Orbolay-source.zip` and `Build-Integration.ps1` from the same release. Extract the source so the layout is:

```text
build-folder/
  Build-Integration.ps1
  Serein-source/
    Cargo.toml
    apps/
    crates/
    ...
```

Run the helper from PowerShell:

```powershell
.\Build-Integration.ps1 -Action BridgeTests
.\Build-Integration.ps1 -Action Check
.\Build-Integration.ps1 -Action Package
```

The helper finds the installed Microsoft tools, uses one Cargo build job, thin LTO and 16 release codegen units, and temporarily maps a free drive letter to shorten native build paths. It does not install the toolchain or change security settings.

If `S:` is already occupied, select a free letter:

```powershell
.\Build-Integration.ps1 -Action Package -BuildDrive T
```

The packaged executable is produced at `Serein-source\dist\serein.exe`. Keep the adjacent resources and licenses. Packaging Serein does not automatically download or bundle Orbolay; the published portable ZIP includes it separately.

### Work on the integration branch

```powershell
git clone --branch feat/orbolay-native-bridge https://github.com/ajaaa332/Serein-Orbolay.git
cd Serein-Orbolay
```

With the C++ build environment initialized, the underlying commands are:

```powershell
cargo test --locked -p orbolay-bridge
cargo test --locked -p serein --bin serein orbolay
cargo xtask check
cargo xtask package
```

The release helper is recommended for the lower-memory Windows build settings and short native paths. You can use it with a clone by passing `-SourceRoot` with the clone's absolute path.

### Source map

| Path | Role |
| --- | --- |
| [`crates/orbolay-bridge`](crates/orbolay-bridge) | Local WebSocket transport, protocol messages, bounds, deduplication, reconnect, and tests. |
| [`apps/desktop/src/orbolay.rs`](apps/desktop/src/orbolay.rs) | Converts authenticated Serein state into overlay snapshots and notifications. |
| [`apps/desktop/src/main.rs`](apps/desktop/src/main.rs) | Starts the bridge and publishes state through application logic. |
| [`apps/desktop/src/notification_runtime.rs`](apps/desktop/src/notification_runtime.rs) | Forwards eligible alerts from Serein's notification pipeline. |
| [`docs/orbolay.md`](docs/orbolay.md) | Technical integration notes and protocol references. |

## Testing and current status

The published `orbolay-v0.1.0` binary was built from commit [`f9ada04`](https://github.com/ajaaa332/Serein-Orbolay/commit/f9ada04fbf3dd1211155253a742dc69885bcd4d9), based on upstream Serein `a7d57c6`.

| Validation | Result |
| --- | --- |
| Full workspace check | Passed: 1,500 tests passed, 16 ignored; formatting and Clippy checks succeeded. |
| Serein-to-overlay state mapping | Focused adapter test passed. |
| Native Rust synthetic events against Orbolay v4.3.1 | Passed; receiver logs confirmed voice changes, notification deduplication, and channel departure. |
| Windows x64 release packaging | Completed successfully. |
| Published asset integrity | GitHub asset SHA-256 digests matched the local release files. |
| Actual Discord voice and notifications | Confirmed working by the fork maintainer through manual use. |
| Automated live microphone/game tests and controlled runtime benchmarks | Not performed. |

The integration source lives on **`feat/orbolay-native-bridge`**. The fork's **`main`** branch contains a newer upstream snapshot; [pull request #1](https://github.com/ajaaa332/Serein-Orbolay/pull/1) tracks integration into it and currently requires conflict resolution. Use the release or the integration branch for the implementation described here.

Documentation updates after the release do not change its executable or its pinned source archive.

## AI-assisted development

**I used AI to help build this fork and its Orbolay integration.** OpenAI Codex assisted with development environment setup, inspecting the upstream projects, writing the Rust bridge and tests, resolving build issues, packaging the Windows release, and writing documentation.

I, **ajaaa332**, directed the project and manually tested actual voice activity and message notifications. AI assistance is disclosed openly so users can understand how this integration was developed. The recorded build and test results above describe checks that actually ran; they do not mean every feature, game, or scenario has been verified.

This disclosure describes the work on **this fork**. It does not attribute the original Serein client, Orbolay, or the official bridge to me or to this AI-assisted development session. Those projects and their contributors deserve their original credit.

## Contributing and reporting problems

Issues and focused improvements are welcome. For integration problems, include:

- The release or source commit you are using.
- Your Windows version and Orbolay version.
- Whether voice participants, speaking indicators, or message notifications are affected.
- The steps to reproduce and whether restarting Orbolay changes the result.
- A screenshot or short, redacted diagnostic excerpt when useful.

Do not include account credentials, Discord tokens, private message history, or unredacted personal information in an issue. Use synthetic data for automated tests. Live account, microphone, and private conversation tests must remain under the account owner's control.

Changes should preserve the local transport, bounded resource use, existing notification filters, and accurate documentation of what has actually been tested.

## Credits and licenses

- **[ViceVerse-cz/Serein](https://github.com/ViceVerse-cz/Serein)** and its contributors: the original native Rust Discord client and its networking, voice, UI, storage, and build infrastructure.
- **[SpikeHD/Orbolay](https://github.com/SpikeHD/Orbolay)** and its contributors: the in-game overlay.
- **[SpikeHD/vc-orbolay-bridge](https://github.com/SpikeHD/vc-orbolay-bridge)**: the official bridge implementation used to understand the real protocol.
- **[ajaaa332](https://github.com/ajaaa332)**: this fork, project direction, and manual acceptance testing.
- **OpenAI Codex**: AI development assistance as described above.

Serein is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE). Existing licenses and notices remain included. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled dependency and asset notices.

Orbolay is a separate GPL-licensed project. The portable release includes its license; its corresponding upstream source is available at [Orbolay v4.3.1](https://github.com/SpikeHD/Orbolay/tree/v4.3.1). This fork does not relicense Orbolay under Serein's licenses.

For general Serein documentation beyond the overlay integration, see the [upstream project](https://github.com/ViceVerse-cz/Serein), [architecture notes](docs/architecture.md), [voice guide](docs/voice.md), and [platform support](docs/platform-support.md).
