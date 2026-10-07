use utoipa::OpenApi;

use crate::handlers::login::{LoginRequest, LoginResponse};
use crate::handlers::upload::{UploadForm, UploadResponse};

#[derive(OpenApi)]
#[openapi(
    info(title = "Elysia image host API"),
    paths(
        crate::handlers::login::login,
        crate::handlers::upload::upload,
        crate::handlers::serve::serve,
    ),
    components(schemas(LoginRequest, LoginResponse, UploadForm, UploadResponse))
)]
pub struct ApiDoc;
