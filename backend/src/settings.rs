use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::image::TargetFormat;

/// Runtime-editable elysia settings: the upload limits and processing options
/// edited from the web UI and persisted as the single `elysia_settings` row.
///
/// Kept separate from [`crate::config::Config`], which holds only the boot-time
/// infrastructure values (bind address, database, upload dir, stream ceiling).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
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
}

/// The subset of [`ElysiaSettings`] exposed publicly, so the upload page
/// can validate files client-side.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
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
        }
    }
}

impl ElysiaSettings {
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
        Ok(())
    }
}
