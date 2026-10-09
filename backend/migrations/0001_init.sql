create table files (
  id text primary key,
  ext text not null,
  mime text not null,
  original_name text,
  size bigint not null,
  width int not null,
  height int not null,
  hash text unique not null,
  original_hash text not null,
  original_ext text not null,
  original_width int not null,
  original_height int not null,
  requested_quality int,
  applied_quality int,
  created_at timestamptz not null default now()
);

comment on table files is 'One row per uploaded image; the row id is also the on-disk filename stem';
comment on column files.id is 'Public short id (8-char base62, case-sensitive); also the on-disk filename stem (<id>.<ext>)';
comment on column files.ext is 'Normalized file extension from the sniffed type: png, jpg, gif, webp, jxl or avif';
comment on column files.mime is 'MIME type detected from the file magic bytes, e.g. image/png';
comment on column files.original_name is 'Original filename from the upload form, if the client sent one';
comment on column files.size is 'File size in bytes';
comment on column files.width is 'Image width in pixels';
comment on column files.height is 'Image height in pixels';
comment on column files.hash is 'SHA-256 of the stored (output) bytes; unique, so identical outputs are deduplicated to one row';
comment on column files.original_hash is 'SHA-256 of the source bytes as uploaded';
comment on column files.original_ext is 'Source file extension before processing';
comment on column files.original_width is 'Source image width in pixels before processing';
comment on column files.original_height is 'Source image height in pixels before processing';
comment on column files.requested_quality is 'The lossy_compression_value as requested; part of the skip-reprocess key';
comment on column files.applied_quality is 'Effective quality used; null means lossless';
comment on column files.created_at is 'When the file was uploaded';

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
  allow_overriding_strip_exif boolean not null default true
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

-- Default row: processing off, 10 MiB upload ceiling (valid against the same
-- rules the API enforces on every save).
insert into elysia_settings
  (id, max_file_size_bytes, max_width_pixels, max_height_pixels,
   target_width_pixels, target_height_pixels, default_target_file_format,
   allow_overriding_file_format, target_file_size_bytes,
   default_compression, allow_overriding_compression,
   strip_exif_by_default, allow_overriding_strip_exif)
values ('singleton', 10485760, 0, 0, 0, 0, null, false, 0, 0, true, false, true);
