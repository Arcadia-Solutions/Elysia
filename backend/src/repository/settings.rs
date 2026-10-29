use sqlx::PgPool;

use crate::error::Result;
use crate::services::image::TargetFormat;
use crate::settings::ElysiaSettings;

/// The settings table holds exactly one row under this id.
const SINGLETON_ID: &str = "singleton";

/// Load the single elysia settings row. The migration seeds it, so this
/// always finds a row on a migrated database.
pub async fn load(pool: &PgPool) -> Result<ElysiaSettings> {
    let row = sqlx::query!(
        r#"select max_file_size_bytes, max_width_pixels, max_height_pixels,
                  target_width_pixels, target_height_pixels,
                  default_target_file_format as "default_target_file_format?: TargetFormat",
                  allow_overriding_file_format,
                  target_file_size_bytes, default_compression,
                  allow_overriding_compression,
                   strip_exif_by_default, allow_overriding_strip_exif,
                   thumbnail_width_pixels, thumbnail_height_pixels, thumbnail_quality,
                   allow_registration, allowed_referrers, uploads_enabled,
                   default_storage_quota_bytes, record_referrer_views,
                   terms_of_service, privacy_policy,
                   default_expiry_days, allow_overriding_expiry, albums_enabled
           from elysia_settings where id = $1"#,
        SINGLETON_ID,
    )
    .fetch_one(pool)
    .await?;

    Ok(ElysiaSettings {
        max_file_size_bytes: row.max_file_size_bytes as u64,
        max_width_pixels: row.max_width_pixels as u32,
        max_height_pixels: row.max_height_pixels as u32,
        target_width_pixels: row.target_width_pixels as u32,
        target_height_pixels: row.target_height_pixels as u32,
        default_target_file_format: row.default_target_file_format,
        allow_overriding_file_format: row.allow_overriding_file_format,
        target_file_size_bytes: row.target_file_size_bytes as u64,
        default_compression: row.default_compression as u8,
        allow_overriding_compression: row.allow_overriding_compression,
        strip_exif_by_default: row.strip_exif_by_default,
        allow_overriding_strip_exif: row.allow_overriding_strip_exif,
        thumbnail_width_pixels: row.thumbnail_width_pixels as u32,
        thumbnail_height_pixels: row.thumbnail_height_pixels as u32,
        thumbnail_quality: row.thumbnail_quality as u8,
        allow_registration: row.allow_registration,
        allowed_referrers: row.allowed_referrers,
        uploads_enabled: row.uploads_enabled,
        default_storage_quota_bytes: row.default_storage_quota_bytes as u64,
        record_referrer_views: row.record_referrer_views,
        terms_of_service: row.terms_of_service,
        privacy_policy: row.privacy_policy,
        default_expiry_days: row.default_expiry_days as u32,
        allow_overriding_expiry: row.allow_overriding_expiry,
        albums_enabled: row.albums_enabled,
    })
}

/// Overwrite the single elysia settings row. Validation happens in the handler
/// before this is called.
pub async fn save(pool: &PgPool, settings: &ElysiaSettings) -> Result<()> {
    sqlx::query!(
        r#"update elysia_settings set
            max_file_size_bytes = $1, max_width_pixels = $2, max_height_pixels = $3,
            target_width_pixels = $4, target_height_pixels = $5,
            default_target_file_format = $6, allow_overriding_file_format = $7,
            target_file_size_bytes = $8,
            default_compression = $9, allow_overriding_compression = $10,
            strip_exif_by_default = $11, allow_overriding_strip_exif = $12,
            thumbnail_width_pixels = $13, thumbnail_height_pixels = $14,
            thumbnail_quality = $15, allow_registration = $16,
            allowed_referrers = $17, uploads_enabled = $18,
            default_storage_quota_bytes = $19, record_referrer_views = $20,
            terms_of_service = $21, privacy_policy = $22,
            default_expiry_days = $23, allow_overriding_expiry = $24,
            albums_enabled = $25
         where id = $26"#,
        settings.max_file_size_bytes as i64,
        settings.max_width_pixels as i32,
        settings.max_height_pixels as i32,
        settings.target_width_pixels as i32,
        settings.target_height_pixels as i32,
        settings.default_target_file_format as Option<TargetFormat>,
        settings.allow_overriding_file_format,
        settings.target_file_size_bytes as i64,
        settings.default_compression as i32,
        settings.allow_overriding_compression,
        settings.strip_exif_by_default,
        settings.allow_overriding_strip_exif,
        settings.thumbnail_width_pixels as i32,
        settings.thumbnail_height_pixels as i32,
        settings.thumbnail_quality as i32,
        settings.allow_registration,
        &settings.allowed_referrers,
        settings.uploads_enabled,
        settings.default_storage_quota_bytes as i64,
        settings.record_referrer_views,
        &settings.terms_of_service,
        &settings.privacy_policy,
        settings.default_expiry_days as i32,
        settings.allow_overriding_expiry,
        settings.albums_enabled,
        SINGLETON_ID,
    )
    .execute(pool)
    .await?;
    Ok(())
}
