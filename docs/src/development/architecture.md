# Architecture

## Repository layout

| Path | Content |
| --- | --- |
| `backend/` | Rust API and image server |
| `frontend/` | Vue 3 + TypeScript single page app (PrimeVue, vue-i18n) |
| `opentelemetry/` | Grafana dashboard and host metrics collector config |
| `compose.yml` | PostgreSQL, the app and the telemetry stack |
| `docs/` | This book |

## Backend

Requests flow through `routes.rs` to `handlers/` (one file per route, grouped by
domain), which call `services/` for the logic and `repository/` for the SQL.
Every route is documented with utoipa and registered in `api_doc.rs`, which
produces the OpenAPI description the frontend client is generated from.

### Configuration and settings

There are two separate sources of configuration:

- `config.rs`: boot-time infrastructure read from `config.yml` (address,
  database, upload directory, secrets, SSO, telemetry). Changing it needs a
  restart.
- `settings.rs`: everything an admin edits from the UI, stored as the single
  `elysia_settings` row and held in memory behind a lock. A save validates and
  swaps the whole value, so uploads see the change at once.

New options belong in settings unless the server cannot start without them.

### Uploads

An upload, whether a file, a URL or an import, goes through the same pipeline
in `services/files.rs`: the body is streamed to disk under the size cap, the
image is deduplicated by hash, then optionally re-encoded, stripped of its
metadata and thumbnailed. Processing jobs are limited to one per CPU.

URL uploads refuse private and reserved addresses.

### Serving

`/i/<id>.<ext>` (image) and `/t/<id>.<ext>` (thumbnail) are served straight
from disk without touching the database. `storage.rs` owns the on-disk layout:
files are sharded by the first two characters of their id, and trashed files
move to a separate directory. View counts are kept in memory per file and
`Referer` host, and flushed to the database every 10 seconds.

`/i/<id>` without an extension is the frontend's image page, which is why the
proxies route on the presence of an extension.

### Background tasks

`scheduler.rs` runs next to the HTTP server: it flushes view counts and wipes
expired trash every hour. Long admin jobs (fixtures, unreferenced image scan,
Chevereto import) run as background tasks and expose a progress endpoint the UI
polls.

### Authentication

A bearer token is either a session JWT or an API key. Each session JWT names a
`sessions` row (`sid` claim): logout deletes it, and a password change deletes the
user's other ones. Admin rights, bans and sessions are re-read from the database
on every request, so they apply immediately.

Session JWTs live 24 hours when the user ticked "remember me", 15 minutes
otherwise. Login also returns a refresh token, stored hashed on the session row,
which `/api/auth/refresh` trades for a new JWT and a new refresh token (the old
one stops working). Each refresh pushes the session expiry back by its lifetime:
60 days when the user ticked "remember me", 2 hours otherwise. Replaying a spent
refresh token more than a minute after its refresh revokes the session, as the
token must have leaked; the grace minute spares concurrent refreshes from
several tabs. The frontend refreshes on a 401 and replays the request.

### Rate limiting

`middlewares/rate_limit.rs` keeps in-memory `governor` buckets: one per client
IP for the routes that need no login (images included), a stricter one per IP on login,
registration and password change (both middlewares wrapped on those routes, so a new
unauthenticated route has to wrap `limit_unauthenticated` too), and one per
user, checked in the auth extractors right after the token is resolved. A failed
authentication spends the per-IP unauthenticated budget; once it is out, API
keys and the SSO client credentials from that IP are refused before being
checked, so a right guess cannot slip through. Without the `rate_limit` config
section nothing is limited. The scheduler drops idle
buckets every minute. The buckets are per process, so they assume a single
backend instance, like the view counter.

## Frontend

Pages live in `src/views`, reusable components in `src/components`, and every
API call goes through the generated client in `src/services/api-schema`. Text
is translated with vue-i18n (`src/i18n`).
