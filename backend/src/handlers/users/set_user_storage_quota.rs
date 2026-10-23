use actix_web::{
    HttpResponse,
    web::{Data, Json, Path},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::users;

#[derive(Deserialize, ToSchema)]
pub struct SetUserStorageQuotaRequest {
    /// Quota in bytes; null follows the global default, 0 is unlimited.
    #[schema(minimum = 0)]
    pub storage_quota_bytes: Option<i64>,
}

#[utoipa::path(
    tag = "Users",
    put,
    path = "/api/users/{id}/storage-quota",
    description = "Override a user's storage quota, or clear the override to follow the global \
        default. Existing files are kept when the user ends up over quota; only new uploads are refused.",
    params(("id" = String, Path, description = "User id")),
    request_body = SetUserStorageQuotaRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Quota saved"),
        (status = 400, description = "Negative quota"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "No such user"),
    )
)]
pub async fn set_user_storage_quota(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
    body: Json<SetUserStorageQuotaRequest>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    if body.storage_quota_bytes.is_some_and(|quota| quota < 0) {
        return Err(Error::BadRequest("storage_quota_bytes must be >= 0".into()));
    }
    if !users::set_storage_quota(pool.get_ref(), *id, body.storage_quota_bytes).await? {
        return Err(Error::NotFound);
    }
    Ok(HttpResponse::NoContent().finish())
}
