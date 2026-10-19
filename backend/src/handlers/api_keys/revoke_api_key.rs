use actix_web::{
    HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::api_keys;

#[utoipa::path(
    tag = "elysia",
    post,
    path = "/api/api-keys/{id}/revoke",
    description = "Revoke one of the caller's active API keys: it is refused from now on. The key \
        stays listed, so the files uploaded with it can still be traced to it.",
    params(("id" = String, Path, description = "API key id")),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "API key revoked"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Called with an API key"),
        (status = 404, description = "No active API key of the caller has this id"),
    )
)]
pub async fn revoke_api_key(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    authenticated.require_session()?;
    if !api_keys::revoke(pool.get_ref(), authenticated.user_id, *id).await? {
        return Err(Error::NotFound);
    }
    Ok(HttpResponse::NoContent().finish())
}
