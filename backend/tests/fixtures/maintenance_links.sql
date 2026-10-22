create table posts (body text, tags varchar(200)[], extra jsonb);
insert into posts values
    ('{"src": "https:\/\/elysia.test\/i\/link000000.png"}', null, null),
    (null, array['HTTP://Elysia.Test/t/link000001.webp'], null),
    (null, null, '{"viewer": "elysia.test/i/link000002"}'),
    ('other.test/i/link000003.png elysia.test/upload', null, null);
