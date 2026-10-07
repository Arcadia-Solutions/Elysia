# Elysia backend

```sh
docker compose up -d db                               # start postgres
cargo run --manifest-path backend/Cargo.toml          # run from repo root (reads ./config.yml)
```

Swagger UI at http://localhost:8080/swagger-ui/, OpenAPI JSON at /api-docs/openapi.json.

Tests need the database running (the `#[sqlx::test]` tests create a throwaway
database each):

```sh
docker compose up -d db
DATABASE_URL=postgres://elysia:password@localhost:5432/elysia \
  cargo test --manifest-path backend/Cargo.toml
```
