# syntax=docker/dockerfile:1

# Multi-stage build of the Warlay hub. cargo-chef caches dependency
# compilation in its own layer (rebuilt only when Cargo.toml/Cargo.lock
# change); the runtime image is distroless and ships only the stripped
# release binary and its default config.
#
# `server` is a member of the root Cargo workspace and depends on crates/*
# via path dependencies -- build from the REPO ROOT:
#   docker build -t warlay-hub -f server/Dockerfile .

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
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=builder /out/warlay-server /usr/local/bin/warlay-server
COPY --chown=65532:65532 server/config/hub.toml server/config/main.toml server/config/helper.toml /app/config/
COPY --from=builder --chown=65532:65532 /out/data /app/data

# main: 8080/tcp REST + realtime WebSocket.
# helper: 7779/udp P2P rendezvous, 7780/udp last-resort relay.
# hub (legacy): 7777/tcp matchmaking, 7778/tcp tunnel, 6112/udp LAN relay.
EXPOSE 8080/tcp 7779/udp 7780/udp 7777/tcp 7778/tcp 6112/udp

# warlay.db (server identity + issued client keys) -- mount a named volume
# so recreating the container doesn't force every client to re-register.
VOLUME ["/app/data"]

ENTRYPOINT ["/usr/local/bin/warlay-server"]
# Run the main server with `main --config /app/config/main.toml` (needs
# PostgreSQL + Redis, see WARLAY_DATABASE_URL/WARLAY_REDIS_URL/
# WARLAY_HELPER_SECRET) and each helper with
# `helper --config /app/config/helper.toml`.
CMD ["hub", "--config", "/app/config/hub.toml"]
