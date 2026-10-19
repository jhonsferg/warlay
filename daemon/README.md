# Warlay daemon (wnetd)

`wnetd.exe` is the Warlay Network Daemon: a Windows background agent that does
everything needing elevated privileges or long-lived sockets, on behalf of the
[desktop client](../desktop/README.md). It has no window; the client talks to
it over a local named pipe.

It also builds and runs on Linux (`wnetd`), with the same requests, handlers
and behavior: the IPC endpoint is a Unix socket instead of the named pipe and
LAN capture uses an `AF_PACKET` socket instead of `SIO_RCVALL` (see
[Linux](#linux)).

## Responsibilities

- **Hub connections** - one encrypted matchmaking connection per registered
  server (`ws://`, `wss://` or legacy `host:port`), with ping tracking and
  reconnect notifications.
- **LAN capture and replay** - captures the host's real Warcraft III LAN
  broadcast with a raw `SIO_RCVALL` socket and replays relayed broadcasts on
  joiners' networks.
- **Game-session tunnel** - P2P QUIC mesh between room members with NAT hole
  punching, and the TCP tunnel through the hub (TLS-wrapped for `wss://`
  servers) as fallback.
- **Voice relay** - forwards Opus voice frames between the client and the
  P2P mesh.
- **Game launch** - starts Warcraft III with the user's configured
  executable.

The networking logic lives in [`crates/engine`](../crates/engine); the daemon
wires it to the IPC protocol defined in
[`crates/protocol/src/ipc.rs`](../crates/protocol/src/ipc.rs).

## Source layout

```mermaid
flowchart LR
    src["daemon/src/"]
    src --> main["main.rs<br/>entry point, --install and --uninstall,<br/>pipe server bootstrap"]
    src --> ipc["ipc_server.rs<br/>named pipe server: JSON requests,<br/>responses and pushed events"]
    src --> handlers["handlers.rs<br/>one handler per DaemonRequest"]
    src --> lan["lan.rs<br/>LAN capture and replay<br/>for the active room"]
    src --> state["state.rs<br/>shared state: connections,<br/>tunnel, mesh"]
    src --> security["security.rs<br/>pipe ACL and connecting<br/>process verification (Windows)"]
    src --> unix["unix_socket.rs<br/>Unix socket server and<br/>peer-credential check (Linux)"]
    src --> paths["paths.rs<br/>data and log directory<br/>per platform"]
    src --> storage["storage.rs<br/>daemon settings in SQLite"]
    src --> autostart["autostart.rs<br/>logon Scheduled Task registration (Windows)"]
    src --> logging["logging.rs and log_control.rs<br/>rotating file logs and<br/>runtime log levels"]
```

## Usage

The desktop client starts the daemon (elevated) every time it launches, so
most players never run it by hand. To start it at logon instead:

```sh
wnetd.exe              # run the agent (asks for elevation)
wnetd.exe --install    # register auto-start at logon (run as administrator)
wnetd.exe --uninstall  # remove the auto-start task
```

Logs are written to `%APPDATA%\com.warlay.lanlauncher\logs\`.

## Security

The daemon must run as administrator because Windows only allows
`SIO_RCVALL` raw capture to elevated processes. To keep that privilege from
being abused, the pipe is created with an ACL that only admits interactive
users, `SYSTEM` and Administrators, and every connecting process's
executable is verified before requests are accepted (`security.rs`).

## Building

```sh
cargo build --release -p daemon   # -> target/release/wnetd.exe
```

Release builds use the Windows GUI subsystem (no console window) and embed a
manifest requesting elevation (`build.rs`). For a client build, run
`npm run prepare-daemon-sidecar` in `desktop/`, which builds the daemon and
copies it where Tauri bundles it.

## Linux

```sh
cargo build --release -p daemon   # -> target/release/wnetd
sudo ./target/release/wnetd       # or grant it CAP_NET_RAW instead of root
```

| | Windows | Linux |
| --- | --- | --- |
| IPC endpoint | named pipe `\\.\pipe\warlay-daemon` | Unix socket: `$WARLAY_DAEMON_SOCKET`, else `$XDG_RUNTIME_DIR/warlay-daemon.sock`, else `/tmp/warlay-daemon-<uid>.sock` |
| Access control | pipe ACL + client executable check | socket file mode `0600` + `SO_PEERCRED` (same user or root) |
| LAN capture | raw socket in `SIO_RCVALL` mode (Administrator) | `AF_PACKET` socket tapping outgoing frames (`CAP_NET_RAW`) |
| Data and logs | `%APPDATA%\com.warlay.lanlauncher\` | `$WARLAY_DATA_DIR`, else `$XDG_DATA_HOME/warlay`, else `~/.local/share/warlay` |
| Shutdown | Ctrl+C | SIGINT or SIGTERM (`docker stop`, systemd) |
| `--install` | logon Scheduled Task | not used: run it from a systemd unit |

The protocol is the same newline-delimited JSON
([`crates/protocol/src/ipc.rs`](../crates/protocol/src/ipc.rs)), so any
client that speaks it can drive either build.

## Tests

```sh
cargo test -p daemon    # from an elevated shell: the test binary carries the elevation manifest
```
