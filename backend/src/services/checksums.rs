//! Checksums of the stored bytes, for the content-addressed `/ih/`, `/mh/` and
//! `/th/` links. SHA-256 is always computed (it also deduplicates uploads),
//! the other algorithms on upload when enabled in the settings, or for
//! existing files by a background job.

use actix_web::web::Data;
use serde::{Deserialize, Serialize};
use sha2::digest::DynDigest;
use sqlx::PgPool;
use tokio::io::AsyncReadExt;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::repository::checksums;
use crate::repository::files::{self, ImagesStatus};
use crate::services::{Job, JobProgress, check_shutdown};
use crate::storage::Storage;

/// Define [`ChecksumAlgorithm`] from `Variant = "name" => Hasher` entries, in
/// the `checksum_algorithm` Postgres enum order. The name is also the files
/// column, the serialized value and the `/ih/<algorithm>/` path segment.
macro_rules! checksum_algorithms {
    ($($variant:ident = $name:literal => $hasher:ty,)*) => {
        /// A checksum algorithm, as the `checksum_algorithm` Postgres enum.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema, sqlx::Type)]
        // utoipa ignores a `serde(rename)` given by the macro, so serde names
        // the variants by `rename_all`, checked against `$name` by a test.
        #[serde(rename_all = "snake_case")]
        #[sqlx(type_name = "checksum_algorithm")]
        pub enum ChecksumAlgorithm {
            $(
                #[sqlx(rename = $name)]
                $variant,
            )*
        }

        impl ChecksumAlgorithm {
            pub const ALL: &[ChecksumAlgorithm] = &[$(ChecksumAlgorithm::$variant),*];

            pub fn name(self) -> &'static str {
                match self {
                    $(ChecksumAlgorithm::$variant => $name,)*
                }
            }

            fn hasher(self) -> Box<dyn DynDigest + Send> {
                match self {
                    $(ChecksumAlgorithm::$variant => Box::new(<$hasher>::default()),)*
                }
            }
        }
    };
}

