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
  created_at timestamptz not null default now(),
  last_seen timestamptz,
  unique (external_issuer, external_sub),
  check ((provider = 'local' and password_hash is not null and external_issuer is null and external_sub is null)
      or (provider = 'sso' and password_hash is null and external_issuer is not null and external_sub is not null))
);

comment on table users is 'Login accounts: local rows hold an Argon2 hash, sso rows are JIT-provisioned from an external sign-on backend (issuer/subject)';
comment on column users.provider is 'local = password login, sso = shadow row provisioned on first sign-in';
comment on column users.last_seen is 'Last authenticated API request';

-- Default admin account (username `admin`, password `change-me-123`).
-- Change the password right after the first login. The hash below is the
-- Argon2id encoding of that password.
insert into users (username, email, password_hash, is_admin)
values ('admin', 'admin@example.com',
  '$argon2id$v=19$m=19456,t=2,p=1$jPojWbnVBAX8ShW6rTqduA$KiXMlsmfe39aYNAKHYsWPSJS1+9EqnIOAHXwhEbDE1M',
  true);

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
  views bigint not null default 0,
  thumbnail_views bigint not null default 0,
  uploaded_by_user_id uuid not null references users(id),
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
comment on column files.views is 'Times the image was served from /i/';
comment on column files.thumbnail_views is 'Times the thumbnail was served from /t/';
comment on column files.uploaded_by_user_id is 'Who uploaded the file';
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
   allowed_referrers text[] not null default '{}'
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

-- Default row: processing off, 10 MiB upload ceiling (valid against the same
-- rules the API enforces on every save).
insert into elysia_settings
  (id, max_file_size_bytes, max_width_pixels, max_height_pixels,
   target_width_pixels, target_height_pixels, default_target_file_format,
   allow_overriding_file_format, target_file_size_bytes,
   default_compression, allow_overriding_compression,
   strip_exif_by_default, allow_overriding_strip_exif,
   thumbnail_width_pixels, thumbnail_height_pixels, thumbnail_quality,
   allow_registration)
values ('singleton', 10485760, 0, 0, 0, 0, null, false, 0, 0, true, false, true, 0, 0, 80, true);
