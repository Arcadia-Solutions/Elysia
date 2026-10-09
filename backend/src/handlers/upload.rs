use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use crate::error::{Error, Result};
use crate::handlers::settings::SharedElysiaSettings;
use crate::middlewares::AdminAuth;
use crate::services::files::{self, Actions, UploadOptions};
use crate::services::image::TargetFormat;
use crate::settings::ElysiaSettings;
use crate::storage::Storage;

#[derive(Debug, MultipartForm, ToSchema)]
pub struct UploadForm {
    // No per-field limit: the overall cap is MultipartFormConfig::total_limit,
    // set from config.storage.max_upload_stream_bytes in main.rs. TempFile streams to
    // disk, so large uploads never buffer in RAM; the per-request size ceiling is the
    // UI-editable elysia setting max_file_size_bytes, checked in the store path.
    #[schema(value_type = String, format = Binary, content_media_type = "application/octet-stream")]
    pub file: TempFile,
}

#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    pub id: String,
    pub ext: String,
    pub url: String,
    /// True when the image was already hosted and this is the existing one
    /// (deduplicated), false when it was newly stored.
    pub existed: bool,
    /// What processing changed; null when the file is stored as uploaded.
    pub actions: Option<Actions>,
}

/// Per-upload processing options (query string, shared by both upload routes).
#[derive(Debug, Default, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UploadOptionsQuery {
    /// Requested lossy quality, 1 to 100; absent means lossless. Requires a
    /// configured target format.
    #[param(minimum = 1, maximum = 100)]
    pub lossy_compression_value: Option<u8>,
    /// Requested output format; absent uses the configured default. Rejected
    /// unless overriding the file format is allowed in settings.
    pub target_file_format: Option<TargetFormat>,
    /// Requested EXIF-stripping choice; absent uses the configured default.
    /// Rejected unless overriding EXIF stripping is allowed in settings.
    pub strip_exif: Option<bool>,
}

impl UploadOptionsQuery {
    /// Convert to service options: reject a client-supplied quality when
    /// overriding is disabled, fall back to the configured default, then reject
    /// compression when processing is off or the quality is out of range.
    pub fn into_options(self, settings: &ElysiaSettings) -> Result<UploadOptions> {
        if self.lossy_compression_value.is_some() && !settings.allow_overriding_compression {
            return Err(Error::BadRequest(
                "overriding compression is disabled".into(),
            ));
        }
        if self.target_file_format.is_some() && !settings.allow_overriding_file_format {
            return Err(Error::BadRequest(
                "overriding the target file format is disabled".into(),
            ));
        }
        if self.strip_exif.is_some() && !settings.allow_overriding_strip_exif {
            return Err(Error::BadRequest(
                "overriding EXIF stripping is disabled".into(),
            ));
        }
        let strip_exif = self.strip_exif.unwrap_or(settings.strip_exif_by_default);
        let target_file_format = self
            .target_file_format
            .or(settings.default_target_file_format);
        let default = (settings.default_compression > 0).then_some(settings.default_compression);
        let lossy_compression_value = self.lossy_compression_value.or(default);
        if lossy_compression_value.is_some() && target_file_format.is_none() {
            return Err(Error::BadRequest(
                "compression is disabled: no target file format configured".into(),
            ));
        }
        if lossy_compression_value.is_some_and(|q| !(1..=100).contains(&q)) {
            return Err(Error::BadRequest(
                "lossy_compression_value must be between 1 and 100".into(),
            ));
        }
        Ok(UploadOptions {
            lossy_compression_value,
            target_file_format,
            strip_exif,
        })
    }
}

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/upload",
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    params(UploadOptionsQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Image uploaded", body = UploadResponse),
        (status = 400, description = "File too large"),
        (status = 401, description = "Missing or invalid admin token"),
        (status = 415, description = "Not a supported image"),
    )
)]
pub async fn upload(
    _auth: AdminAuth,
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    query: Query<UploadOptionsQuery>,
    MultipartForm(form): MultipartForm<UploadForm>,
) -> Result<HttpResponse> {
    let settings = *settings.read().expect("settings lock poisoned");
    let options = query.into_inner().into_options(&settings)?;
    let stored = files::store_upload(
        pool.get_ref(),
        storage.get_ref(),
        &settings,
        form.file,
        &options,
    )
    .await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: format!("/i/{}.{}", stored.id, stored.ext),
        id: stored.id,
        ext: stored.ext,
        existed: stored.existed,
        actions: stored.actions,
    }))
}
