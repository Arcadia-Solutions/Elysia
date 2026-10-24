-- Local user accounts, SSO-ready: a future sign-on backend provisions shadow
-- rows with provider = 'sso' (issuer + subject) instead of a password hash.
-- gen_random_uuid() is built into postgres 18, no extension needed.
create type auth_provider as enum ('local', 'sso');

create table users (
  id uuid primary key default gen_random_uuid(),
  provider auth_provider not null default 'local',
  email text unique,
  username text not null unique,
  password_hash text,
  external_issuer text,
  external_sub text,
  is_admin boolean not null default false,
  banned boolean not null default false,
  storage_quota_bytes bigint check (storage_quota_bytes >= 0),
  created_at timestamptz not null default now(),
  last_seen timestamptz,
  unique (external_issuer, external_sub),
  check ((provider = 'local' and password_hash is not null and external_issuer is null and external_sub is null)
      or (provider = 'sso' and password_hash is null and external_issuer is not null and external_sub is not null))
);

comment on table users is 'Login accounts: local rows hold an Argon2 hash, sso rows are JIT-provisioned from an external sign-on backend (issuer/subject)';
comment on column users.provider is 'local = password login, sso = shadow row provisioned on first sign-in';
comment on column users.last_seen is 'Last authenticated API request';
comment on column users.storage_quota_bytes is 'Per-user storage quota override in bytes: null follows elysia_settings.default_storage_quota_bytes (admins are then unlimited), 0 is unlimited';

-- Default admin account (username `admin`, password `change-me-123`).
-- Change the password right after the first login. The hash below is the
-- Argon2id encoding of that password.
insert into users (username, email, password_hash, is_admin)
values ('admin', 'admin@example.com',
  '$argon2id$v=19$m=19456,t=2,p=1$jPojWbnVBAX8ShW6rTqduA$KiXMlsmfe39aYNAKHYsWPSJS1+9EqnIOAHXwhEbDE1M',
  true);

-- Long-lived bearer tokens for scripts and upload tools. Only the
-- SHA-256 of the token is stored. Rows are never deleted: revoked and expired
-- keys stay so files.api_key_id keeps pointing at the key that uploaded them.
create type api_key_scope as enum ('upload', 'full');

create table api_keys (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references users(id),
  name text not null,
  scope api_key_scope not null,
  token_hash text not null,
  token_prefix text not null,
  created_at timestamptz not null default now(),
  expires_at timestamptz,
  last_used_at timestamptz,
  revoked_at timestamptz
);

comment on table api_keys is 'API keys acting as their user; soft revoked (revoked_at), never deleted';
comment on column api_keys.scope is 'upload = only the upload routes, full = every route a session reaches except password change, API key management and admin powers';
comment on column api_keys.token_hash is 'SHA-256 (hex) of the token; the token itself is shown once at creation';
comment on column api_keys.token_prefix is 'First characters of the token, so users can tell their keys apart';
comment on column api_keys.expires_at is 'After this the key is refused; null never expires';
comment on column api_keys.last_used_at is 'Last request authenticated with the key';
comment on column api_keys.revoked_at is 'When the user revoked the key; null while not revoked';

-- Every API key request looks its key up by token hash.
create unique index api_keys_token_hash_idx on api_keys (token_hash);

create type file_status as enum ('available', 'trashed', 'deleted');
create type deletion_source as enum ('manual', 'scheduler');

create table files (
  id text primary key,
  ext text not null,
  mime text not null,
  original_name text,
  size bigint not null,
  width int not null,
  height int not null,
  hash text not null,
  original_hash text not null,
  original_ext text not null,
  original_width int not null,
  original_height int not null,
  requested_quality int,
  applied_quality int,
  has_thumbnail boolean not null default false,
  phash bigint not null,
  views bigint not null default 0,
  thumbnail_views bigint not null default 0,
  uploaded_by_user_id uuid not null references users(id),
  api_key_id uuid references api_keys(id),
  created_at timestamptz not null default now(),
  status file_status not null default 'available',
  trashed_at timestamptz,
  trashed_by_id uuid references users(id),
  deletion_source deletion_source,
  deleted_by_id uuid references users(id),
  check ((status = 'available') = (trashed_at is null)),
  check ((status = 'available') = (trashed_by_id is null)),
  check ((status = 'deleted') = (deletion_source is not null)),
  check ((deletion_source = 'manual') is true = (deleted_by_id is not null))
);

