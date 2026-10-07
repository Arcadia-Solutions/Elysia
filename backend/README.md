# Elysia backend

```sh
docker compose up -d db                               # start postgres
cargo run --manifest-path backend/Cargo.toml          # run from repo root (reads ./config.yml)
```

Swagger UI at http://localhost:8080/swagger-ui/, OpenAPI JSON at /api-docs/openapi.json.
