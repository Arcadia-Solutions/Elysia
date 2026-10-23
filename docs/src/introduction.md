# Introduction

Elysia is a self-hosted image host. Users upload images (from a file, the clipboard
or a URL), Elysia optionally re-encodes them (WebP, JPEG XL, AVIF, PNG or JPEG),
strips their metadata, generates thumbnails and serves them under stable links.

It is built to sit next to another site, primarily Arcadia: users can sign in
with that site's accounts, the site can upload on their behalf, and image
hotlinking can be restricted to it.

The project has two parts:

- **backend**: a Rust (actix-web) API backed by PostgreSQL, which also serves the
  image files.
- **frontend**: a Vue 3 single page app for uploading, browsing and administration.

The [Usage](usage/installation.md) chapters cover running and operating an
instance, the [Development](development/getting-started.md) chapters cover working
on the code.
