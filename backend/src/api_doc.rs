use utoipa::OpenApi;

use crate::handlers::change_password::ChangePasswordRequest;
use crate::handlers::images::{ImageSummary, ImagesPage};
use crate::handlers::login::{LoginRequest, LoginResponse};
use crate::handlers::me::ProfileResponse;
use crate::handlers::metadata::MetadataResponse;
use crate::handlers::register::{RegisterRequest, RegisterResponse};
use crate::handlers::upload::{UploadForm, UploadOptionsQuery, UploadResponse};
use crate::handlers::upload_url::UploadUrlRequest;
use crate::services::files::{Actions, CompressionAction, ConvertAction, ResizeAction};
use crate::services::image::TargetFormat;
use crate::settings::{ElysiaSettings, PublicElysiaSettings, UnauthElysiaSettings};

#[derive(OpenApi)]
#[openapi(
    info(title = "Elysia image host API"),
    paths(
        crate::handlers::register::register,
        crate::handlers::login::login,
        crate::handlers::me::me,
        crate::handlers::change_password::change_password,
        crate::handlers::upload::upload,
        crate::handlers::upload_url::upload_url,
        crate::handlers::serve::serve,
        crate::handlers::serve_thumbnail::serve_thumbnail,
        crate::handlers::metadata::metadata,
        crate::handlers::images::images,
        crate::handlers::get_elysia_settings::get_elysia_settings,
        crate::handlers::get_public_elysia_settings::get_public_elysia_settings,
        crate::handlers::get_elysia_unauth_settings::get_elysia_unauth_settings,
        crate::handlers::put_elysia_settings::put_elysia_settings,
    ),
    components(schemas(
        RegisterRequest,
        RegisterResponse,
        LoginRequest,
        LoginResponse,
        ProfileResponse,
        ChangePasswordRequest,
        MetadataResponse,
        ImageSummary,
        ImagesPage,
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
        UnauthElysiaSettings,
        TargetFormat
    ))
)]
pub struct ApiDoc;
