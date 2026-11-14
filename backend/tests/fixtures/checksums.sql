-- One available file of the seeded admin, with only its SHA-256 computed.
insert into files
  (id, ext, mime, size, width, height, sha256, original_hash, original_ext, original_size,
   original_width, original_height, phash, uploaded_by_user_id)
select 'hashed00', 'png', 'image/png', 1, 1, 1, repeat('ab', 32), repeat('ab', 32), 'png', 1, 1, 1, 0, id
from users where username = 'admin';
