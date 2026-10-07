create table files (
  id text primary key,
  ext text not null,
  mime text not null,
  original_name text,
  size bigint not null,
  width int not null,
  height int not null,
  created_at timestamptz not null default now()
);

comment on table files is 'One row per uploaded image; the row id is also the on-disk filename stem';
comment on column files.id is 'Public short id (8-char base62, case-sensitive); also the on-disk filename stem (<id>.<ext>)';
comment on column files.ext is 'Normalized file extension from the sniffed type: png, jpg, gif or webp';
comment on column files.mime is 'MIME type detected from the file magic bytes, e.g. image/png';
comment on column files.original_name is 'Original filename from the upload form, if the client sent one';
comment on column files.size is 'File size in bytes';
comment on column files.width is 'Image width in pixels';
comment on column files.height is 'Image height in pixels';
comment on column files.created_at is 'When the file was uploaded';