comment on table files is 'One row per uploaded image; the row id is also the on-disk filename stem';
comment on column files.id is 'Public short id (8-char base62, case-sensitive); also the on-disk filename stem (<first 2 id chars>/<id>.<ext>)';
comment on column files.ext is 'Normalized file extension from the sniffed type: png, jpg, gif, webp, jxl or avif';
comment on column files.mime is 'MIME type detected from the file magic bytes, e.g. image/png';
comment on column files.original_name is 'Original filename from the upload form, if the client sent one';
comment on column files.size is 'File size in bytes';
comment on column files.width is 'Image width in pixels';
comment on column files.height is 'Image height in pixels';
comment on column files.hash is 'SHA-256 of the stored (output) bytes; unique among available files, so identical outputs are deduplicated to one row';
comment on column files.original_hash is 'SHA-256 of the source bytes as uploaded';
comment on column files.original_ext is 'Source file extension before processing';
comment on column files.original_width is 'Source image width in pixels before processing';
comment on column files.original_height is 'Source image height in pixels before processing';
comment on column files.requested_quality is 'The lossy_compression_value as requested; part of the skip-reprocess key';
comment on column files.applied_quality is 'Effective quality used; null means lossless';
comment on column files.has_thumbnail is 'True when a separate <first 2 id chars>/<id>.t.webp exists; when false /t/ serves the main file (image already within the thumbnail box)';
comment on column files.phash is '64-bit DCT perceptual hash, close for visually similar images (compare with the Hamming distance), computed on upload';
comment on column files.views is 'Times the image was served from /i/';
comment on column files.thumbnail_views is 'Times the thumbnail was served from /t/';
comment on column files.uploaded_by_user_id is 'Who uploaded the file';
comment on column files.api_key_id is 'The API key the file was uploaded with; null for session and SSO delegated uploads';
comment on column files.created_at is 'When the file was uploaded';
comment on column files.status is 'available = served, trashed = moved to <root>/trash and no longer served, deleted = wiped from disk (the row stays for stats)';
comment on column files.trashed_at is 'When the file was trashed; trashed files are wiped 30 days later. Null while available';
comment on column files.trashed_by_id is 'Who trashed the file. Null while available';
comment on column files.deletion_source is 'What wiped the file from disk: manual = an admin emptied the trash, scheduler = its 30 days ran out. Null until deleted';
comment on column files.deleted_by_id is 'The admin who emptied the trash, for a manual deletion. Null otherwise';

-- Dedup only matches available files, so re-uploading a trashed image stores it anew.
create unique index files_hash_idx on files (hash) where status = 'available';
-- The trash wipe looks up old trashed files.
create index files_trashed_at_idx on files (trashed_at) where status = 'trashed';

create table file_referrer_views (
  file_id text not null references files(id) on delete cascade,
  referrer text not null,
  views bigint not null default 0,
  thumbnail_views bigint not null default 0,
  primary key (file_id, referrer)
);

comment on table file_referrer_views is 'Views of a file split by the Referer host they came from; files.views and files.thumbnail_views hold the totals';
comment on column file_referrer_views.referrer is 'Lowercased Referer host; empty for requests without a Referer';
comment on column file_referrer_views.views is 'Times the image was served from /i/ to this referrer';
comment on column file_referrer_views.thumbnail_views is 'Times the thumbnail was served from /t/ to this referrer';

-- Named sets of images a user curates. Any available image may go in any of
-- its owner's albums, whoever uploaded it.
create table albums (
  id uuid primary key default gen_random_uuid(),
  owner_id uuid not null references users(id),
  name text not null check (char_length(name) between 1 and 64),
  created_at timestamptz not null default now(),
  unique (owner_id, name)
);

comment on table albums is 'Image albums; anyone may view them, only their owner and admins change them';
comment on column albums.name is 'Display name, 1 to 64 characters, unique per owner';

create table album_images (
  album_id uuid not null references albums(id) on delete cascade,
  file_id text not null references files(id) on delete cascade,
  primary key (album_id, file_id)
);

comment on table album_images is 'Which files are in which album; a file may be in several albums. Trashed and deleted files stay listed here but are hidden';

-- The image page lists the albums a file is in.
create index album_images_file_id_idx on album_images (file_id);

create table unreferenced_files (
  file_id text primary key references files(id) on delete cascade,
  scanned_at timestamptz not null
);

comment on table unreferenced_files is 'Result of the last unreferenced images scan: available files no column of the scanned database linked to.';
comment on column unreferenced_files.scanned_at is 'When the scan started';

create table similar_files (
  file_id text primary key references files(id) on delete cascade,
  group_id int not null
);

comment on table similar_files is 'Result of the last similar images scan: available files whose perceptual hash is close to another one''s, grouped';
comment on column similar_files.group_id is 'Files sharing it look alike';

create index similar_files_group_id_idx on similar_files (group_id);

-- Chevereto import started from the web UI (Maintenance). A single row, seeded
-- below; only updated afterwards.
create table chevereto_import (
  singleton boolean primary key default true check (singleton),
  last_image_id bigint not null default 0,
  total bigint not null default 0,
  done bigint not null default 0,
  imported bigint not null default 0,
  rewritten_rows bigint not null default 0,
  started_at timestamptz,
  finished_at timestamptz,
  error text
);

insert into chevereto_import default values;

comment on table chevereto_import is 'Progress of the current or last Chevereto import run, kept so a stopped import resumes where it left off';
comment on column chevereto_import.last_image_id is 'Last Chevereto image id of the last fully processed chunk; the next run resumes after it';
comment on column chevereto_import.total is 'Chevereto images the current or last run had to process';
comment on column chevereto_import.done is 'Images the current or last run processed, failed ones included';
comment on column chevereto_import.imported is 'Images the current or last run imported';
comment on column chevereto_import.rewritten_rows is 'Backend rows whose links the current or last run rewrote';
comment on column chevereto_import.finished_at is 'When the last run ended; null while running or after a restart interrupted it';
comment on column chevereto_import.error is 'Why the last run stopped early, if it did';

