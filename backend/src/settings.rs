use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::checksums::ChecksumAlgorithm;
use crate::services::image::TargetFormat;

/// Shared, hot-swappable elysia settings. Uploads read a snapshot; a save
/// replaces the whole value, so a change applies without a restart.
pub type SharedElysiaSettings = actix_web::web::Data<std::sync::RwLock<ElysiaSettings>>;

/// Runtime-editable elysia settings: the upload limits and processing options
/// edited from the web UI and persisted as the single `elysia_settings` row.
///
/// Kept separate from [`crate::config::Config`], which holds only the boot-time
/// infrastructure values (bind address, database, upload dir).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ElysiaSettings {
    /// Reject uploads larger than this; 0 disables the check.
    #[serde(default)]
    pub max_file_size_bytes: u64,
    /// Reject images wider than this; 0 disables the check.
    #[serde(default)]
    pub max_width_pixels: u32,
    /// Reject images taller than this; 0 disables the check.
    #[serde(default)]
    pub max_height_pixels: u32,
    /// Resize width bound; 0 = no resize.
    #[serde(default)]
    pub target_width_pixels: u32,
    /// Resize height bound; 0 = no resize.
    #[serde(default)]
    pub target_height_pixels: u32,
    /// Default output format; null stores uploads as-is (processing off).
    #[serde(default)]
    pub default_target_file_format: Option<TargetFormat>,
    /// Lossy target size in bytes; 0 = no target.
    #[serde(default)]
    pub target_file_size_bytes: u64,
    /// Lossy quality (1-100) applied to uploads; 0 = lossless.
    #[serde(default)]
    pub default_compression: u8,
    /// When true, an upload may ask for lossless instead of the default
    /// compression.
    #[serde(default)]
    pub allow_lossless: bool,
    /// When true, EXIF is stripped from an upload that does not
    /// request otherwise.
    #[serde(default)]
    pub strip_exif_by_default: bool,
    /// When false, an upload may not request its own EXIF-stripping choice and
    /// the default is always used.
    #[serde(default)]
    pub allow_overriding_strip_exif: bool,
    /// Medium box width; 0 on both axes disables medium images. Not overridable
    /// per upload.
    #[serde(default)]
    pub medium_width_pixels: u32,
    /// Medium box height; 0 on both axes disables medium images.
    #[serde(default)]
    pub medium_height_pixels: u32,
    /// Lossy WebP quality (1-100) for generated medium images.
    #[serde(default)]
    pub medium_quality: u8,
    /// Thumbnail box width; the image is cropped to the box aspect ratio, then
    /// shrunk into it. 0 on either axis disables thumbnails.
    #[serde(default)]
    pub thumbnail_width_pixels: u32,
    /// Thumbnail box height; 0 on either axis disables thumbnails.
    #[serde(default)]
    pub thumbnail_height_pixels: u32,
    /// Lossy WebP quality (1-100) for generated thumbnails.
    #[serde(default)]
    pub thumbnail_quality: u8,
    /// When true, anyone may create a local account via the register endpoint.
    /// When false, only an external sign-on backend can provision users.
    #[serde(default = "default_allow_registration")]
    pub allow_registration: bool,
    /// Hostnames (e.g. `example.com`) whose `Referer` may load `/i/` and `/m/`.
    /// Empty allows any request, including ones without a `Referer`. Elysia's
    /// own pages (its host, under the path of `server.public_url`) are always
    /// allowed so the web UI keeps showing images.
    #[serde(default)]
    pub allowed_referrers: Vec<String>,
    /// When false, every upload (file or URL) is rejected, admins included.
    #[serde(default = "default_uploads_enabled")]
    pub uploads_enabled: bool,
    /// Bytes of available and trashed files each user may hold, unless
    /// overridden per user; 0 is unlimited. Admins are exempt without an override.
    #[serde(default)]
    pub default_storage_quota_bytes: u64,
    /// When true, views are counted per `Referer` host (Elysia's own pages
    /// under their host and base path, see [`referer_host`]); when false, every view
    /// is counted under no referrer. Without `allowed_referrers`, any site (or
    /// anyone forging a `Referer`) adds a row per file and host, so the
    /// referrer views table can grow indefinitely: keep this off unless the
    /// allowlist bounds the hosts.
    #[serde(default)]
    pub record_referrer_views: bool,
    /// Terms of service, shown to everyone; empty hides the page and its links.
    #[serde(default)]
    pub terms_of_service: String,
    /// Privacy policy, shown to everyone; empty hides the page and its links.
    #[serde(default)]
    pub privacy_policy: String,
    /// Days after which an upload that requests no expiry is deleted; 0 never
    /// expires.
    #[serde(default)]
    pub default_expiry_days: u32,
    /// When false, an upload may not request its own expiry and the default is
    /// always used.
    #[serde(default)]
    pub allow_overriding_expiry: bool,
    /// When false, images cannot be added to or removed from albums.
    #[serde(default = "default_albums_enabled")]
    pub albums_enabled: bool,
    /// Who may rehost images from a URL.
    #[serde(default)]
    pub url_uploads: UrlUploads,
    /// Shown on the upload page while uploads are disabled; empty shows a
    /// generic notice.
    #[serde(default)]
    pub uploads_disabled_message: String,
    /// Checksums computed on upload besides SHA-256, which always is. Each one
    /// gives the image links by that checksum.
    #[serde(default)]
    pub checksum_algorithms: Vec<ChecksumAlgorithm>,
}