// Every algorithm of https://github.com/RustCrypto/hashes with a fixed output size.
checksum_algorithms! {
    AsconHash256 = "ascon_hash256" => ascon_hash256::AsconHash256,
    BashHash256 = "bash_hash256" => bash_hash::BashHash256,
    BashHash384 = "bash_hash384" => bash_hash::BashHash384,
    BashHash512 = "bash_hash512" => bash_hash::BashHash512,
    BeltHash = "belt_hash" => belt_hash::BeltHash,
    Blake2b128 = "blake2b128" => blake2::Blake2b128,
    Blake2b256 = "blake2b256" => blake2::Blake2b256,
    Blake2b512 = "blake2b512" => blake2::Blake2b512,
    Blake2s128 = "blake2s128" => blake2::Blake2s128,
    Blake2s256 = "blake2s256" => blake2::Blake2s256,
    Fsb160 = "fsb160" => fsb::Fsb160,
    Fsb224 = "fsb224" => fsb::Fsb224,
    Fsb256 = "fsb256" => fsb::Fsb256,
    Fsb384 = "fsb384" => fsb::Fsb384,
    Fsb512 = "fsb512" => fsb::Fsb512,
    Gost94CryptoPro = "gost94_crypto_pro" => gost94::Gost94CryptoPro,
    Gost94S2015 = "gost94_s2015" => gost94::Gost94s2015,
    Gost94Test = "gost94_test" => gost94::Gost94Test,
    Gost94Ua = "gost94_ua" => gost94::Gost94UA,
    Groestl224 = "groestl224" => groestl::Groestl224,
    Groestl256 = "groestl256" => groestl::Groestl256,
    Groestl384 = "groestl384" => groestl::Groestl384,
    Groestl512 = "groestl512" => groestl::Groestl512,
    Jh224 = "jh224" => jh::Jh224,
    Jh256 = "jh256" => jh::Jh256,
    Jh384 = "jh384" => jh::Jh384,
    Jh512 = "jh512" => jh::Jh512,
    Kupyna224 = "kupyna224" => kupyna::Kupyna224,
    Kupyna256 = "kupyna256" => kupyna::Kupyna256,
    Kupyna384 = "kupyna384" => kupyna::Kupyna384,
    Kupyna512 = "kupyna512" => kupyna::Kupyna512,
    Md2 = "md2" => md2::Md2,
    Md4 = "md4" => md4::Md4,
    Md5 = "md5" => md5::Md5,
    Ripemd128 = "ripemd128" => ripemd::Ripemd128,
    Ripemd160 = "ripemd160" => ripemd::Ripemd160,
    Ripemd256 = "ripemd256" => ripemd::Ripemd256,
    Ripemd320 = "ripemd320" => ripemd::Ripemd320,
    Sha1 = "sha1" => sha1::Sha1,
    Sha224 = "sha224" => sha2::Sha224,
    Sha256 = "sha256" => sha2::Sha256,
    Sha384 = "sha384" => sha2::Sha384,
    Sha512 = "sha512" => sha2::Sha512,
    Sha512_224 = "sha512_224" => sha2::Sha512_224,
    Sha512_256 = "sha512_256" => sha2::Sha512_256,
    Sha3_224 = "sha3_224" => sha3::Sha3_224,
    Sha3_256 = "sha3_256" => sha3::Sha3_256,
    Sha3_384 = "sha3_384" => sha3::Sha3_384,
    Sha3_512 = "sha3_512" => sha3::Sha3_512,
    Keccak224 = "keccak224" => sha3::Keccak224,
    Keccak256 = "keccak256" => sha3::Keccak256,
    Keccak384 = "keccak384" => sha3::Keccak384,
    Keccak512 = "keccak512" => sha3::Keccak512,
    Shabal192 = "shabal192" => shabal::Shabal192,
    Shabal224 = "shabal224" => shabal::Shabal224,
    Shabal256 = "shabal256" => shabal::Shabal256,
    Shabal384 = "shabal384" => shabal::Shabal384,
    Shabal512 = "shabal512" => shabal::Shabal512,
    Skein256_256 = "skein256_256" => skein::Skein256_256,
    Skein256_512 = "skein256_512" => skein::Skein256_512,
    Skein512_256 = "skein512_256" => skein::Skein512_256,
    Skein512_512 = "skein512_512" => skein::Skein512_512,
    Skein1024_256 = "skein1024_256" => skein::Skein1024_256,
    Skein1024_512 = "skein1024_512" => skein::Skein1024_512,
    Skein1024_1024 = "skein1024_1024" => skein::Skein1024_1024,
    Sm3 = "sm3" => sm3::Sm3,
    Streebog256 = "streebog256" => streebog::Streebog256,
    Streebog512 = "streebog512" => streebog::Streebog512,
    Tiger = "tiger" => tiger::Tiger,
    Tiger2 = "tiger2" => tiger::Tiger2,
    Whirlpool = "whirlpool" => whirlpool::Whirlpool,
}

/// Lowercase hex digests of a file's bytes, one per algorithm in order, in a
/// single streamed read so large files never buffer in memory.
pub async fn hash_file(
    path: &std::path::Path,
    algorithms: &[ChecksumAlgorithm],
) -> Result<Vec<String>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hashers: Vec<_> = algorithms
        .iter()
        .map(|algorithm| algorithm.hasher())
        .collect();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        for hasher in &mut hashers {
            hasher.update(&buffer[..read]);
        }
    }
    Ok(hashers
        .into_iter()
        .map(|hasher| to_hex(&hasher.finalize()))
        .collect())
}

/// Lowercase hex of a digest.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// SHA-256 of a file's bytes, as lowercase hex.
pub async fn sha256_file(path: &std::path::Path) -> Result<String> {
    let mut digests = hash_file(path, &[ChecksumAlgorithm::Sha256]).await?;
    Ok(digests.remove(0))
}

