use std::io::Write;

use actix_multipart::Multipart;
use actix_web::{
    HttpRequest, HttpResponse,
    http::header::CONTENT_LENGTH,
    web::{Data, Query},
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::middlewares::UploadUser;
use crate::services::files::{self, Actions, UploadOptions};
use crate::services::image::TargetFormat;
use crate::settings::ElysiaSettings;
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

/// OpenAPI shape of the upload body. The handler streams the multipart body by
/// hand instead of extracting this, so the size cap applies while it streams.
#[derive(ToSchema)]
pub struct UploadForm {
    #[schema(value_type = String, format = Binary, content_media_type = "application/octet-stream")]
    pub file: Vec<u8>,
}

/// Room for the multipart boundaries and part headers on top of the file.
const MULTIPART_OVERHEAD: u64 = 16 * 1024;

#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    pub id: String,
    pub ext: String,
    pub url: String,
    /// URL of the thumbnail; a generated WebP when one exists, otherwise the
    /// original file served from `/t/`.
    pub thumbnail_url: String,
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
    /// Requested lossy quality, 1 to 100, or 0 for lossless; absent uses the
    /// configured default. Requires a configured target format.
    #[param(minimum = 0, maximum = 100)]
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
        if self.lossy_compression_value.is_some_and(|q| q > 100) {
            return Err(Error::BadRequest(
                "lossy_compression_value must be between 0 and 100".into(),
            ));
        }
        // 0 explicitly requests lossless, overriding the default quality.
        let lossy_compression_value = match self.lossy_compression_value {
            Some(0) => None,
            requested => requested.or(default),
        };
        if lossy_compression_value.is_some() && target_file_format.is_none() {
            return Err(Error::BadRequest(
                "compression is disabled: no target file format configured".into(),
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
    security(("bearer" = []), ("sso_client_basic" = [])),
    responses(
        (status = 200, description = "Image uploaded", body = UploadResponse),
        (status = 400, description = "File too large"),
        (status = 401, description = "Missing or invalid session, or invalid SSO client credentials. The SSO provider may upload on behalf of its users with its client credentials as Basic auth and the user subject in the On-Behalf-Of-Sub header (On-Behalf-Of-Username names the account on first upload)"),
        (status = 415, description = "Not a supported image"),
    )
)]
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    authenticated: UploadUser,
    config: Data<Config>,
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    query: Query<UploadOptionsQuery>,
    request: HttpRequest,
    mut multipart: Multipart,
) -> Result<HttpResponse> {
    let settings = settings.read().expect("settings lock poisoned").clone();
    let options = query.into_inner().into_options(&settings)?;

    // The body is only read here, after the extractors (auth included) passed.
    // Refuse a declared oversized body up front, then cap what actually streams
    // to disk with the live max_file_size_bytes, counting every part.
    let declared = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok()?.parse::<u64>().ok())
        .unwrap_or(0);
    files::check_size(&settings, declared.saturating_sub(MULTIPART_OVERHEAD))?;
    let multipart_error = |e: actix_multipart::MultipartError| Error::BadRequest(e.to_string());
    let mut size = 0u64;
    let mut upload = None;
    while let Some(mut field) = multipart.try_next().await.map_err(multipart_error)? {
        let is_file = field.name() == Some("file");
        let file_name = field
            .content_disposition()
            .and_then(|disposition| disposition.get_filename())
            .map(str::to_string);
        let mut temp = is_file.then(|| storage.new_temp()).transpose()?;
        let mut file_size = 0u64;
        while let Some(chunk) = field.try_next().await.map_err(multipart_error)? {
            size += chunk.len() as u64;
            file_size += chunk.len() as u64;
            files::check_size(&settings, size)?;
            if let Some(temp) = &mut temp {
                temp.write_all(&chunk)?;
            }
        }
        if let Some(temp) = temp {
            upload = Some((temp, file_size, file_name));
            break;
        }
    }
    let (temp, file_size, file_name) =
        upload.ok_or_else(|| Error::BadRequest("missing file field".into()))?;

    let stored = files::store_upload(
        pool.get_ref(),
        storage.get_ref(),
        &settings,
        temp,
        file_size,
        file_name,
        &options,
        authenticated.user_id,
    )
    .await?;

    Ok(HttpResponse::Ok().json(UploadResponse {
        url: stored.image_url(&config.server.public_url),
        thumbnail_url: stored.thumbnail_url(&config.server.public_url),
        id: stored.id,
        ext: stored.ext,
        existed: stored.existed,
        actions: stored.actions,
    }))
}
