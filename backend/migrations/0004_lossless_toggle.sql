-- Uploads no longer pick their own format or quality: they follow the
-- defaults, or ask for lossless when allowed.
alter table elysia_settings drop column allow_overriding_file_format;
alter table elysia_settings rename column allow_overriding_compression to allow_lossless;
alter table elysia_settings alter column allow_lossless set default false;

comment on column elysia_settings.default_compression is 'Lossy quality (1-100) applied to uploads; 0 = lossless';
comment on column elysia_settings.allow_lossless is 'When true, an upload may ask for lossless instead of the default_compression';

-- New installations (nothing stored yet) on a lossless-capable format default
-- to lossless.
update elysia_settings set default_compression = 0
  where not exists (select 1 from files) and default_target_file_format in ('webp', 'jpegxl');

-- Uploads no longer bring their own quality, so make the default match the
-- format: PNG has no lossy mode, AVIF and JPEG have no lossless one.
update elysia_settings set default_compression = 0 where default_target_file_format = 'png';
update elysia_settings set default_compression = 75
  where default_target_file_format in ('avif', 'jpg') and default_compression = 0;

-- Lossless is only an alternative to a lossy default, in a format that has it.
update elysia_settings set allow_lossless = allow_lossless and default_compression > 0
  and coalesce(default_target_file_format in ('webp', 'jpegxl'), false);

alter table elysia_settings
  add constraint png_is_lossless
    check (default_target_file_format <> 'png' or default_compression = 0),
  add constraint avif_jpg_need_compression
    check (default_target_file_format not in ('avif', 'jpg') or default_compression > 0),
  add constraint lossless_needs_lossy_default
    check (not allow_lossless or (default_compression > 0
      and coalesce(default_target_file_format in ('webp', 'jpegxl'), false)));
