create domain link_text as text;

create table backend_like (
    id int,
    body text,
    title varchar(200),
    pictures text[],
    extra jsonb,
    legacy json,
    avatar link_text,
    code char(60),
    untouched int
);

insert into backend_like (id, body, title, pictures, extra, legacy, avatar, code, untouched) values
    (1, 'see https://img.example.com/images/2024/05/03/abc.png now', null, null, null, null, null, null, 1),
    (2, null, 'https://img.example.com/image/jbz', null, null, null, null, null, 2),
    (3, null, null, array['https://img.example.com/images/2024/05/03/abc.th.png', 'other'], null, null, null, null, 3),
    (4, null, null, null, '{"url": "https://img.example.com/images/2024/05/03/abc.md.png"}', null, null, null, 4),
    (5, 'no links here', 'plain', array['plain'], '{"a": 1}', '{"a": 1}', 'plain', 'plain', 5),
    (6, 'https://img.example.com/image/jbz', 'https://img.example.com/image/jbz', null, null, null, null, null, 6),
    (7, null, null, null, null, '{"url": "https://img.example.com/image/jbz"}', 'https://img.example.com/image/jbz', 'https://img.example.com/image/jbz', 7),
    (8, 'https://img.example.com/images/2024/05/03/other.png', null, null, null, null, null, null, 8);
