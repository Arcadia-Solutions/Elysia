FROM rust:1.95-bookworm AS builder
# cmake and nasm build the vendored JPEG XL and AVIF encoders
RUN apt-get update && apt-get install -y cmake nasm && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY .sqlx .sqlx
COPY migrations migrations
COPY src src
RUN --mount=type=cache,id=elysia-target,target=/app/target,sharing=locked \
    --mount=type=cache,id=elysia-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    SQLX_OFFLINE=true cargo build --release --bin elysia && mv target/release/elysia /usr/local/bin

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/elysia /usr/local/bin
# config.yml is mounted here, its relative storage.upload_dir (./data/uploads) follows it
WORKDIR /app
ENTRYPOINT ["/usr/local/bin/elysia"]
