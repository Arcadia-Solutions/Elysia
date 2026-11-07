# Importing from Chevereto

Elysia can take over from a Chevereto v4 instance: Maintenance > Chevereto
import copies every image into Elysia and, optionally, rewrites the links stored
in the site that embeds them. Every field of the form is explained in the page.

## What gets imported

Images are read from Chevereto's MySQL database and downloaded over HTTP, then
go through the normal upload pipeline: medium images, EXIF stripping and
deduplication apply, size and pixel limits do not. Each image keeps its upload
date (unless it already existed in Elysia, then its date is left untouched) and
its uploader:

- When user matching is on, a Chevereto user whose username exists in the
  embedding site's users table becomes their single sign-on account in Elysia
  (`auth.sso` must be configured).
- Everyone else, and guest uploads, go to the fallback user.

## Link rewriting

When the embedding site's PostgreSQL database is given, after each chunk of
images every Chevereto link (image, medium, medium and viewer page) found in
its text-like columns is replaced with the Elysia link built from
`server.public_url`. Medium links become Elysia medium links. Each chunk is
rewritten in one transaction and only rows whose content changes are updated.

A column too short to hold the new link aborts the chunk: widen it and resume.

Once a run has processed its last chunk, it counts the rows of that database
that still contain a Chevereto link and shows the number, highlighted when it is
not zero. Links of images that did not import (failed, unsupported storage,
deleted from Chevereto) are left untouched, so they are counted: retry the
failed images, then check again. A dry run counts too, which tells how many
rows hold Chevereto links before importing. Nothing is counted when the run
stops early or without a backend database.

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
stopping the run; "Retry failed" imports only those images again, in chunks,
without moving the saved resume point. Images that succeed leave the list, the
others stay with their new error.

## Dry run

With "Dry run" on, a run does all the checks above and downloads every image it
would import to validate it (reachable and a supported image), but stores
nothing in Elysia, creates no user and rewrites no link. The progress then
counts the images that would import. Images that would fail are added to the
failures list (a later real run replays them anyway), while failures of real
runs stay listed even when they would now succeed. The saved resume point does
not move, so a real "Resume" afterwards starts where the last real run stopped.
