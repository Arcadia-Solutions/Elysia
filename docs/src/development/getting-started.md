# Getting started

## Requirements

- Rust (the toolchain is pinned in `backend/rust-toolchain`)
- Node.js 22
- Docker, for PostgreSQL
- cmake, nasm and a C++ compiler: the JPEG XL encoder is built from vendored
  source and the AVIF encoder needs nasm on x86_64. The first build is slow.

## Running locally

From the repository root:

```sh
cp config.example.yml config.yml        # edit what's needed in this file
docker compose up -d db
cargo run --manifest-path backend/Cargo.toml
```

In another terminal:

```sh
cd frontend
npm ci
npm run dev
```

The UI is on <http://localhost:5174>, the Vite dev server proxies the API and
images to the backend on port 8090. Log in as `admin` / `change-me-123`.

Image encoding is very slow in debug builds (seconds per image for AVIF and
JPEG XL). Use `cargo run --release` when uploading many images.

The database holds nothing important in development: reset it with
`cargo sqlx database reset --force` and `rm -r data/uploads`.

## Test data

Admins can generate users and images from Browse > "Generate fixtures". It
creates the requested number of users (`alice`, `bob`, ..., then `alice2`, ...)
with the given password, and uploads generated images through the real upload
pipeline, spread over the requested number of past days. It adds to what is
already there. Only debug builds of the API (`cargo run`) allow it: release
builds, like the Docker image, refuse it and the button stays disabled.

## Database changes

The schema lives in a single migration, `backend/migrations/0001_init.sql`,
edited in place. Queries are
checked at compile time against the cache in `backend/.sqlx`, so after changing
a query or the schema, with the database running:

```sh
cd backend
cargo sqlx database reset --force
cargo sqlx prepare
```

## API client

The frontend's API client in `frontend/src/services/api-schema` is generated
from the backend's OpenAPI description, never edit it by hand. With the backend
running:

```sh
cd frontend
npm run gen:api
```

## Checks

These run in CI:

```sh
# backend
cargo clippy
cargo fmt --all -- --check
# frontend
npm run lint
npm run type-check
npm run check-format
```
