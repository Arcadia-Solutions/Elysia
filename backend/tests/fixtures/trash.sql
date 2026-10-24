-- Two trashed files of the seeded admin: one past its 30 days, one not.
insert into files
  (id, ext, mime, size, width, height, hash, original_hash, original_ext,
   original_width, original_height, phash, uploaded_by_user_id, status, trashed_at, trashed_by_id)
select trashed.id, 'png', 'image/png', 1, 1, 1, trashed.id, trashed.id, 'png', 1, 1, hashtextextended(trashed.id, 0), users.id, 'trashed', trashed.trashed_at, users.id
from users, (values ('expired0', now() - interval '31 days'), ('recent00', now() - interval '29 days'))
  as trashed(id, trashed_at)
where users.username = 'admin';
