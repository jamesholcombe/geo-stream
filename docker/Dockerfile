# Build stage
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p cli --bin geo-stream

# Runtime: small Debian image with glibc (dynamic link of release binary)
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /
COPY --from=builder /app/target/release/geo-stream /usr/local/bin/geo-stream
ENTRYPOINT ["/usr/local/bin/geo-stream"]