create table chevereto_import_failures (
  image_id bigint primary key,
  fetch_url text,
  error text not null
);

comment on table chevereto_import_failures is 'Chevereto images that could not be imported; dropped again when a run replays them';
comment on column chevereto_import_failures.fetch_url is 'Where the original was downloaded from; null for an unsupported storage mode';

-- Skip-reprocess lookup filters on original_hash (see find_record_by_source).
create index files_original_hash_idx on files (original_hash);

-- Runtime-editable elysia settings, edited from the web UI. A single row
-- (id = 'singleton'); the boot-time infrastructure values stay in config.yml.
create type target_format as enum ('webp', 'jpegxl', 'avif', 'png', 'jpg');

create table elysia_settings (
  id text primary key,
  max_file_size_bytes bigint not null,
  max_width_pixels int not null,
  max_height_pixels int not null,
  target_width_pixels int not null,
  target_height_pixels int not null,
  default_target_file_format target_format,
  allow_overriding_file_format boolean not null default false,
  target_file_size_bytes bigint not null,
  default_compression int not null default 0,
  allow_overriding_compression boolean not null default true,
   strip_exif_by_default boolean not null default false,
   allow_overriding_strip_exif boolean not null default true,
   thumbnail_width_pixels int not null default 0,
   thumbnail_height_pixels int not null default 0,
   thumbnail_quality int not null default 80,
   allow_registration boolean not null default true,
   allowed_referrers text[] not null default '{}',
   uploads_enabled boolean not null default true,
   default_storage_quota_bytes bigint not null default 0 check (default_storage_quota_bytes >= 0),
   record_referrer_views boolean not null default false
 );

comment on table elysia_settings is 'Single-row (id = singleton) runtime elysia settings edited from the web UI';
comment on column elysia_settings.max_file_size_bytes is 'Reject uploads larger than this; 0 disables the check';
comment on column elysia_settings.max_width_pixels is 'Reject images wider than this; 0 disables the check';
comment on column elysia_settings.max_height_pixels is 'Reject images taller than this; 0 disables the check';
comment on column elysia_settings.target_width_pixels is 'Resize width bound; 0 = no resize';
comment on column elysia_settings.target_height_pixels is 'Resize height bound; 0 = no resize';
comment on column elysia_settings.default_target_file_format is 'webp | jpegxl | avif | png | jpg; null stores uploads as-is (processing off)';
comment on column elysia_settings.allow_overriding_file_format is 'When false, uploads may not request their own target file format; the default is always used';
comment on column elysia_settings.target_file_size_bytes is 'Lossy target size in bytes; 0 = no target';
comment on column elysia_settings.default_compression is 'Lossy quality (1-100) applied when an upload requests none; 0 = none';
comment on column elysia_settings.allow_overriding_compression is 'When false, uploads may not request their own compression; the default is always used';
comment on column elysia_settings.strip_exif_by_default is 'When true, EXIF is stripped from an upload that does not request otherwise';
comment on column elysia_settings.allow_overriding_strip_exif is 'When false, uploads may not request their own EXIF-stripping choice; the default is always used';
comment on column elysia_settings.thumbnail_width_pixels is 'Thumbnail box width; 0 on both axes disables thumbnails';
comment on column elysia_settings.thumbnail_height_pixels is 'Thumbnail box height; 0 on both axes disables thumbnails';
comment on column elysia_settings.thumbnail_quality is 'Lossy WebP quality (1-100) for generated thumbnails';
comment on column elysia_settings.allow_registration is 'When true, anyone may create a local account via the register endpoint';
comment on column elysia_settings.allowed_referrers is 'Hostnames whose Referer may load /i/ and /t/; empty allows any (including no Referer)';
comment on column elysia_settings.uploads_enabled is 'When false, every upload is rejected, admins included';
comment on column elysia_settings.default_storage_quota_bytes is 'Bytes of available and trashed files each non-admin user may hold, unless overridden per user; 0 is unlimited';
comment on column elysia_settings.record_referrer_views is 'When true, views are split per Referer host in file_referrer_views; when false, every view is counted under no referrer';

-- Default row
insert into elysia_settings
  (id, max_file_size_bytes, max_width_pixels, max_height_pixels,
   target_width_pixels, target_height_pixels, default_target_file_format,
   allow_overriding_file_format, target_file_size_bytes,
   default_compression, allow_overriding_compression,
   strip_exif_by_default, allow_overriding_strip_exif,
   thumbnail_width_pixels, thumbnail_height_pixels, thumbnail_quality,
   allow_registration, allowed_referrers, uploads_enabled,
   default_storage_quota_bytes, record_referrer_views)
values ('singleton', 26214400, 10000, 10000, 0, 0, 'webp', false, 0, 75, false,
  true, true, 500, 500, 80, false, '{}', true, 0, false);
