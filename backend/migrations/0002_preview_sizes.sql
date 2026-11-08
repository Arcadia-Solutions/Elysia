-- has_medium / has_thumbnail are now derived from the sizes (> 0).
alter table files
  drop column has_medium,
  drop column has_thumbnail,
  add column medium_size bigint not null default 0,
  add column thumbnail_size bigint not null default 0;

comment on column files.medium_size is 'Bytes of the medium preview <first 2 id chars>/<id>.m.webp; 0 without one, then /m/ serves the main file (image already within the medium box)';
comment on column files.thumbnail_size is 'Bytes of the thumbnail preview <first 2 id chars>/<id>.t.webp; 0 without one, then /t/ serves the main file (image already within the thumbnail box)';
