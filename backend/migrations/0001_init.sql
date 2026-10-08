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
