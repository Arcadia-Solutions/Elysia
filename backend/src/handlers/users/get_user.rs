use actix_web::{
    HttpResponse,
    web::{Data, Path},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::users::{self, UserSummary};

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/users/{id}",
    description = "One user's profile. Admin only.",
    params(("id" = String, Path, description = "User id")),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The user", body = UserSummary),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "No such user"),
    )
)]
pub async fn get_user(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let user = users::find_summary(pool.get_ref(), *id)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(HttpResponse::Ok().json(user))
}
