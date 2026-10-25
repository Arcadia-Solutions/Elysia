# Administration

Admins get four extra pages in the web UI: Settings, Users, Reports and Maintenance,
plus a storage dashboard and access to every user's uploads from Browse.

## Settings

The Settings page holds everything that can change without a restart: upload
limits, image processing (output format, resizing, compression, EXIF
stripping, thumbnails), which of those an uploader may override, open
registration, allowed referrers, the default storage quota and a switch to
pause all uploads. A save applies immediately. Each field is explained in the
page itself.

Contradictory combinations are refused on save. The ones worth knowing in
advance:

- A maximum file size is always required.
- Re-encoding, per-upload format overrides and thumbnails all decode the image
  in memory, so they require maximum width and height to be set. Without them a
  small file declaring huge dimensions could exhaust memory.
- AVIF and JPEG have no lossless mode: if they can be produced, either a
  default compression is set or uploaders must be allowed to pick one.

### Allowed referrers

When the list is empty, images and thumbnails are served to anyone. Otherwise
only requests whose `Referer` host is in the list (or is Elysia itself) get
them, which also blocks requests without a `Referer`. Entries are bare
hostnames like `example.com`.

### Views per referrer

Off by default: every view is counted as "no referrer". When on, views are
split by `Referer` host, one row per file and host. Without allowed referrers
anyone can send arbitrary `Referer` values and grow that table without bound,
so only turn it on together with an allowlist.

## Users

The Users page lists local and single sign-on accounts. From a user's page an
admin can browse their uploads, ban or unban them, and override their storage
quota.

A banned user's sessions, logins, API keys and delegated uploads are refused at
once.

Storage quotas count both available and trashed files. Users without an
override get the default quota from Settings; admins without an override are
unlimited. 0 means unlimited.

## Reports

Any user can report an image from its page, with a reason, as many times as
they want. The Reports page
lists every reported image, the most reported first, with who reported it and
why. Deleting an image takes it out of the list (restoring it brings its
reports back), dismissing drops its reports and keeps the image. Once a trashed
image is deleted for good, its reports are dropped too.

## Trash

Deleting an image moves it to the trash. A trashed image stops being served to
other sites immediately, but Elysia's own pages still show it to admins. Trashed
files are wiped from disk 30 days later by a background task, or right away when
an admin empties the trash.

Uploaders can trash their own images during the hour following the upload,
admins can trash and restore any image. An image cannot be restored if the same
file was uploaded again since.

## Maintenance

### Unreferenced images

Finds images nothing links to anymore. Point it at the database of the site
embedding the images (PostgreSQL or MySQL) and the hostnames its links use; it
scans every text-like column in the background and lists the images uploaded
before the scan that are linked from nowhere. Nothing changes until you trash
them, and trashing the whole list is refused once the scan is more than 24
hours old.

### Chevereto import

See [Importing from Chevereto](chevereto-import.md).
