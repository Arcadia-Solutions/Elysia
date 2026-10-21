# Elysia backend

```sh
docker compose up -d db                               # start postgres
cargo run --manifest-path backend/Cargo.toml          # run from repo root (reads ./config.yml)
```

Swagger UI at http://localhost:8090/swagger-ui/, OpenAPI JSON at /api-docs/openapi.json.

To run everything in docker instead (backend and web UI on http://localhost:5174):

```sh
docker compose --profile app up -d --build
```

The images are built from `backend/Dockerfile` and `frontend/Dockerfile`. The web UI proxies the
API and the images to `elysia:8090`, override it with the `ELYSIA_BACKEND` environment variable.

Any `config.yml` value can be overridden with an `ELYSIA_<SECTION>__<KEY>` environment variable,
e.g. `ELYSIA_DATABASE__PASSWORD`.

Tests need the database running (the `#[sqlx::test]` tests create a throwaway
database each):

```sh
docker compose up -d db
DATABASE_URL=postgres://elysia:password@localhost:5432/elysia \
  cargo test --manifest-path backend/Cargo.toml
```

## Test data

```sh
cargo run --release --manifest-path backend/Cargo.toml --bin seed -- 300   # image count, default 300
```

Creates 12 users (`alice`, `bob`, ..., password `password123`) and uploads generated
images through the real upload pipeline, spread over the last 3 years. Runs on top
of what is there, so run it again to add more. Reset with
`cargo sqlx database reset --force` and `rm -r data/uploads`. Use `--release`: the
AVIF and JPEG XL encoders take seconds per image in a debug build.

## Importing from Chevereto

```sh
cp backend/src/bin/import_chevereto/import.example.yml import.yml   # then fill it in
cargo run --release --manifest-path ../../Cargo.toml --bin import_chevereto -- import.yml
```

Copies every image of a Chevereto v4 instance (read from its MySQL database,
downloaded over HTTP) into Elysia through the real upload pipeline, with the size and
pixel limits bypassed but processing (thumbnails, EXIF stripping, dedup) kept. Each
image keeps its upload date and its uploader: when `backend.users` is set, Chevereto
users whose username exists in the backend's users table become their SSO account
there (`auth.sso` must be configured), everyone else and guest uploads go to
`fallback_username`.

When `backend` is set, after each chunk Chevereto links (image, medium, thumbnail and
viewer page URLs) are rewritten to Elysia URLs in every text-like column of the
backend's Postgres database (text, varchar,
char, citext, domains over them, their arrays, json and jsonb), in one transaction.
Medium links point to the Elysia thumbnail. Only rows whose content changes are updated, once per table, so the backend's update triggers
fire once per rewritten row. A column too short for the new link aborts the chunk:
widen it and rerun.

The last finished image id is saved in `state_file`, so the import can be stopped and
rerun at any time (it resumes after the last finished chunk). Images that could not be
imported are listed in `failed_file` and do not stop the run. To retry them, set the
state file to the lowest failed `image_id` minus 1 and rerun (dedup makes the replay
safe): at startup, entries after the state file's id are dropped from `failed_file`,
since those images are imported again.

At startup the importer checks Chevereto's `chv_settings` and refuses to run unless
`upload_image_path` is `images`, `route_image` is `image` and `root_route` is not
`image`: the link patterns and download URLs assume these defaults. Chevereto served
under a URL subpath is not supported for link rewriting (patterns are built from
`link_hosts` + `/images/` and `/image/`).

Each chunk scans the backend's text columns with one regex matching any Chevereto link,
so time one chunk against a production-size copy of the backend before the real run. A bigger
`batch_size` means fewer full scans.

Chevereto users are matched to backend users by exact username when their first image
is imported. A user without a backend account is logged once and their images go to
`fallback_username`.

`tests/chevereto/test.sh` runs the importer end to end against a real Chevereto in
docker (also run in CI).

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
