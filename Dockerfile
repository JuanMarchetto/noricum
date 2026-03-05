# Stage 1: Build
FROM rust:latest AS builder

WORKDIR /usr/src/noricum
COPY . .

RUN cargo build --release --workspace

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends gcc libc6-dev && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/noricum/target/release/noricum-cli /usr/local/bin/noricum
COPY --from=builder /usr/src/noricum/target/release/noricum-mcp-server /usr/local/bin/noricum-mcp-server

ENTRYPOINT ["noricum"]
