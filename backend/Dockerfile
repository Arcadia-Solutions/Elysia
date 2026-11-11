FROM rust:1.95-trixie AS chef
# cmake and nasm build the vendored JPEG XL and AVIF encoders, libdav1d decodes AVIF (needs >= 1.3)
RUN apt-get update && apt-get install -y cmake nasm pkg-config libdav1d-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# dependencies get their own layer, only rebuilt when Cargo.toml/Cargo.lock change
FROM chef AS builder
# e.g. `-C target-cpu=native` when the image is built on the host that runs it
ARG RUSTFLAGS
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY .sqlx .sqlx
COPY migrations migrations
COPY src src
RUN SQLX_OFFLINE=true cargo build --release --bin elysia

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates libdav1d7 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/elysia /usr/local/bin
# config.yml is mounted here, its relative storage.upload_dir (./data/uploads) follows it
WORKDIR /app
ENTRYPOINT ["/usr/local/bin/elysia"]
