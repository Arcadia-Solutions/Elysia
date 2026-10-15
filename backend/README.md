# Elysia backend

```sh
docker compose up -d db                               # start postgres
cargo run --manifest-path backend/Cargo.toml          # run from repo root (reads ./config.yml)
```

Swagger UI at http://localhost:8090/swagger-ui/, OpenAPI JSON at /api-docs/openapi.json.

Tests need the database running (the `#[sqlx::test]` tests create a throwaway
database each):

```sh
docker compose up -d db
DATABASE_URL=postgres://elysia:password@localhost:5432/elysia \
  cargo test --manifest-path backend/Cargo.toml
```

## System dependencies

The JPEG XL encoder (libjxl) is built from vendored source, so no system libjxl is needed.
The first build is slower and needs cmake, nasm (for the AVIF encoder on x86_64) and a C++ compiler.

## Telemetry

```sh
docker compose --profile telemetry up -d   # Grafana stack (otel-lgtm) + host metrics
```

Set `telemetry.otlp_endpoint: http://localhost:4317` in `config.yml` and restart the backend.
Grafana is at http://localhost:3001 (`admin` / `GF_SECURITY_ADMIN_PASSWORD`, default `elysia`).
Import `opentelemetry/dashboard.json` from Dashboards > New > Import.
