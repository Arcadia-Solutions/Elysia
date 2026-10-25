# Backup and restore

A backup is the database plus the uploads volume. The commands below assume
the Docker Compose setup from [Installation](installation.md) and run from the
repository root.

## Backup

```bash
#!/usr/bin/env bash
set -euo pipefail
out="backup-$(date +%Y%m%d-%H%M%S)"
mkdir "$out"
docker compose exec -T db pg_dump -U elysia -Fc elysia > "$out/db.dump"
docker compose run --rm --no-deps -T --entrypoint tar elysia \
  czf - -C /app/data --exclude='uploads/.tmp*' uploads > "$out/uploads.tar.gz"
```

Copy the resulting directory off the machine. `config.yml` is not in it: keep
a copy of it too.

## Restore

This replaces the current database and uploads with the backup's, so the
backend is stopped meanwhile.

```bash
#!/usr/bin/env bash
set -euo pipefail
in="$1"   # a directory made by the backup script
docker compose stop elysia
docker compose exec -T db pg_restore -U elysia -d elysia --clean --if-exists \
  < "$in/db.dump"
docker compose run --rm --no-deps -T --entrypoint sh elysia \
  -c 'find /app/data/uploads -mindepth 1 -delete && tar xzf - -C /app/data' \
  < "$in/uploads.tar.gz"
docker compose start elysia
```

Restore with the same or an older Elysia version than the one that took the
backup: migrations only run forward, on start.

## Without Docker

Same steps: `pg_dump -Fc` the database and archive `storage.upload_dir`
(skipping its `.tmp*` files).