/// Longest expiry, in days, an upload or the default may set.
pub const MAX_EXPIRY_DAYS: u32 = 36500;

fn default_allow_registration() -> bool {
    true
}

fn default_uploads_enabled() -> bool {
    true
}

fn default_albums_enabled() -> bool {
    true
}

/// Who may rehost images from a URL; admins get no exception. Stored in the DB
/// as the `url_uploads` Postgres enum.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "url_uploads", rename_all = "snake_case")]
pub enum UrlUploads {
    Disabled,
    /// Only the SSO provider uploading on behalf of its users.
    SsoOnly,
    #[default]
    Everyone,
}

/// The subset of [`ElysiaSettings`] exposed to logged-in users, loaded once
/// by the frontend (upload validation, quota defaults).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PublicElysiaSettings {
    pub max_file_size_bytes: u64,
    pub max_width_pixels: u32,
    pub max_height_pixels: u32,
    pub target_width_pixels: u32,
    pub target_height_pixels: u32,
    pub default_target_file_format: Option<TargetFormat>,
    /// Whether an upload may ask for lossless; drives the lossless toggle on
    /// the upload page.
    pub allow_lossless: bool,
    /// Whether an upload may choose to strip its own metadata; drives the
    /// EXIF-stripping control on the upload page.
    pub allow_overriding_strip_exif: bool,
    /// Default EXIF-stripping choice; pre-sets the control on the upload page.
    pub strip_exif_by_default: bool,
    pub target_file_size_bytes: u64,
    /// Whether open registration is allowed; lets the login page offer signup.
    pub allow_registration: bool,
    /// Whether uploads are accepted; lets the upload page say they are paused.
    pub uploads_enabled: bool,
    /// Storage quota of users without an override; 0 is unlimited.
    pub default_storage_quota_bytes: u64,
    /// Expiry of uploads that request none, in days; 0 never expires.
    pub default_expiry_days: u32,
    /// Whether an upload may pick its own expiry; drives the expiry control on
    /// the upload page.
    pub allow_overriding_expiry: bool,
    /// Whether images may be added to or removed from albums; drives the album
    /// controls on the upload, image and album pages.
    pub albums_enabled: bool,
    /// Whether users may rehost images from a URL themselves; drives the URL
    /// field on the upload page.
    pub url_uploads_enabled: bool,
    /// Shown on the upload page while uploads are disabled; empty shows a
    /// generic notice.
    pub uploads_disabled_message: String,
}

