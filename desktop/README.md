# Warlay desktop client

`warlay.exe` is the Windows application players use: register servers, browse
and join rooms, chat, talk, and launch Warcraft III. It is a
[Tauri 2](https://tauri.app) app with a Vue 3 + TypeScript frontend and a Rust
backend. Privileged networking (LAN capture, game tunnels) runs in the
separate [daemon](../daemon/README.md), which the client talks to over a
named pipe and ships as a Tauri sidecar.

## Contents

- [Features](#features)
- [Architecture](#architecture)
- [How a hub game reaches your LAN screen](#how-a-hub-game-reaches-your-lan-screen)
- [Development](#development)
- [Building](#building)
- [Tests](#tests)

## Features

- **Servers rail** - register several hubs (`wss://host`, `ws://host:port` or
  `host:port`) and stay connected to all of them; the server provides its own
  display name during the handshake.
- **Rooms** - create (optionally with a password), list and join rooms in
  real time, with roster, activity log and automatic "in progress" detection.
- **Game launch** - starts Warcraft III with the configured executable and
  arguments once the room's tunnel is ready.
- **Chat and voice** - room and private text chat, Opus voice chat with an
  in-game overlay and a global microphone mute shortcut.
- **LAN tab** - the legacy lan-mesh flow: scan local subnets, find relay
  bridges, list observed games, and turn this PC into a mesh peer.
- **Agent and console views** - daemon status and settings, live logs, log
  level control.

## Architecture

```
desktop/
├── src/                        Vue 3 frontend
│   ├── api.ts                  typed wrappers over Tauri invoke/listen
│   ├── types.ts                DTO mirrors and address parsing
│   ├── store/                  Pinia stores (servers, rooms, chat, settings, agent, lanMesh, ...)
│   ├── components/             UI by feature (rail, rooms, servers, chat, lan, settings, console, base)
│   ├── composables/, utils/    shared helpers, zod validation schemas
│   └── assets/                 fonts, sounds, SCSS design tokens
├── src-tauri/                  Rust backend (crate `desktop`, binary `warlay`)
│   ├── src/commands/           #[tauri::command] handlers, the only layer touching AppHandle/State
│   ├── src/daemon_ipc.rs       named-pipe client for the daemon (wnetd)
│   ├── src/storage/            local SQLite: client identity, registered servers and their keys, settings
│   ├── src/voice/              audio capture/playback (cpal), Opus codec, jitter buffer
│   ├── src/network/            lan-mesh discovery and relay client
│   ├── src/launcher/           starts and tracks the game process
│   ├── src/overlay.rs          click-through in-game overlay window
│   └── tauri.conf.json         window, bundle (NSIS) and sidecar configuration
└── scripts/                    build helpers: daemon sidecar, portable zip
```

The backend never opens hub connections or tunnels itself: commands such as
`connect_to_server`, `start_hosting_tunnel` or `launch_game` are forwarded to
the daemon as `DaemonRequest`s (see `crates/protocol/src/ipc.rs`), and the
daemon pushes events back (`HubMessage`, `TunnelStatusChanged`, voice
frames, ...), which the backend re-emits to the frontend as Tauri events.

## How a hub game reaches your LAN screen

Warcraft III discovers LAN games with UDP broadcasts on port 6112 and plays
over TCP. The daemon reproduces both across the internet:

1. **Capture (host).** `lan_capture` (in `crates/engine`) opens a raw socket
   per local interface in Windows' `SIO_RCVALL` mode - the driver-free
   technique used by tools such as RawCap, needing only `ws2_32.dll` - and
   forwards the host's real W3GS broadcast bytes to the room.
2. **Replay (joiners).** `lan_rebroadcast` replays those exact bytes on each
   joiner's local network, so the game appears in _Local Area Network_ with
   no protocol guesswork.
3. **Tunnel.** When the joiner's game connects, the daemon carries the TCP
   session either over a direct QUIC connection between the players (after
   NAT hole punching through the hub's rendezvous service) or, as a
   fallback, through the hub's TCP tunnel. With a `wss://` server the tunnel
   is TLS-wrapped and the server certificate is verified.

`SIO_RCVALL` requires administrator rights, which is why the daemon runs
elevated; joiners only replay and connect, but run the same daemon.

## Development

Requirements: Windows, Rust stable, Node.js 20+.

```sh
npm ci
npm run prepare-daemon-sidecar   # builds wnetd.exe into src-tauri/binaries/
npm run tauri dev
```

`npm run tauri dev` starts Vite on port 1420 and opens the app against it.
Start the daemon elevated (`target/release/wnetd.exe`, or let the client
launch it) so the client has an agent to talk to.

Plain `cargo build` of `src-tauri` always targets the dev server URL; use
`npx tauri build` for a binary that embeds the frontend.

## Building

```sh
npx tauri build                 # frontend + daemon sidecar + warlay.exe + NSIS installer
npm run build-portable-zip      # zip with warlay.exe, wnetd.exe and LEEME.txt
```

Artifacts end up in `../target/release/bundle/nsis/`. Binaries link the MSVC
runtime statically (`src-tauri/.cargo/config.toml`), so they run on a clean
Windows install; they only need the WebView2 runtime, which ships with
Windows 10 and 11.

## Tests

```sh
npm run build                         # vue-tsc type-check + Vite bundle
cargo test -p desktop                 # backend unit tests (from the repo root)
```
