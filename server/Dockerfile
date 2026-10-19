# syntax=docker/dockerfile:1

# Multi-stage build of the Warlay server. cargo-chef caches dependency
# compilation in its own layer (rebuilt only when Cargo.toml/Cargo.lock
# change); the runtime images are distroless and ship only the stripped
# release binary and their default config.
#
# One binary, one image per role, so each role runs in its own container:
#   docker build --target main   -t warlay-main   -f server/Dockerfile .
#   docker build --target helper -t warlay-helper -f server/Dockerfile .
#   docker build                 -t warlay-hub    -f server/Dockerfile .   (legacy hub, default)
#
# `server` is a member of the root Cargo workspace and depends on crates/*
# via path dependencies -- build from the REPO ROOT.

FROM rust:1-slim-bookworm AS chef
# rusqlite's "bundled" feature compiles SQLite from source.
RUN apt-get update && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p server
COPY . .
RUN cargo build --release -p server \
    && mkdir -p /out/data \
    && cp target/release/warlay-server /out/warlay-server

# distroless/cc: glibc + libgcc + CA certificates, no shell, runs as
# nonroot (uid 65532).
FROM gcr.io/distroless/cc-debian12:nonroot AS base
WORKDIR /app
COPY --from=builder /out/warlay-server /usr/local/bin/warlay-server
ENTRYPOINT ["/usr/local/bin/warlay-server"]

# Main server: accounts, friends, communities, rooms, chat. Needs
# PostgreSQL + Redis (WARLAY_DATABASE_URL, WARLAY_REDIS_URL,
# WARLAY_HELPER_SECRET). Stateless: scale it horizontally behind a balancer.
FROM base AS main
COPY --chown=65532:65532 server/config/main.toml /app/config/main.toml
# 8080/tcp REST + realtime WebSocket.
EXPOSE 8080/tcp
CMD ["main", "--config", "/app/config/main.toml"]

# Helper: P2P rendezvous + last-resort relay, and optionally a realtime/chat
# edge. Configure it with a mounted TOML or the WARLAY_HELPER_* variables
# (see server/README.md).
FROM base AS helper
COPY --chown=65532:65532 server/config/helper.toml /app/config/helper.toml
# 7779/udp rendezvous, 7780/udp relay, 8090/tcp edge (when enabled).
EXPOSE 7779/udp 7780/udp 8090/tcp
CMD ["helper", "--config", "/app/config/helper.toml"]

# Legacy all-in-one hub. Kept as the last stage so a plain `docker build`
# still produces it.
FROM base AS hub
COPY --chown=65532:65532 server/config/hub.toml /app/config/hub.toml
COPY --from=builder --chown=65532:65532 /out/data /app/data
# 7777/tcp matchmaking, 7778/tcp tunnel, 7779/udp rendezvous, 6112/udp LAN relay.
EXPOSE 7777/tcp 7778/tcp 7779/udp 6112/udp
# warlay.db (server identity + issued client keys) -- mount a named volume
# so recreating the container doesn't force every client to re-register.
VOLUME ["/app/data"]
CMD ["hub", "--config", "/app/config/hub.toml"]
