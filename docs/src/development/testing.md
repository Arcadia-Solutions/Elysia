# Testing

## Backend

Tests live in `backend/tests/` and need PostgreSQL: each `#[sqlx::test]` creates
and drops its own throwaway database.

```sh
docker compose up -d db
DATABASE_URL=postgres://elysia:password@localhost:5432/elysia \
  cargo test --manifest-path backend/Cargo.toml
```

Shared helpers are in `tests/common`. Test data comes from the repository
functions or from the `.sql` files in `tests/fixtures`, not from queries
written in the tests.

## Chevereto end to end test

`backend/tests/chevereto/test.sh` installs a real Chevereto in Docker, uploads
a few images, stores their links in a database mirroring Arcadia's schema, runs
the import through the API and checks the files, uploaders and rewritten links.
It needs Docker, cargo and jq, and also runs in CI.

## CI

GitHub Actions runs, when the matching directory changes:

- **Backend**: clippy, build, tests, the Chevereto end to end test and rustfmt.
- **Frontend**: lint, type check and formatting.
- **Docs**: builds this book.
