# Multi-stage container for the merged Bitblock L1 Bits testnet node (Testnet/Devnet only).
# Build context must be the repo root (`docker build -f node/deploy/Dockerfile .`).
FROM rust:1-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY node/Cargo.toml node/Cargo.lock ./node/
COPY node/crates ./node/crates

RUN cargo build --release --locked --manifest-path node/Cargo.toml -p bitblock-node \
 && strip node/target/release/bitblock-node

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -r -u 10001 -m -d /var/lib/bitblock bitblock

COPY --from=builder /src/node/target/release/bitblock-node /usr/local/bin/bitblock-node

USER bitblock
WORKDIR /var/lib/bitblock

# P2P (libp2p TCP + QUIC/UDP) and HTTP proof-door RPC
EXPOSE 19100/tcp 19100/udp 8645/tcp

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8645/health || exit 1

ENTRYPOINT ["/usr/local/bin/bitblock-node"]
CMD ["run", "--config", "/data/node.json"]
