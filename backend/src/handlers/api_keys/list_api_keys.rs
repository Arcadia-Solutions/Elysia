use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::api_keys::{self, ApiKeySummary};

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/api-keys",
    description = "The caller's API keys, revoked and expired ones included, newest first.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "API keys", body = Vec<ApiKeySummary>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Called with an API key"),
    )
)]
pub async fn list_api_keys(authenticated: AuthUser, pool: Data<PgPool>) -> Result<HttpResponse> {
    authenticated.require_session()?;
    let keys = api_keys::list(pool.get_ref(), authenticated.user_id).await?;
    Ok(HttpResponse::Ok().json(keys))
}
