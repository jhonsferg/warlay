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

- the affected component (`server`, `desktop`, or a crate under `crates/`)
  and version,
- a description of the issue and its impact,
- steps to reproduce or a proof of concept, if available.

You should receive an acknowledgement within a few days. Once a fix is
released, the advisory will be published with credit to the reporter unless
you ask otherwise.

## Security model

Knowing what Warlay does and does not protect against helps triage reports:

- **Accounts and sessions**: passwords are hashed with Argon2id; access and
  refresh tokens are random 32-byte values stored server-side only as their
  SHA-256, and logins are rate limited. Access tokens are short lived and are
  sent as a bearer token to the REST API and the realtime WebSocket.
- **Transport security**: the client is built for one server address. With an
  `https://` address the API and the realtime socket are protected by TLS and
  the server certificate is verified against the public Web PKI. A plain
  `http://` address is supported for private tests and offers no protection
  against an eavesdropper or an active man-in-the-middle.
- **Account recovery**: without e-mail, through one-time recovery codes
  (8 per set, 50 bits each, only their hashes are stored). Using one sets a
  new password and signs the account out everywhere; the endpoint is rate
  limited per address and per account.
- **Private chat**: direct messages are end-to-end encrypted between clients
  (X25519 key agreement, HKDF-SHA256 and ChaCha20-Poly1305); the server
  stores and relays ciphertext. Safety numbers let users verify each other's
  keys. Room chat is not end-to-end encrypted.
- **Helpers**: the rendezvous and the UDP relay only serve players holding a
  room ticket signed by the main server with the shared `helper_secret` (keep
  it long, random and private). The relay forwards opaque datagrams.
- **Peer-to-peer tunnel**: direct QUIC connections between players are
  encrypted but peers are not authenticated (there is no PKI for players).
- **Self-update**: the client only downloads installers published by this
  repository's releases and verifies them against the release's
  `SHA256SUMS.txt` before running them.
- **Legacy `hub` mode** (not used by the current client): its WebSocket
  sessions use an X25519 exchange and ChaCha20-Poly1305 with a server-issued
  pre-shared key, which does not authenticate the server on its own; use TLS.
- **Desktop client**: runs elevated because Windows requires it for raw LAN
  capture (`SIO_RCVALL`). It exposes no local service, pipe or listening
  socket for other programs to talk to: the UI and the network code run in
  the same process.
