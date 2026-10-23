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

Admins can generate fixtures from the browse page ("Generate fixtures", or
`POST /api/fixtures`): it creates the chosen number of users (`alice`, `bob`, ..., then `alice2`, ...,
with the password given in the form) and uploads generated images through the real upload pipeline,
dated over the chosen number of past days. Runs on top of what is there, so run
it again to add more. Reset with `cargo sqlx database reset --force` and
`rm -r data/uploads`. Run the server with `--release`: the AVIF and JPEG XL
encoders take seconds per image in a debug build.

## Importing from Chevereto

Admins run it from the web UI, Maintenance > Chevereto import, where every field is
explained. It copies every image of a Chevereto v4 instance (read from its MySQL
database, downloaded over HTTP) into Elysia through the real upload pipeline, in the
background, with the size and pixel limits bypassed but processing (thumbnails, EXIF
stripping, dedup) kept. Each image keeps its upload date and its uploader: when users
are matched, Chevereto users whose username exists in the backend's users table become
their SSO account there (`auth.sso` must be configured), everyone else and guest uploads
go to the fallback user.

With a backend database, after each chunk Chevereto links (image, medium, thumbnail and
viewer page URLs) are rewritten to Elysia URLs built from `server.public_url` in every
text-like column of the backend's Postgres database (text, varchar, char, citext, domains
over them, their arrays, json and jsonb), in one transaction. Medium links point to the
Elysia thumbnail. Only rows whose content changes are updated, once per table, so the
backend's update triggers fire once per rewritten row. A column too short for the new
link aborts the chunk: widen it and resume.

Progress is saved in the `chevereto_import` table after each chunk, so the import can be
stopped (or interrupted by a restart) and resumed. Images that could not be imported are
listed in `chevereto_import_failures` (and in the UI) and do not stop the run; "Retry
failed" restarts from the lowest failed image (dedup makes the replay safe).

Before starting, the importer connects to the Chevereto database and site, the backend
database and its users table (when set), and returns the first failure as the start
error. It also checks Chevereto's `chv_settings` and refuses to run unless
`upload_image_path` is `images`, `route_image` is `image` and `root_route` is not
`image`: the link patterns and download URLs assume these defaults. Chevereto served
under a URL subpath is not supported for link rewriting (patterns are built from the
link hosts + `/images/` and `/image/`).

Each chunk scans the backend's text columns with one regex matching any Chevereto link,
so time one chunk against a production-size copy of the backend before the real run. A
bigger batch size means fewer full scans.

`tests/chevereto/test.sh` runs the import end to end against a real Chevereto in docker
through the API (also run in CI).

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
