use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    /// When false, an upload may not request its own target file format and the
    /// default is always used.
    #[serde(default)]
    pub allow_overriding_file_format: bool,
    /// Lossy target size in bytes; 0 = no target.
    #[serde(default)]
    pub target_file_size_bytes: u64,
    /// Lossy quality (1-100) applied when an upload requests none; 0 = none.
    #[serde(default)]
    pub default_compression: u8,
    /// When false, an upload may not request its own compression and the
    /// default is always used.
    #[serde(default)]
    pub allow_overriding_compression: bool,
    /// When true, EXIF is stripped from an upload that does not
    /// request otherwise.
    #[serde(default)]
    pub strip_exif_by_default: bool,
    /// When false, an upload may not request its own EXIF-stripping choice and
    /// the default is always used.
    #[serde(default)]
    pub allow_overriding_strip_exif: bool,
    /// Thumbnail box width; 0 on both axes disables thumbnails. Not overridable
    /// per upload.
    #[serde(default)]
    pub thumbnail_width_pixels: u32,
    /// Thumbnail box height; 0 on both axes disables thumbnails.
    #[serde(default)]
    pub thumbnail_height_pixels: u32,
    /// Lossy WebP quality (1-100) for generated thumbnails.
    #[serde(default)]
    pub thumbnail_quality: u8,
    /// When true, anyone may create a local account via the register endpoint.
    /// When false, only an external sign-on backend can provision users.
    #[serde(default = "default_allow_registration")]
    pub allow_registration: bool,
    /// Hostnames (e.g. `example.com`) whose `Referer` may load `/i/` and `/t/`.
    /// Empty allows any request, including ones without a `Referer`. Elysia's
    /// own host is always allowed so the web UI keeps showing images.
    #[serde(default)]
    pub allowed_referrers: Vec<String>,
    /// When false, every upload (file or URL) is rejected, admins included.
    #[serde(default = "default_uploads_enabled")]
    pub uploads_enabled: bool,
    /// Bytes of available and trashed files each user may hold, unless
    /// overridden per user; 0 is unlimited. Admins are exempt without an override.
    #[serde(default)]
    pub default_storage_quota_bytes: u64,
    /// When true, views are counted per `Referer` host; when false, every view
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
    /// When false, images cannot be rehosted from a URL, admins included.
    #[serde(default = "default_url_uploads_enabled")]
    pub url_uploads_enabled: bool,
    /// Shown on the upload page while uploads are disabled; empty shows a
    /// generic notice.
    #[serde(default)]
    pub uploads_disabled_message: String,
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

fn default_url_uploads_enabled() -> bool {
    true
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
    /// Whether an upload may pick its own target file format; drives the
    /// format selector on the upload page.
    pub allow_overriding_file_format: bool,
    /// Whether an upload may request its own compression quality; drives the
    /// quality slider on the upload page.
    pub allow_overriding_compression: bool,
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
    /// Whether images may be rehosted from a URL; drives the URL field on the
    /// upload page.
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
            allow_overriding_file_format: s.allow_overriding_file_format,
            allow_overriding_compression: s.allow_overriding_compression,
            allow_overriding_strip_exif: s.allow_overriding_strip_exif,
            strip_exif_by_default: s.strip_exif_by_default,
            target_file_size_bytes: s.target_file_size_bytes,
            allow_registration: s.allow_registration,
            uploads_enabled: s.uploads_enabled,
            default_storage_quota_bytes: s.default_storage_quota_bytes,
            default_expiry_days: s.default_expiry_days,
            allow_overriding_expiry: s.allow_overriding_expiry,
            albums_enabled: s.albums_enabled,
            url_uploads_enabled: s.url_uploads_enabled,
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
    Own,
    Other(String),
}

/// Classify the `Referer` host against `own_host` (the `Host` the request was
/// sent to, port ignored). None when there is no parseable `Referer`.
pub fn referer_host(referer: Option<&str>, own_host: &str) -> Option<RefererHost> {
    let host = referer
        .and_then(|referer| url::Url::parse(referer).ok())
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))?;
    let own_host = url::Url::parse(&format!("http://{own_host}"))
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase));
    Some(if own_host.as_deref() == Some(host.as_str()) {
        RefererHost::Own
    } else {
        RefererHost::Other(host)
    })
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
    }

    /// Whether an image request may be served: the list is empty, or the
    /// `Referer` host is listed or is `own_host` (see [`referer_host`]).
    pub fn referrer_allowed(&self, referer: Option<&str>, own_host: &str) -> bool {
        if self.allowed_referrers.is_empty() {
            return true;
        }
        match referer_host(referer, own_host) {
            Some(RefererHost::Own) => true,
            Some(RefererHost::Other(host)) => self.allowed_referrers.contains(&host),
            None => false,
        }
    }

    /// Whether a terms of service or privacy policy is set, so new accounts
    /// must accept them.
    pub fn legal_set(&self) -> bool {
        !self.terms_of_service.is_empty() || !self.privacy_policy.is_empty()
    }

    /// Whether thumbnail generation is on (a box is set on at least one axis).
    pub fn thumbnails_enabled(&self) -> bool {
        self.thumbnail_width_pixels > 0 || self.thumbnail_height_pixels > 0
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
        // AVIF and JPEG have no lossless mode, so they can only be produced with
        // a selected quality. If such a format can be the output (it is the default, or an
        // upload may pick it) there must be a quality source: a default
        // compression, or the ability to request one per upload.
        let lossless_incapable_reachable = self.allow_overriding_file_format
            || self
                .default_target_file_format
                .is_some_and(|format| !format.supports_lossless());
        if lossless_incapable_reachable
            && self.default_compression == 0
            && !self.allow_overriding_compression
        {
            return Err(
                "allow_overriding_compression cannot be disabled without a default_compression \
                 when a format with no lossless mode can be produced (AVIF, JPEG)"
                    .into(),
            );
        }
        // Processing decodes the source into memory (width * height * 4 bytes),
        // so it must run behind pixel caps or a small file declaring huge
        // dimensions is a decompression bomb. An allowed per-upload format
        // override can trigger a decode even with no default format, so the caps
        // are required then too.
        if (self.default_target_file_format.is_some() || self.allow_overriding_file_format)
            && (self.max_width_pixels == 0 || self.max_height_pixels == 0)
        {
            return Err(
                "max_width_pixels and max_height_pixels must be > 0 when a target file \
                 format is set or overriding it is allowed (they bound the server-side decode)"
                    .into(),
            );
        }
        // Thumbnail generation decodes the source too, so it needs the same pixel
        // caps, and a valid lossy WebP quality.
        if self.thumbnails_enabled() {
            if self.max_width_pixels == 0 || self.max_height_pixels == 0 {
                return Err(
                    "max_width_pixels and max_height_pixels must be > 0 when thumbnails are \
                     enabled (they bound the server-side decode)"
                        .into(),
                );
            }
            if self.thumbnail_quality < 1 || self.thumbnail_quality > 100 {
                return Err("thumbnail_quality must be between 1 and 100".into());
            }
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