impl From<ElysiaSettings> for PublicElysiaSettings {
    fn from(s: ElysiaSettings) -> Self {
        Self {
            max_file_size_bytes: s.max_file_size_bytes,
            max_width_pixels: s.max_width_pixels,
            max_height_pixels: s.max_height_pixels,
            target_width_pixels: s.target_width_pixels,
            target_height_pixels: s.target_height_pixels,
            default_target_file_format: s.default_target_file_format,
            allow_lossless: s.allow_lossless,
            allow_overriding_strip_exif: s.allow_overriding_strip_exif,
            strip_exif_by_default: s.strip_exif_by_default,
            target_file_size_bytes: s.target_file_size_bytes,
            allow_registration: s.allow_registration,
            uploads_enabled: s.uploads_enabled,
            default_storage_quota_bytes: s.default_storage_quota_bytes,
            default_expiry_days: s.default_expiry_days,
            allow_overriding_expiry: s.allow_overriding_expiry,
            albums_enabled: s.albums_enabled,
            url_uploads_enabled: s.url_uploads == UrlUploads::Everyone,
            uploads_disabled_message: s.uploads_disabled_message,
        }
    }
}

/// The subset of settings exposed without authentication, so the
/// login and register pages can offer signup and single sign-on.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnauthElysiaSettings {
    /// Whether open registration is allowed; lets the login page offer signup.
    pub allow_registration: bool,
    /// Name of the single sign-on provider, when enabled; lets the login page
    /// offer "Login with <name>".
    pub sso_provider_name: Option<String>,
    /// Terms of service; empty when not set.
    pub terms_of_service: String,
    /// Privacy policy; empty when not set.
    pub privacy_policy: String,
}

/// Lowercased host of a `Referer`, sorted into Elysia's own pages and the rest.
pub enum RefererHost {
    /// The host followed by `base_path` (`domain.com/elysia`), so views from
    /// Elysia stay apart from those of another site on the same host.
    Own(String),
    Other(String),
}

/// Classify the `Referer` against `own_host` (the `Host` the request was sent
/// to, port ignored) and `base_path` (see `ServerConfig::base_path`): a page of
/// another site on the same host, outside `base_path`, is not one of Elysia's
/// own. None when there is no parseable `Referer`.
pub fn referer_host(referer: Option<&str>, own_host: &str, base_path: &str) -> Option<RefererHost> {
    let url = referer.and_then(|referer| url::Url::parse(referer).ok())?;
    let host = url.host_str().map(str::to_ascii_lowercase)?;
    let own_host = url::Url::parse(&format!("http://{own_host}"))
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase));
    let under_base_path = url
        .path()
        .strip_prefix(base_path)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
    Some(
        if own_host.as_deref() == Some(host.as_str()) && under_base_path {
            RefererHost::Own(format!("{host}{base_path}"))
        } else {
            RefererHost::Other(host)
        },
    )
}

impl ElysiaSettings {
    /// Trim and lowercase the referrer hostnames, dropping blank ones, so the
    /// stored list matches what [`Self::referrer_allowed`] compares against.
    pub fn normalize(&mut self) {
        self.allowed_referrers = self
            .allowed_referrers
            .iter()
            .map(|host| host.trim().to_ascii_lowercase())
            .filter(|host| !host.is_empty())
            .collect();
        self.allowed_referrers.sort();
        self.allowed_referrers.dedup();
        self.checksum_algorithms
            .retain(|algorithm| *algorithm != ChecksumAlgorithm::Sha256);
        self.checksum_algorithms.sort();
        self.checksum_algorithms.dedup();
    }

    /// Whether an image request may be served: the list is empty, or the
    /// `Referer` host is listed or is one of Elysia's own pages (see [`referer_host`]).
    pub fn referrer_allowed(&self, referer: Option<&str>, own_host: &str, base_path: &str) -> bool {
        if self.allowed_referrers.is_empty() {
            return true;
        }
        match referer_host(referer, own_host, base_path) {
            Some(RefererHost::Own(_)) => true,
            Some(RefererHost::Other(host)) => self.allowed_referrers.contains(&host),
            None => false,
        }
    }

    /// Whether a terms of service or privacy policy is set, so new accounts
    /// must accept them.
    pub fn legal_set(&self) -> bool {
        !self.terms_of_service.is_empty() || !self.privacy_policy.is_empty()
    }

    /// Whether medium generation is on (a box is set on at least one axis).
    pub fn medium_enabled(&self) -> bool {
        self.medium_width_pixels > 0 || self.medium_height_pixels > 0
    }

