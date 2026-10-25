use actix_web::{HttpResponse, web::Data};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::sessions;

#[utoipa::path(
    tag = "Auth",
    post,
    path = "/api/auth/logout",
    description = "Revoke the caller's session token. Other sessions of the user stay valid.",
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Logged out"),
        (status = 401, description = "Missing, invalid or already revoked session"),
        (status = 403, description = "Called with an API key"),
    )
)]
pub async fn logout(authenticated: AuthUser, pool: Data<PgPool>) -> Result<HttpResponse> {
    let session_id = authenticated.require_session()?;
    sessions::delete(pool.get_ref(), session_id).await?;
    Ok(HttpResponse::NoContent().finish())
}