/// Algorithms to compute on upload: SHA-256 first, then the enabled ones.
pub fn upload_algorithms(enabled: &[ChecksumAlgorithm]) -> Vec<ChecksumAlgorithm> {
    let mut algorithms = vec![ChecksumAlgorithm::Sha256];
    algorithms.extend(
        enabled
            .iter()
            .filter(|algorithm| **algorithm != ChecksumAlgorithm::Sha256),
    );
    algorithms
}

/// How many available files have a checksum of one algorithm.
#[derive(Serialize, ToSchema)]
pub struct ChecksumCount {
    pub algorithm: ChecksumAlgorithm,
    pub files: i64,
}

/// Checksum coverage of the available files.
#[derive(Serialize, ToSchema)]
pub struct ChecksumCounts {
    /// Available files.
    pub total_files: i64,
    /// One entry per algorithm at least one available file has, in algorithm order.
    pub counts: Vec<ChecksumCount>,
}

pub async fn counts(pool: &PgPool) -> Result<ChecksumCounts> {
    Ok(ChecksumCounts {
        total_files: files::count(pool, ImagesStatus::Available, None, None).await?,
        counts: checksums::counts(pool)
            .await?
            .into_iter()
            .map(|(algorithm, files)| ChecksumCount { algorithm, files })
            .collect(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct ComputeChecksumsParams {
    /// Algorithms to compute for the available files missing them.
    pub algorithms: Vec<ChecksumAlgorithm>,
}

/// State of the current or last checksum computation.
#[derive(Clone, Serialize, ToSchema)]
pub struct ChecksumsProgress {
    pub running: bool,
    /// Files hashed so far.
    pub done: i64,
    /// Files missing one of the algorithms when the run started.
    pub total: i64,
    /// Files missing from disk, skipped.
    pub missing: i64,
    /// Why the last run stopped early, if it did.
    pub error: Option<String>,
}

impl JobProgress for ChecksumsProgress {
    fn running(&mut self) -> &mut bool {
        &mut self.running
    }

    fn error(&mut self) -> &mut Option<String> {
        &mut self.error
    }
}

static JOB: Job<ChecksumsProgress> = Job::new(ChecksumsProgress {
    running: false,
    done: 0,
    total: 0,
    missing: 0,
    error: None,
});

pub fn progress() -> ChecksumsProgress {
    JOB.progress()
}

/// Start computing `params.algorithms` for the available files missing them
/// in the background. Fails without algorithms or if a run is already going.
pub fn start_compute(
    pool: PgPool,
    storage: Data<Storage>,
    params: ComputeChecksumsParams,
) -> Result<()> {
    let mut algorithms = params.algorithms;
    algorithms.sort();
    algorithms.dedup();
    if algorithms.is_empty() {
        return Err(Error::BadRequest("pick at least one algorithm".into()));
    }
    JOB.spawn(
        "checksums computation",
        |progress| {
            progress.done = 0;
            progress.total = 0;
            progress.missing = 0;
        },
        async move { compute(&pool, &storage, &algorithms).await },
    )
}

async fn compute(pool: &PgPool, storage: &Storage, algorithms: &[ChecksumAlgorithm]) -> Result<()> {
    const PAGE_SIZE: i64 = 1000;
    let total = checksums::count_missing(pool, algorithms).await?;
    JOB.update(|progress| progress.total = total);
    let mut after = String::new();
    loop {
        let rows = checksums::list_missing(pool, algorithms, &after, PAGE_SIZE).await?;
        let Some(last) = rows.last() else {
            return Ok(());
        };
        after = last.id.clone();
        for row in rows {
            check_shutdown()?;
            let path = storage.path_for(&row.id, &row.ext);
            match hash_file(&path, algorithms).await {
                Ok(digests) => checksums::set(pool, &row.id, algorithms, &digests).await?,
                // Uploads are never edited in place, so an unreadable file is gone.
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    JOB.update(|progress| progress.missing += 1);
                }
                Err(error) => return Err(error),
            }
            JOB.update(|progress| progress.done += 1);
        }
    }
}
