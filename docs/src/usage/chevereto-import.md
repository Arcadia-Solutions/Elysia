# Importing from Chevereto

Elysia can take over from a Chevereto v4 instance: Maintenance > Chevereto
import copies every image into Elysia and, optionally, rewrites the links stored
in the site that embeds them. Every field of the form is explained in the page.

## What gets imported

Images are read from Chevereto's MySQL database and downloaded over HTTP, then
go through the normal upload pipeline: thumbnails, EXIF stripping and
deduplication apply, size and pixel limits do not. Each image keeps its upload
date and its uploader:

- When user matching is on, a Chevereto user whose username exists in the
  embedding site's users table becomes their single sign-on account in Elysia
  (`auth.sso` must be configured).
- Everyone else, and guest uploads, go to the fallback user.

## Link rewriting

When the embedding site's PostgreSQL database is given, after each chunk of
images every Chevereto link (image, medium, thumbnail and viewer page) found in
its text-like columns is replaced with the Elysia link built from
`server.public_url`. Medium links become Elysia thumbnail links. Each chunk is
rewritten in one transaction and only rows whose content changes are updated.

A column too short to hold the new link aborts the chunk: widen it and resume.

Each chunk scans every text column of that database, so time one chunk on a
production-size copy first. A bigger batch size means fewer scans.

## Requirements

Before starting, the importer checks every connection it needs and reports the
first failure. It also refuses to run unless Chevereto uses its default routes:
`upload_image_path` is `images`, `route_image` is `image` and `root_route` is
not `image`. Chevereto served under a URL subpath is not supported for link
rewriting.

## Stopping and resuming

Progress is saved after each chunk, so the import can be stopped, or interrupted
by a restart, and resumed later. Images that fail are listed in the page without
stopping the run; "Retry failed" restarts from the first failed image, which is
safe thanks to deduplication.
