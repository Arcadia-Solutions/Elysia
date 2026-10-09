use utoipa::OpenApi;

use crate::handlers::login::{LoginRequest, LoginResponse};
use crate::handlers::upload::{UploadForm, UploadOptionsQuery, UploadResponse};
use crate::handlers::upload_url::UploadUrlRequest;
use crate::services::files::{Actions, CompressionAction, ConvertAction, ResizeAction};
use crate::services::image::TargetFormat;
use crate::settings::{ElysiaSettings, PublicElysiaSettings};

#[derive(OpenApi)]
#[openapi(
    info(title = "Elysia image host API"),
    paths(
        crate::handlers::login::login,
        crate::handlers::upload::upload,
        crate::handlers::upload_url::upload_url,
        crate::handlers::serve::serve,
        crate::handlers::serve_thumbnail::serve_thumbnail,
        crate::handlers::settings::get_elysia_settings,
        crate::handlers::settings::get_public_elysia_settings,
        crate::handlers::settings::put_elysia_settings,
    ),
    components(schemas(
        LoginRequest,
        LoginResponse,
        UploadForm,
        UploadResponse,
        UploadUrlRequest,
        UploadOptionsQuery,
        Actions,
        ResizeAction,
        ConvertAction,
        CompressionAction,
        ElysiaSettings,
        PublicElysiaSettings,
        TargetFormat
    ))
)]
pub struct ApiDoc;
