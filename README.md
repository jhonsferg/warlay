<div align="center">

# Warlay

**Play Warcraft III LAN games with people who are not on your network**<br/>
No third-party VPN, no port forwarding on the players' side.

[![CI](https://img.shields.io/github/actions/workflow/status/jhonsferg/warlay/ci.yml?branch=main&style=for-the-badge&logo=githubactions&logoColor=white&label=CI)](https://github.com/jhonsferg/warlay/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jhonsferg/warlay?style=for-the-badge&logo=github&logoColor=white&label=Release)](https://github.com/jhonsferg/warlay/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow?style=for-the-badge&logo=opensourceinitiative&logoColor=white)](LICENSE)

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tokio](https://img.shields.io/badge/Tokio-000000?style=for-the-badge&logo=tokio&logoColor=white)](https://tokio.rs)
[![Tauri 2](https://img.shields.io/badge/Tauri_2-24C8DB?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)
[![Vue 3](https://img.shields.io/badge/Vue_3-4FC08D?style=for-the-badge&logo=vuedotjs&logoColor=white)](https://vuejs.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![Vite](https://img.shields.io/badge/Vite-646CFF?style=for-the-badge&logo=vite&logoColor=white)](https://vite.dev)
[![Pinia](https://img.shields.io/badge/Pinia-FFD859?style=for-the-badge&logo=pinia&logoColor=black)](https://pinia.vuejs.org)
[![Sass](https://img.shields.io/badge/Sass-CC6699?style=for-the-badge&logo=sass&logoColor=white)](https://sass-lang.com)
[![SQLite](https://img.shields.io/badge/SQLite-003B57?style=for-the-badge&logo=sqlite&logoColor=white)](https://www.sqlite.org)
[![Docker](https://img.shields.io/badge/Docker-2496ED?style=for-the-badge&logo=docker&logoColor=white)](https://www.docker.com)
[![GitHub Actions](https://img.shields.io/badge/GitHub_Actions-2088FF?style=for-the-badge&logo=githubactions&logoColor=white)](https://github.com/features/actions)

[![Client: Windows](https://img.shields.io/badge/Client-Windows-0078D4?style=for-the-badge&logo=data:image/svg%2Bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI%2BPHBhdGggZmlsbD0iI2ZmZiIgZD0iTTAgMGgxMS40djExLjRIMHpNMTIuNiAwSDI0djExLjRIMTIuNnpNMCAxMi42aDExLjRWMjRIMHpNMTIuNiAxMi42SDI0VjI0SDEyLjZ6Ii8%2BPC9zdmc%2B)](#downloads)
[![Server: Linux, macOS, Windows](https://img.shields.io/badge/Server-Linux_%7C_macOS_%7C_Windows-FCC624?style=for-the-badge&logo=linux&logoColor=black)](#downloads)

</div>

Warlay pairs a small server you run on a VPS with a Windows desktop client.
Players register the server, create or join rooms in real time, and Warlay
makes the host's game show up in everyone's in-game _Local Area Network_
screen, then carries the real game session either directly between players
(P2P over QUIC) or through the server as a fallback.

## Contents

- [Features](#features)
- [How it works](#how-it-works)
- [Downloads](#downloads)
- [Quick start](#quick-start)
- [Repository layout](#repository-layout)
- [Building from source](#building-from-source)
- [Testing](#testing)
- [Releases](#releases)
- [Contributing](#contributing)
- [License](#license)

## Features

- **Rooms in real time** - create, list, join and password-protect rooms;
  every change is pushed over WebSocket, no polling.
- **Real LAN discovery** - the host's actual W3GS broadcast is captured off
  the wire and replayed on each player's network, so the game lists it exactly
  as if everyone shared a LAN.
- **Game-session tunnel** - direct peer-to-peer QUIC with NAT hole punching,
  falling back automatically to a TCP relay through the server.
- **Secure by default** - per-session X25519 + ChaCha20-Poly1305 encryption on
  the control plane, server-issued client keys, and optional TLS (`wss://`)
  for both the control plane and the game tunnel.
- **Text and voice chat** - room chat, private messages, and Opus voice chat
  between room members.
- **Legacy LAN-mesh mode** - bridge broadcast traffic between isolated subnets
  (for example an Android hotspot running the server on Termux).

## How it works

```mermaid
flowchart LR
    subgraph host["Player A (host)"]
        hostGame["Warcraft III"]
        hostUi["Desktop client<br/>warlay.exe"]
        hostDaemon["Daemon<br/>wnetd.exe"]
    end

    subgraph hub["Warlay server (VPS)"]
        matchmaking["Matchmaking<br/>7777/tcp"]
        tunnel["Game tunnel<br/>7778/tcp"]
        rendezvous["P2P rendezvous<br/>7779/udp"]
        lanRelay["LAN relay<br/>6112/udp"]
    end

    subgraph joiner["Player B (joiner)"]
        joinerGame["Warcraft III"]
        joinerUi["Desktop client<br/>warlay.exe"]
        joinerDaemon["Daemon<br/>wnetd.exe"]
    end

    hostUi <-->|named pipe| hostDaemon
    joinerUi <-->|named pipe| joinerDaemon
    hostGame <-->|LAN broadcast and game TCP| hostDaemon
    joinerGame <-->|LAN broadcast and game TCP| joinerDaemon

    hostDaemon <-->|ws / wss| matchmaking
    joinerDaemon <-->|ws / wss| matchmaking
    hostDaemon -.->|signaling| rendezvous
    joinerDaemon -.->|signaling| rendezvous

    hostDaemon <==>|P2P QUIC, preferred| joinerDaemon
    hostDaemon <-->|TCP relay, fallback| tunnel
    tunnel <-->|TCP relay, fallback| joinerDaemon
```

| Component                       | Runs on                             | Role                                                    |
| ------------------------------- | ----------------------------------- | ------------------------------------------------------- |
| [`server/`](server/README.md)   | VPS (Linux, macOS, Windows, Docker) | Matchmaking hub, game-session relay, P2P rendezvous.    |
| [`daemon/`](daemon/README.md)   | Player's PC (Windows, elevated)     | `wnetd.exe`: LAN capture/replay, game tunnel, P2P mesh. |
| [`desktop/`](desktop/README.md) | Player's PC (Windows)               | `warlay.exe`: the user interface (Tauri 2 + Vue 3).     |

### Network ports (server)

| Port | Protocol | Purpose                                       |
| ---- | -------- | --------------------------------------------- |
| 7777 | TCP      | WebSocket matchmaking control plane (`/ws`)   |
| 7778 | TCP      | Game-session tunnel (relay fallback)          |
| 7779 | UDP      | P2P rendezvous (signaling only, no game data) |
| 6112 | UDP      | LAN discovery relay                           |

Behind a TLS reverse proxy the usual mapping is `443 -> 7777` (`wss://`) and
`7443 -> 7778` (TLS tunnel); see [server/README.md](server/README.md#tls-with-a-reverse-proxy).

### Server addresses

The client accepts three address forms:

| Address                     | Transport                                                                                                  |
| --------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `wss://hub.example.com`     | TLS for matchmaking (port 443) and for the game tunnel (port 7443), certificate verified. **Recommended.** |
| `ws://hub.example.com:7777` | Plain WebSocket and plain game tunnel (port 7778).                                                         |
| `hub.example.com:7777`      | Same as `ws://`, kept for compatibility.                                                                   |

## Downloads

Every release on the [Releases page](https://github.com/jhonsferg/warlay/releases/latest) ships:

| Artifact                                         | Platform                                                                      |
| ------------------------------------------------ | ----------------------------------------------------------------------------- |
| `warlay-desktop-vX.Y.Z-windows-x64-setup.exe`    | Windows installer (client + daemon)                                           |
| `warlay-desktop-vX.Y.Z-windows-x64-portable.zip` | Portable client + daemon                                                      |
| `warlay-daemon-vX.Y.Z-windows-x64.zip`           | Daemon only                                                                   |
| `warlay-server-vX.Y.Z-<target>.tar.gz` / `.zip`  | Server for Linux (static x64/ARM64), macOS (Intel/Apple Silicon), Windows x64 |
| `ghcr.io/jhonsferg/warlay-server:X.Y.Z`          | Server Docker image                                                           |
| `SHA256SUMS.txt`                                 | Checksums for every file                                                      |

## Quick start

### Run a server

```sh
docker volume create warlay-data
docker run -d --name warlay-hub --restart unless-stopped \
  -p 7777:7777/tcp -p 7778:7778/tcp -p 7779:7779/udp -p 6112:6112/udp \
  -v warlay-data:/app/data \
  ghcr.io/jhonsferg/warlay-server:latest
```

Or download a server archive and run `./warlay-server hub --config config/hub.toml`.
Configuration, TLS and deployment details are in [server/README.md](server/README.md).

### Play

1. Install `warlay-desktop-...-setup.exe` (or unzip the portable build and
   run `warlay.exe`). Accept the UAC prompt: the daemon needs administrator
   rights to capture LAN traffic.
2. Register the server (`wss://your-domain` or `host:7777`).
3. Create a room, or join one, and start Warcraft III from the room panel.
4. The host creates the game in _Local Area Network_; it appears for every
   room member.

## Repository layout

```mermaid
flowchart LR
    root["warlay/"]

    root --> crates["crates/<br/>shared Rust libraries"]
    crates --> protocol["protocol<br/>wire formats: control plane,<br/>tunnel handshake, daemon IPC"]
    crates --> crypto["crypto<br/>X25519 handshake, ChaCha20-Poly1305<br/>framing, PSK proofs"]
    crates --> transport["transport<br/>async UDP socket wrapper"]
    crates --> matchmaking["matchmaking<br/>rooms, client sessions,<br/>matchmaking hub"]
    crates --> core["core<br/>UDP relay, TCP tunnel hub,<br/>UDP rendezvous"]
    crates --> storage["storage<br/>SQLite: server identity,<br/>issued client keys"]
    crates --> config["config<br/>TOML settings"]
    crates --> engine["engine<br/>client networking: hub connection,<br/>tunnels, P2P mesh, LAN capture"]
    crates --> facade["warlay<br/>facade re-exporting the crates<br/>as warlay::*, feature-gated"]

    root --> server["server/<br/>hub binary warlay-server,<br/>Dockerfile, sample configs"]
    root --> daemon["daemon/<br/>Windows background agent wnetd.exe"]
    root --> desktop["desktop/<br/>Windows client: Vue 3 frontend<br/>and Tauri 2 backend"]
    root --> github[".github/<br/>CI, release pipeline,<br/>issue and PR templates"]
```

The binaries (`server`, `daemon`, `desktop/src-tauri`) stay thin: protocol,
crypto and networking logic live in `crates/` and are consumed through the
`warlay` facade with only the features each binary needs. The server never
links the Windows-only `engine` crate.

## Building from source

Requirements: Rust stable, Node.js 20+, and on Windows the MSVC build tools.
The server builds on any platform; the daemon and the desktop client need
Windows.

```sh
# Server
cargo build --release -p server            # -> target/release/warlay-server
docker build -t warlay-hub -f server/Dockerfile .

# Desktop client + daemon + installer (Windows)
cd desktop
npm ci
npx tauri build                             # also builds the daemon sidecar
npm run build-portable-zip
```

## Testing

```sh
# Server and shared crates (any OS)
cargo test -p server -p protocol -p crypto -p matchmaking -p core -p storage -p config -p transport

# Whole workspace (Windows; the daemon's test binary requires an elevated shell)
cargo test --workspace

# Frontend type-check and bundle
cd desktop && npm run build

# Live tests against a deployed hub (opt-in)
cargo test -p engine -- --ignored
```

## Releases

Releases are fully automated. `main` is protected, and every pull request
merged into it triggers the [release workflow](.github/workflows/release.yml),
which:

1. computes the next [SemVer](https://semver.org) version from the
   [Conventional Commits](https://www.conventionalcommits.org) since the last
   tag (`feat` -> minor, `fix` and others -> patch, breaking -> major),
2. builds the server for five targets and the Windows client, daemon and
   installer with that version stamped in,
3. pushes the server image to GHCR,
4. tags the commit and publishes a GitHub Release with notes and checksums.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and
our [Code of Conduct](CODE_OF_CONDUCT.md). Report security issues privately as
described in [SECURITY.md](SECURITY.md).

## License

Warlay is released under the [MIT License](LICENSE). Warcraft III is a
trademark of Blizzard Entertainment; this project is not affiliated with or
endorsed by Blizzard.
