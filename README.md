<h1 align="center">
  <img alt="Elysia's logo" src="./frontend/src/assets/logo.example.svg" width="120px"/><br/>
  Elysia
</h1>

<p align="center"><b>Self-hosted image host, built to sit next to your website.</b></p>

<p align="center">
  <a href="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/backend.yml"><img src="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/backend.yml/badge.svg" alt="Backend CI"></a>
  <a href="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/frontend.yml"><img src="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/frontend.yml/badge.svg" alt="Frontend CI"></a>
  <a href="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/e2e.yml"><img src="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/e2e.yml/badge.svg" alt="E2E"></a>
  <a href="https://github.com/Arcadia-Solutions/Elysia/actions/workflows/backend.yml"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/Arcadia-Solutions/Elysia/badges/coverage.json" alt="coverage"></a>
  <a href="https://arcadia-solutions.github.io/Elysia/"><img src="https://img.shields.io/badge/docs-mdBook-orange" alt="Docs"></a>
</p>

<p align="center">
  <a href="https://discord.gg/amYWVk7pS3"><img src="https://img.shields.io/badge/Discord-Chat-5865F2?logo=discord&logoColor=white" alt="Join Our Discord"></a>
  <a href="https://matrix.to/#/%23Arcadia-Solutions%3Amatrix.org?via=matrix.org"><img src="https://img.shields.io/badge/Matrix-Chat-000?logo=matrix&logoColor=fff" alt="Join Our Matrix"></a>
</p>

## Screenshots

<img src="media/upload.webp" alt="Upload">
<img src="media/browse.webp" alt="Browse">
<img src="media/dashboard.webp" alt="Dashboard">
<img src="media/settings.webp" alt="Settings">

Elysia lets users upload images, re-encodes and de-duplicates them, and serves them under stable links. It is made to pair with [Arcadia](https://github.com/Arcadia-Solutions/Arcadia), but works with any website.

## Features

- **Uploads** from a file, the clipboard or a URL
- **Re-encoding** to WebP, JPEG XL, AVIF, PNG or JPEG (or lossless), metadata stripping, medium and thumbnail previews
- **Integration** with another site: OpenID Connect SSO, uploads on behalf of users, API keys, hotlink protection by referrer
- **Moderation**: reports, trash, expiry, user management
- **Maintenance tools**: similar image detection (perceptual hash), unreferenced image cleanup (in your website's db)
- **Chevereto importer** with link rewriting, dry run and resume
- **Operations**: admin dashboard, rate limiting, disk space guard, outbound proxy, OpenTelemetry traces/metrics/logs
- **Customizable**: logo, favicon, custom CSS/JS, terms of service and privacy policy pages
- **OpenAPI** schema with Swagger UI

## Stack

| Part | Tech |
| --- | --- |
| Backend | Rust, actix-web, PostgreSQL (sqlx) |
| Frontend | Vue 3, TypeScript, PrimeVue |

## Quick start

```sh
cp config.example.yml config.yml   # set auth.jwt_secret at least
docker compose --profile app up -d
```

The web UI is then on <http://localhost:5174>. See [the docs](https://arcadia-solutions.github.io/Elysia/) for installation, administration, integration, backups and development.
