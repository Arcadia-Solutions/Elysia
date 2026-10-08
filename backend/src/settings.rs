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
    /// Output format; null stores uploads as-is (processing off).
    #[serde(default)]
    pub target_file_format: Option<TargetFormat>,
    /// Lossy target size in bytes; 0 = no target.
    #[serde(default)]
    pub target_file_size_bytes: u64,
}

impl ElysiaSettings {
    /// Reject contradictory settings. Enforced on every save so the running
    /// state and the stored row are always consistent.
    pub fn validate(&self) -> Result<(), String> {
        // Processing options require a target format: we will not re-encode to
        // an unknown format.
        if self.target_file_format.is_none()
            && (self.target_width_pixels > 0
                || self.target_height_pixels > 0
                || self.target_file_size_bytes > 0)
        {
            return Err("target_file_format is required when target_width_pixels, \
                 target_height_pixels or target_file_size_bytes is set"
                .into());
        }
        // An upload with no byte ceiling is an unbounded disk/memory write, so
        // require one always.
        if self.max_file_size_bytes == 0 {
            return Err("max_file_size_bytes must be > 0".into());
        }
        // Processing decodes the source into memory (width * height * 4 bytes),
        // so it must run behind pixel caps or a small file declaring huge
        // dimensions is a decompression bomb.
        if self.target_file_format.is_some()
            && (self.max_width_pixels == 0 || self.max_height_pixels == 0)
        {
            return Err("max_width_pixels and max_height_pixels must be > 0 when \
                 target_file_format is set (they bound the server-side decode)"
                .into());
        }
        Ok(())
    }
}
