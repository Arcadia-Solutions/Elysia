use utoipa::OpenApi;

use crate::handlers::login::{LoginRequest, LoginResponse};
use crate::handlers::upload::{UploadForm, UploadOptionsQuery, UploadResponse};
use crate::handlers::upload_url::UploadUrlRequest;
use crate::services::files::{Actions, CompressionAction, ConvertAction, ResizeAction};

#[derive(OpenApi)]
#[openapi(
    info(title = "Elysia image host API"),
    paths(
        crate::handlers::login::login,
        crate::handlers::upload::upload,
        crate::handlers::upload_url::upload_url,
        crate::handlers::serve::serve,
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
        CompressionAction
    ))
)]
pub struct ApiDoc;
