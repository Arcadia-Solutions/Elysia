# Installation

## With Docker Compose

The repository's `compose.yml` runs PostgreSQL, the backend and the web UI:

```sh
cp config.example.yml config.yml   # then edit it, see below
docker compose --profile app up -d --build
```

The web UI is on <http://localhost:5174>. It serves the single page app and
proxies the API (`/api`), the images (`/i`, `/t`) and the API docs to the
backend. The backend address defaults to `elysia:8090` and can be changed with
the `ELYSIA_BACKEND` environment variable of the frontend container.
Request bodies are capped at 100MB, change it with `ELYSIA_MAX_BODY_SIZE`
(e.g. `500MB`) if `max_file_size_bytes` is set higher.

Uploaded files live in the `elysia_uploads` volume, the database in
`elysia_pgdata`. See [Backup and restore](backup.md).

### Behind another proxy

The rate limits key on the client IP. If another proxy (Cloudflare, a load
balancer...) sits in front of the web UI, the web UI's Caddy throws away the
`X-Forwarded-For` it sends, and every user ends up sharing that proxy's
address. Trust it in the `frontend/Caddyfile` global options, then add its
ranges to `rate_limit.trusted_proxies` in `config.yml` if they are public:

```caddyfile
{
	servers {
		trusted_proxies static 173.245.48.0/20 103.21.244.0/22
	}
}
```

## Configuration

`config.yml` only holds what the backend needs to boot: listen address, public
URL, database, upload directory, session secret, single sign-on, telemetry
and rate limits.
Every key is described in `config.example.yml`. Everything else (upload limits,
image processing, registration, quotas, allowed referrers...) is edited at
runtime from the web UI, see [Administration](administration.md).

The backend reads `config.yml` from the path in `ELYSIA_CONFIG`, or else from the
current directory or the closest parent containing one. A relative
`storage.upload_dir` is resolved against the config file's directory.

Any value can be overridden with an `ELYSIA_<SECTION>__<KEY>` environment
variable, for example `ELYSIA_DATABASE__PASSWORD` or `ELYSIA_SERVER__PUBLIC_URL`.
The compose file uses this to point the backend at the `db` container.

The backend refuses to start when:

- `auth.jwt_secret` is shorter than 32 characters or still the example value,
- `auth.sso` is set but `server.public_url` is empty.

## First login

The database is created with one admin account: username `admin`, password
`change-me-123`. Change the password from the Account page right away.

Accounts created through registration or single sign-on are never admins.

## Without Docker

Build the backend (`cargo build --release` in `backend/`, see
[Getting started](../development/getting-started.md) for the build dependencies)
and the frontend (`npm ci && npm run build` in `frontend/`). Serve
`frontend/dist` as a single page app and route the same paths as
`frontend/Caddyfile` to the backend. Migrations run automatically when the
backend starts.

## Favicon

Place your icon at `frontend/public/favicon.ico` before building the frontend.
The file is gitignored.

## Custom CSS and JS

Two optional files, gitignored, are linked into every page when they exist at
build time:

- `frontend/public/custom.css`: css applied after the app styles
- `frontend/public/custom.js`: js executed once the app is mounted

Leaving them out is a valid setup. Creating, editing or removing them requires
rebuilding the frontend.
