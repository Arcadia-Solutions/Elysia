use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::config::Config;
use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::chevereto_import::{self, CheveretoImportParams};
use crate::settings::SharedElysiaSettings;
use crate::storage::Storage;

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/maintenance/chevereto-import",
    description = "Start copying every image of a Chevereto v4 instance into Elysia in the \
        background, through the upload pipeline with the size limits lifted, keeping uploaders \
        and upload dates. With a backend database, the Chevereto links in its text-like columns \
        are rewritten to Elysia links after each chunk. Progress is saved per chunk, so a \
        stopped import resumes. Follow it with `GET /api/maintenance/chevereto-import/progress`.",
    request_body = CheveretoImportParams,
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Import started"),
        (status = 400, description = "Invalid parameters, or a needed service (Chevereto database or site, backend database or users table) failed its check"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 409, description = "An import is already running"),
    )
)]
pub async fn start_chevereto_import(
    authenticated: AuthUser,
    config: Data<Config>,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    settings: SharedElysiaSettings,
    body: Json<CheveretoImportParams>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let settings = settings.read().expect("settings lock poisoned").clone();
    chevereto_import::start(
        pool.get_ref().clone(),
        storage,
        &config,
        settings,
        body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Accepted().finish())
}
