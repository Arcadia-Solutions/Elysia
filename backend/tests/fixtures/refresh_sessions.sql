-- Refresh tokens are the SHA-256 of the strings in the comments.
insert into users (id, username, password_hash)
values ('00000000-0000-0000-0000-000000000001', 'alice', 'unused');

insert into sessions (user_id, refresh_token_hash, previous_refresh_token_hash, refreshed_at, remember_me, expires_at) values
  -- 'expired-refresh-token'
  ('00000000-0000-0000-0000-000000000001',
   'b810b42b1e2a00760df44961d3ce371a3a1534b14f96d104288e7d8205179d19',
   null, null, false, now() - interval '1 minute'),
  -- 'current-refresh-token', spent 'spent-refresh-token' two minutes ago
  ('00000000-0000-0000-0000-000000000001',
   '7db682c0ea66a44ce7173de5f61b910eb3ff294376c9bedea2794109549efe08',
   '73e3470dec60ac7b126bfbccb82e480d4ddab1c17eea560bc611c9a981de27e7',
   now() - interval '2 minutes', false, now() + interval '1 hour'),
  -- 'graced-current-refresh-token', spent 'graced-spent-refresh-token' just now
  ('00000000-0000-0000-0000-000000000001',
   'aaa133804c5a0ec9a348fb1e25bbcccb3ab0a9136ab68af1082ad90bedda9fbd',
   '2319832d875e5eece85c3065f5e275dd2d6d35c1404af22769146d46c8a92291',
   now(), false, now() + interval '1 hour');