    /// Whether thumbnail generation is on (a box is set on both axes).
    pub fn thumbnail_enabled(&self) -> bool {
        self.thumbnail_width_pixels > 0 && self.thumbnail_height_pixels > 0
    }

    /// Reject contradictory settings. Enforced on every save so the running
    /// state and the stored row are always consistent.
    pub fn validate(&self) -> Result<(), String> {
        // Processing options require a target format: we will not re-encode to
        // an unknown format.
        if self.default_target_file_format.is_none()
            && (self.target_width_pixels > 0
                || self.target_height_pixels > 0
                || self.target_file_size_bytes > 0)
        {
            return Err(
                "default_target_file_format is required when target_width_pixels, \
                 target_height_pixels or target_file_size_bytes is set"
                    .into(),
            );
        }
        // An upload with no byte ceiling is an unbounded disk/memory write, so
        // require one always.
        if self.max_file_size_bytes == 0 {
            return Err("max_file_size_bytes must be > 0".into());
        }
        // A default compression is a lossy quality, so it needs both a valid
        // range and a format to re-encode to.
        if self.default_compression > 100 {
            return Err("default_compression must be between 0 and 100".into());
        }
        if self.default_compression > 0 && self.default_target_file_format.is_none() {
            return Err(
                "default_compression requires a default_target_file_format to re-encode to".into(),
            );
        }
        if self.default_compression > 0
            && self.default_target_file_format == Some(TargetFormat::Png)
        {
            return Err("png has no lossy mode, default_compression must be 0".into());
        }
        // AVIF and JPEG have no lossless mode, so they need a default compression.
        if matches!(self.default_target_file_format, Some(format) if !format.supports_lossless())
            && self.default_compression == 0
        {
            return Err(
                "default_compression is required for a format with no lossless mode (AVIF, JPEG)"
                    .into(),
            );
        }
        // Lossless is only an alternative to a lossy default, in a format that has it.
        if self.allow_lossless
            && (self.default_compression == 0
                || !self
                    .default_target_file_format
                    .is_some_and(|format| format.supports_lossless()))
        {
            return Err(
                "allow_lossless requires a default_compression and a format with a lossless mode \
                 (WebP, JPEG XL)"
                    .into(),
            );
        }
        // Processing decodes the source into memory (width * height * 4 bytes),
        // so it must run behind pixel caps or a small file declaring huge
        // dimensions is a decompression bomb.
        if self.default_target_file_format.is_some()
            && (self.max_width_pixels == 0 || self.max_height_pixels == 0)
        {
            return Err(
                "max_width_pixels and max_height_pixels must be > 0 when a target file \
                 format is set (they bound the server-side decode)"
                    .into(),
            );
        }
        // Preview generation decodes the source too, so it needs the same pixel
        // caps, and a valid lossy WebP quality.
        if (self.medium_enabled() || self.thumbnail_enabled())
            && (self.max_width_pixels == 0 || self.max_height_pixels == 0)
        {
            return Err(
                "max_width_pixels and max_height_pixels must be > 0 when medium images or \
                 thumbnails are enabled (they bound the server-side decode)"
                    .into(),
            );
        }
        if self.medium_enabled() && !(1..=100).contains(&self.medium_quality) {
            return Err("medium_quality must be between 1 and 100".into());
        }
        if self.thumbnail_enabled() && !(1..=100).contains(&self.thumbnail_quality) {
            return Err("thumbnail_quality must be between 1 and 100".into());
        }
        // Stored as a signed bigint, so a larger count would wrap negative.
        if self.default_storage_quota_bytes > i64::MAX as u64 {
            return Err(format!(
                "default_storage_quota_bytes must be <= {}",
                i64::MAX
            ));
        }
        if self.default_expiry_days > MAX_EXPIRY_DAYS {
            return Err(format!("default_expiry_days must be <= {MAX_EXPIRY_DAYS}"));
        }
        if let Some(host) = self
            .allowed_referrers
            .iter()
            .find(|host| host.contains(['/', ':']) || host.contains(char::is_whitespace))
        {
            return Err(format!(
                "allowed_referrers entry `{host}` must be a bare hostname, e.g. example.com"
            ));
        }
        Ok(())
    }
}
