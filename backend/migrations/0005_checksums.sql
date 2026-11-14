-- Checksums of the stored bytes, one column per algorithm named after its
-- checksum_algorithm label, served by the content-addressed /ih/, /mh/ and
-- /th/ links. Every algorithm of https://github.com/RustCrypto/hashes with a
-- fixed output size. sha256 (the former hash column) is always set, as it
-- deduplicates uploads; the others stay null until computed.
create type checksum_algorithm as enum (
  'ascon_hash256',
  'bash_hash256', 'bash_hash384', 'bash_hash512',
  'belt_hash',
  'blake2b128', 'blake2b256', 'blake2b512', 'blake2s128', 'blake2s256',
  'fsb160', 'fsb224', 'fsb256', 'fsb384', 'fsb512',
  'gost94_crypto_pro', 'gost94_s2015', 'gost94_test', 'gost94_ua',
  'groestl224', 'groestl256', 'groestl384', 'groestl512',
  'jh224', 'jh256', 'jh384', 'jh512',
  'kupyna224', 'kupyna256', 'kupyna384', 'kupyna512',
  'md2', 'md4', 'md5',
  'ripemd128', 'ripemd160', 'ripemd256', 'ripemd320',
  'sha1',
  'sha224', 'sha256', 'sha384', 'sha512', 'sha512_224', 'sha512_256',
  'sha3_224', 'sha3_256', 'sha3_384', 'sha3_512',
  'keccak224', 'keccak256', 'keccak384', 'keccak512',
  'shabal192', 'shabal224', 'shabal256', 'shabal384', 'shabal512',
  'skein256_256', 'skein256_512', 'skein512_256', 'skein512_512',
  'skein1024_256', 'skein1024_512', 'skein1024_1024',
  'sm3',
  'streebog256', 'streebog512',
  'tiger', 'tiger2',
  'whirlpool'
);

alter table files rename column hash to sha256;
alter index files_hash_idx rename to files_sha256_idx;
comment on column files.sha256 is 'SHA-256 of the stored (output) bytes, lowercase hex; unique among available files, so identical outputs are deduplicated to one row';

do $$
declare
  algorithm text;
begin
  foreach algorithm in array enum_range(null::checksum_algorithm)::text[] loop
    continue when algorithm = 'sha256';
    execute format('alter table files add column %I text', algorithm);
    execute format('create index %I on files (%I) where %I is not null',
                   'files_' || algorithm || '_idx', algorithm, algorithm);
    execute format('comment on column files.%I is %L', algorithm,
                   'Lowercase hex ' || algorithm || ' of the stored bytes; null until computed');
  end loop;
end
$$;

-- The available file whose stored bytes have this checksum.
create function file_id_by_checksum(algorithm checksum_algorithm, digest text) returns text
language plpgsql stable as $$
declare
  found_id text;
begin
  execute format('select id from files where %I = $1 and status = ''available'' limit 1', algorithm)
    into found_id using digest;
  return found_id;
end
$$;

-- Set checksums of a file, the digest at the same index as its algorithm.
create function set_file_checksums(file_id text, algorithms checksum_algorithm[], digests text[]) returns void
language plpgsql as $$
begin
  for index in 1 .. coalesce(array_length(algorithms, 1), 0) loop
    execute format('update files set %I = $1 where id = $2', algorithms[index])
      using digests[index], file_id;
  end loop;
end
$$;

alter table elysia_settings
  add column checksum_algorithms checksum_algorithm[] not null default '{}';

comment on column elysia_settings.checksum_algorithms is 'Checksums computed on upload besides SHA-256, which always is';
