# Security Policy

## Supported versions

Only the latest release published on the
[Releases page](https://github.com/jhonsferg/warlay/releases) receives security
fixes. Please upgrade before reporting an issue that may already be fixed.

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/jhonsferg/warlay/security/advisories/new).
Include:

- the affected component (`server`, `daemon`, `desktop`, or a crate under `crates/`)
  and version,
- a description of the issue and its impact,
- steps to reproduce or a proof of concept, if available.

You should receive an acknowledgement within a few days. Once a fix is
released, the advisory will be published with credit to the reporter unless
you ask otherwise.

## Security model

Knowing what Warlay does and does not protect against helps triage reports:

- **Control plane**: every WebSocket session runs an X25519 ephemeral key
  exchange and switches to ChaCha20-Poly1305 with per-direction anti-replay
  counters before any credential is exchanged. Clients authenticate with a
  server-issued pre-shared key, proven via HMAC bound to the session keys.
- **Transport security**: when a hub is registered as `wss://host`, the
  control plane and the TCP game tunnel are additionally wrapped in TLS and the
  hub certificate is verified against the public Web PKI. Plain `host:port`
  addresses are supported for compatibility and offer no protection against an
  active man-in-the-middle.
- **Peer-to-peer tunnel**: direct QUIC connections between players are
  encrypted but peers are not authenticated (there is no PKI for players).
- **Daemon**: runs elevated because Windows requires it for raw LAN capture
  (`SIO_RCVALL`). Its named pipe is restricted by ACL to interactive users,
  `SYSTEM` and Administrators (no remote or anonymous access), and the daemon
  checks the executable of every connecting client process.
