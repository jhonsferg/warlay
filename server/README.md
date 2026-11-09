# Warlay server

`warlay-server` is the backend the Warlay desktop client talks to. It is one
binary with four modes:

- **`main`** - the main server: accounts, friends, communities, rooms, chat
  and notifications, over a REST API and a realtime WebSocket. Needs
  PostgreSQL and Redis. It carries no game traffic.
- **`helper`** - a helper: ticket-gated UDP rendezvous (so players can open a
  direct P2P link) plus the last-resort UDP relay. No database and no social
  logic; optionally it also serves the realtime socket and chat as an "edge".
- **`hub`** (legacy) - the previous all-in-one matchmaking server over
  WebSocket with a TCP game tunnel. The current desktop client does not use it.
- **`lan-mesh`** (legacy) - broadcast-reflection mesh between physically
  isolated LAN subnets (for example a Termux instance on an Android hotspot),
  with statically configured peers and no central server.

## Contents

- [Running](#running)
- [Configuration](#configuration)
- [Deploying with Docker](#deploying-with-docker)
- [Replacing a container without cutting games](#replacing-a-container-without-cutting-games)
- [TLS with a reverse proxy](#tls-with-a-reverse-proxy)
- [Architecture](#architecture)
- [Security](#security)
- [Legacy hub mode](#legacy-hub-mode)
- [LAN-mesh mode](#lan-mesh-mode)
- [Tests](#tests)

## Running

Download the archive for your platform from the
[latest release](https://github.com/jhonsferg/warlay/releases/latest), or
build it yourself with `cargo build --release -p server`.

```sh
# main server (PostgreSQL and Redis must be reachable)
./warlay-server main --config config/main.toml

# a helper, on this or another machine
./warlay-server helper --config config/helper.toml
```

For a single-VPS setup, enable the `[embedded_helper]` block of `main.toml`:
the helper then runs inside the main server's process and registers itself,
so there is one thing to run and no `helper_secret` to share.

Open these ports in your firewall:

| Port | Protocol | Service                | Purpose                                         |
| ---- | -------- | ---------------------- | ----------------------------------------------- |
| 8080 | TCP      | main                   | REST API and realtime WebSocket (`/ws`)         |
| 7779 | UDP      | helper                 | P2P rendezvous (signaling only)                 |
| 7780 | UDP      | helper                 | Relay used when a direct link is not possible   |
| 8090 | TCP      | helper (optional edge) | Realtime socket and chat served from the helper |

The addresses players use to reach a helper are the `public_rendezvous_addr`
and `public_relay_addr` it announces to the main server, not its bind
addresses, so they must be reachable from the internet.

## Configuration

`config/main.toml` (shipped in every archive and in the main image):

```toml
[main]
http_addr = "0.0.0.0:8080"
database_url = "postgres://warlay:CHANGE_ME@postgres:5432/warlay"
redis_url = "redis://redis:6379"
# Shared with every helper server. Generate with: openssl rand -hex 32
helper_secret = ""
access_token_ttl_secs = 900
refresh_token_ttl_days = 30
room_ticket_ttl_secs = 43200
cors_origins = []                 # empty: any origin (the desktop webview)
# client_ip_header = "x-real-ip"  # real client address behind a proxy (per-IP limits)
disconnect_grace_secs = 45        # a player offline this long leaves the room
host_absence_close_secs = 86400   # a room whose host never returns is closed

# Uncomment to also run a helper inside the main server (single-VPS setup).
# [embedded_helper]
# name = "main-vps"
# region = "us-east"
# rendezvous_bind = "0.0.0.0:7779"
# relay_bind = "0.0.0.0:7780"
# public_rendezvous_addr = "warlay.example.com:7779"
# public_relay_addr = "warlay.example.com:7780"

[observability]
log_level = "info"                # "debug" to trace rooms and helpers
```

`config/helper.toml`:

```toml
[helper]
name = "helper-sa-1"              # keep it stable: the identity derives from it
region = "sa-east"
rendezvous_bind = "0.0.0.0:7779"
relay_bind = "0.0.0.0:7780"
public_rendezvous_addr = "helper1.example.com:7779"
public_relay_addr = "helper1.example.com:7780"
main_url = "https://warlay.example.com"
helper_secret = ""                # must equal the main server's helper_secret

[observability]
log_level = "info"
```

Secrets can come from the environment instead of the file; see the variable
table under
[Main server and helpers as separate containers](#main-server-and-helpers-as-separate-containers).
Run `warlay-server <mode> --help` for the command-line flags.

## Deploying with Docker

The image is built from the **repository root** (the server depends on the
workspace crates), with one stage per role:

```sh
docker build --target main   -t warlay-main   -f server/Dockerfile .
docker build --target helper -t warlay-helper -f server/Dockerfile .
```

The [`Dockerfile`](Dockerfile) uses `cargo-chef` to cache dependency builds
and a distroless, non-root runtime image that contains only the stripped
binary and the default config. A plain `docker build` without `--target`
produces the legacy `hub`, the last stage; that is also what the
`ghcr.io/jhonsferg/warlay-server` image published with each release contains.

To ship a locally built image to a server without a registry:

```sh
docker save warlay-main | gzip | ssh my-server 'gunzip | docker load'
```

Build on a machine with more RAM and cores than a small VPS if you can; a
release build of the workspace needs well over 1 GiB of memory.

For a complete single-server deployment with nginx and TLS, follow the
[VPS guide](../README.md#deploying-the-server-on-a-vps).

### Main server and helpers as separate containers

The main server and each helper run in their own container and scale
independently (images built as shown above).

The main server is stateless (everything lives in PostgreSQL and Redis), so
it can run as several replicas behind a load balancer:

```sh
docker run -d --name warlay-main --restart unless-stopped -p 8080:8080 \
  -e WARLAY_DATABASE_URL=postgres://warlay:secret@db:5432/warlay \
  -e WARLAY_REDIS_URL=redis://redis:6379 \
  -e WARLAY_HELPER_SECRET=<openssl rand -hex 32> \
  warlay-main
```

A helper is configured entirely through the environment, so one image
serves any number of them. Setting `WARLAY_EDGE_PUBLIC_URL` also makes it an
edge (players' realtime socket and chat), which needs the shared PostgreSQL
and Redis:

```sh
docker run -d --name warlay-helper-1 --restart unless-stopped \
  -p 7779:7779/udp -p 7780:7780/udp -p 8090:8090/tcp \
  -e WARLAY_HELPER_SECRET=<same as the main server> \
  -e WARLAY_HELPER_NAME=helper-1 -e WARLAY_HELPER_REGION=sa-east \
  -e WARLAY_MAIN_URL=https://warlay.example.com \
  -e WARLAY_PUBLIC_RENDEZVOUS_ADDR=helper1.example.com:7779 \
  -e WARLAY_PUBLIC_RELAY_ADDR=helper1.example.com:7780 \
  -e WARLAY_EDGE_PUBLIC_URL=https://helper1.example.com \
  -e WARLAY_DATABASE_URL=postgres://warlay:secret@db:5432/warlay \
  -e WARLAY_REDIS_URL=redis://redis:6379 \
  warlay-helper
```

| Variable                        | Role         | Overrides                                                             |
| ------------------------------- | ------------ | --------------------------------------------------------------------- |
| `WARLAY_DATABASE_URL`           | main, edge   | `main.database_url`, `helper.edge.database_url`                       |
| `WARLAY_REDIS_URL`              | main, edge   | `main.redis_url`, `helper.edge.redis_url`                             |
| `WARLAY_HELPER_SECRET`          | main, helper | `main.helper_secret`, `helper.helper_secret`                          |
| `WARLAY_HELPER_NAME`            | helper       | `helper.name` (keep it stable: the helper's identity derives from it) |
| `WARLAY_HELPER_REGION`          | helper       | `helper.region`                                                       |
| `WARLAY_MAIN_URL`               | helper       | `helper.main_url`                                                     |
| `WARLAY_PUBLIC_RENDEZVOUS_ADDR` | helper       | `helper.public_rendezvous_addr`                                       |
| `WARLAY_PUBLIC_RELAY_ADDR`      | helper       | `helper.public_relay_addr`                                            |
| `WARLAY_EDGE_PUBLIC_URL`        | helper       | `helper.edge.public_url` (turns the edge on)                          |

## Replacing a container without cutting games

The servers keep no state that a game depends on, so a container can be
replaced while players are in a match:

- **Main server** (REST + realtime): sessions, rooms and chat live in PostgreSQL
  and Redis, which must keep running (and keep their volumes) across the
  replacement. Clients reconnect their socket on their own; a player is only
  removed from a room after `disconnect_grace_secs` (45 s) offline and a host
  never is. A desktop client signs the user out only when the server rejects
  the refresh token, never because the server was unreachable or answered 5xx.
- **Helper** (rendezvous + UDP relay): games between players with a direct link
  do not touch the helper and are not affected at all. Relayed games stall while
  the helper is down and carry on when it is back (the relay channel ids are
  derived from the room and the two players, so they survive a restart; clients
  rebind every 3 s and relayed links tolerate 45 s of silence). Measured in the
  lab: a 25 s helper outage paused a relayed game for as long and the match went
  on without a reconnect.
- Both servers stop on `SIGTERM` (what `docker stop` and a recreate send), so a
  replacement does not wait for Docker's kill timeout.
- Clients older than this change keep the old relay behavior: for them a helper
  restart still cuts a relayed game, so update the clients before relying on it.

## TLS with a reverse proxy

The servers speak plain HTTP and UDP; terminate TLS in front of the main
server so the client is built for an `https://` address:

| Public port | Forward to          | Protocol                                   |
| ----------- | ------------------- | ------------------------------------------ |
| 443         | `127.0.0.1:8080`    | HTTPS and WebSocket upgrade (`/ws`)        |
| 7779, 7780  | the helper directly | UDP (a proxy is not involved)              |
| 443         | `127.0.0.1:8090`    | HTTPS, only for a helper that runs an edge |

Publish 8080 on loopback only (`-p 127.0.0.1:8080:8080/tcp`) so all REST and
realtime traffic goes through the proxy. Example with nginx and a Let's
Encrypt certificate (the `map` goes in the `http` context):

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl;
    server_name warlay.example.com;

    ssl_certificate     /etc/letsencrypt/live/warlay.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/warlay.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_read_timeout 1h;
        proxy_send_timeout 1h;
    }
}
```

When certbot renews the certificate it reloads nginx and the listener picks
up the new certificate without dropping active connections.

## Architecture

```mermaid
flowchart LR
    src["server/src/"]
    src --> main["main.rs<br/>bootstrap and wiring"]
    src --> cli["cli.rs<br/>clap subcommands:<br/>main, helper, hub, lan-mesh"]
    src --> mainserver["main_server.rs<br/>main mode: config, environment,<br/>optional embedded helper"]
    src --> helper["helper.rs<br/>helper mode: rendezvous, relay,<br/>optional edge, heartbeats"]
    src --> shutdown["shutdown.rs<br/>SIGTERM / Ctrl-C"]
    src --> udp["udp.rs<br/>UDP socket buffers"]
    src --> ws["ws_server.rs<br/>legacy hub: axum WebSocket glue"]
    src --> logging["logging.rs<br/>tracing setup"]
```

Everything else lives in the shared crates (see the
[repository layout](../README.md#repository-layout)):

- **`social`** - the main server: the REST router and realtime WebSocket
  (`/ws`), accounts and sessions, friends, communities, rooms, invites, chat,
  notifications and the helper registry, on PostgreSQL (schema migrated on
  start) and Redis (sessions, rate limits, realtime fan-out).
- **`core`** - the data-plane engines: `UdpRendezvous` (P2P signaling) and
  `UdpRelay` (relay), both ticket-gated, plus the legacy hub's `RelayHub` and
  `TcpTunnelHub` and lan-mesh's `PeerRegistry`, `LoopGuard`, `RelayEngine` and
  `ControlResponder`.
- **`protocol`** - wire formats: rendezvous and relay messages, `RoomTicket`,
  `HelperHeartbeat`, plus the legacy hub and lan-mesh formats.
- **`crypto`**, **`storage`**, **`config`**, **`transport`**, **`matchmaking`**
  - shared building blocks; the last two crates serve the legacy `hub`.

### How a game session is set up

1. A player creates or joins a room through the main server's REST API.
2. The main server answers with a **room ticket** (signed with the shared
   `helper_secret`, valid `room_ticket_ttl_secs`) and the addresses of the
   helpers that are alive, which report to it with a heartbeat every few
   seconds.
3. The client registers with a helper's **rendezvous** using the ticket; the
   helper replies with the other members' public endpoints so they can try a
   direct QUIC connection (NAT hole punching). It never carries game bytes.
4. When no direct link is possible, the client binds a **relay** channel with
   the same ticket and the helper forwards opaque datagrams between the
   members. The channel id is derived from the room and the two players, so it
   survives a helper restart.

The UDP rendezvous and relay sockets ask the kernel for 4 MiB receive and
send buffers (logged as `UDP socket bound`, with the size actually granted).
Linux caps them at `net.core.rmem_max` and `net.core.wmem_max`; raise both to
4194304 (`sysctl -w`) so a burst from many rooms is not dropped.

## Security

- **Accounts.** Passwords are hashed with Argon2id on a dedicated thread
  pool; an unknown account takes as long to refuse as a known one, and when
  the pool is saturated login is refused with 503 instead of queueing without
  bound. Logins and other sensitive calls are rate limited per window in
  Redis, per account and per client address. Behind a reverse proxy set
  `client_ip_header` (for example `x-real-ip`) or every player will share the
  proxy's address; leave it empty when clients connect directly.
- **Sessions.** Access and refresh tokens are 32 random bytes (URL-safe
  base64), stored only as their SHA-256; access tokens live in Redis for
  `access_token_ttl_secs` and are sent as `Authorization: Bearer`. A client
  signs the user out only when the server rejects the refresh token, never
  because the server was unreachable or answered 5xx.
- **Private chat.** Direct messages are end-to-end encrypted between clients
  (X25519 + ChaCha20-Poly1305, identity keys published at `/me/e2e-key`); the
  server stores and relays ciphertext only.
- **Room tickets.** Helpers accept only players who present a ticket signed
  with `helper_secret` and not expired, so the secret must be long and random
  (`openssl rand -hex 32`) and shared only with your helpers.
- **Transport.** Deploy the main server behind TLS (see above) and build the
  client for an `https://` address; over plain `http://` the API traffic is
  not encrypted in transit. P2P links are encrypted QUIC but peers are not
  authenticated to each other.

## Legacy hub mode

The `hub` mode is the previous all-in-one server: centralized matchmaking over
WebSocket, a TCP tunnel for the game session, P2P rendezvous and a
LAN-discovery relay, with SQLite for its identity. The current desktop client
does not connect to it; it stays for existing deployments and for
`cargo test -p matchmaking`. Its ports are 7777/tcp (control plane `/ws`),
7778/tcp (tunnel), 7779/udp (rendezvous) and 6112/udp (LAN relay).

```sh
./warlay-server hub --config config/hub.toml

# or with flags only
./warlay-server hub --ws-addr 0.0.0.0:7777 --relay-addr 0.0.0.0:6112 \
  --tcp-tunnel-addr 0.0.0.0:7778 --name "My Warlay hub"
```

Open these ports in your firewall:

| Port | Protocol | Purpose                                             |
| ---- | -------- | --------------------------------------------------- |
| 7777 | TCP      | WebSocket matchmaking control plane (`/ws`)         |
| 7778 | TCP      | Game-session tunnel (relay fallback when P2P fails) |
| 7779 | UDP      | P2P rendezvous (signaling only)                     |
| 6112 | UDP      | LAN discovery relay                                 |

The client derives the tunnel and rendezvous addresses from the host it was
given, so 7778 and 7779 must be reachable on the same host as the
matchmaking endpoint.

`config/hub.toml`:

```toml
server_name = "Warlay VPS #1"          # shown to players in the client

[hub]
ws_addr = "0.0.0.0:7777"
relay_addr = "0.0.0.0:6112"
tcp_tunnel_addr = "0.0.0.0:7778"
rendezvous_udp_addr = "0.0.0.0:7779"
membership_ttl_secs = 30               # prune silent UDP relay members
data_dir = "/app/data"                 # warlay.db lives here
client_inactivity_purge_days = 30      # forget clients that never come back
room_inactivity_purge_minutes = 15     # drop rooms with nobody connected

[security]
# password = "changeme"                # require a password on first connection

[observability]
log_level = "info"                     # "debug" to trace tunnel pairing and relays
```

Run `warlay-server hub --help` for the equivalent command-line flags.

### Listeners

- **WebSocket control plane** (`ws_addr`, path `/ws`) - after the handshake
  (see [Connection security](#connection-security)) the client authenticates
  and sends `createRoom`, `joinRoom`, `leaveRoom`, chat messages, `ping`, and
  so on. The server pushes `roomList`, `roomCreated`, `roomUpdated` and
  `roomRemoved` to every client in real time.
- **TCP game-session tunnel** (`tcp_tunnel_addr`) - carries the real WC3
  game connection when P2P is not possible. Each connection starts with a
  length-prefixed `TunnelHello`:
  - `HostControl { room_token }` - the host keeps one open per room as a
    signaling channel.
  - `JoinerStream { room_token }` - one per WC3 connection on a joiner.
  - `HostBackend { room_token }` - opened by the host after reading a `0x01`
    byte on its control channel ("a joiner is waiting"). The hub pairs it
    FIFO with the oldest waiting `JoinerStream` and copies bytes both ways,
    without knowing anything about the game protocol.
- **UDP rendezvous** (`rendezvous_udp_addr`) - room members register their
  observed public endpoint; the hub replies with the other members' endpoints
  so they can attempt a direct QUIC connection (NAT hole punching). It never
  carries game bytes, and its failure only means sessions fall back to the
  TCP tunnel.
- **UDP buffers** - the rendezvous and relay sockets ask the kernel for 4 MiB
  receive and send buffers (logged as `UDP socket bound`, with the size
  actually granted). Linux caps them at `net.core.rmem_max` and
  `net.core.wmem_max`; raise both to 4194304 (`sysctl -w`) so a burst from many
  rooms is not dropped by the kernel.
- **UDP relay** (`relay_addr`) - fans `RelayFrame { room_token, payload }`
  datagrams out to the other members of a room, as unicast. Because nothing is
  broadcast there is no self-echo risk, so hub mode needs no `LoopGuard`.

### Connection security

Every WebSocket connection runs a handshake before any `ClientMessage` is
processed:

1. Client sends plaintext `HandshakeHello { client_id, client_ephemeral_pubkey }`.
2. Server replies `HandshakeWelcome { server_id, server_name, requires_password, server_ephemeral_pubkey }`.
3. Both sides run X25519 ECDH and derive, with HKDF, a fresh pair of
   directional ChaCha20-Poly1305 keys. From here every frame is binary
   ciphertext; nonces are per-direction strictly increasing counters, which
   rejects replays. Keys are discarded when the connection closes.
4. The client sends one encrypted `AuthRequest`:
   - `Fresh` - no stored key, and the server needs no password.
   - `Password { password }` - no stored key; only ever sent inside the
     encrypted channel.
   - `Proof { hmac }` - proves possession of a previously issued key with
     `HMAC-SHA256(psk, client_pubkey || server_pubkey)`. Binding the proof to
     this session's ephemeral keys stops it from being replayed elsewhere.
5. The server answers `AuthResponse::Accepted { psk }` (a new key on
   first-time auth) or `Rejected { message, retry_as_new }`. `retry_as_new`
   lets a client whose key was purged re-register without asking the user for
   anything unless a password is required.

**Threat model.** This protects against passive eavesdroppers and gives
forward secrecy per connection. On its own it is not a PAKE and does not
authenticate the server, so it does not stop an active man-in-the-middle
during the key exchange. Deploy behind TLS (see
a TLS reverse proxy) and have players use a
`wss://` address to close that gap: the client then verifies the server
certificate for both the control plane and the game tunnel.

### Persistence

`data_dir` holds `warlay.db` (SQLite, compiled in): the server's own
`server_id`, generated on first boot, and every client's issued key, indexed
by the client's persistent `client_id`. A background task purges clients not
seen for `client_inactivity_purge_days` every hour; a purged client's next
connection looks like a first-time one.

## LAN-mesh mode

```sh
./warlay-server lan-mesh --bind 0.0.0.0:6112 --control 0.0.0.0:6116 \
  --peer <peer-ip>:6112 --name laptop-bridge
# or
./warlay-server lan-mesh --config config/default.toml
```

Each node relays only what it captured locally to every configured peer, and
never re-forwards inbound peer traffic, which keeps an N-node mesh loop-free.
`LoopGuard` fingerprints self-injected payloads for a short TTL to suppress
the one real loop: the OS delivering a node's own re-broadcast back to its
own socket.

On Termux (Android, ARM64):

```sh
pkg install rust
cargo build --release -p server
./target/release/warlay-server lan-mesh --config server/config/default.toml
```

Grant Termux a wake lock (`termux-wake-lock`) and disable battery
optimizations so the bridge keeps running with the screen off.

## Scope and limitations

- **lan-mesh** solves broadcast discovery across subnets where unicast routing
  already works and only broadcast is blocked (AP client isolation, routed
  subnets). It does not proxy the game's TCP session; that would need a
  TUN/TAP interface, which is out of scope for a userspace relay.
- **main + helper** (the current design) solve accounts, discovery and the
  game session for Warcraft III: the client captures the host's real LAN
  broadcast and replays it for the other members, and the game connection
  travels over P2P QUIC or the helper's UDP relay.

## Tests

```sh
cargo test -p server -p social -p protocol -p crypto -p matchmaking -p core -p storage -p config -p transport
```
