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
pub struct SetUserBannedRequest {
    pub banned: bool,
}

#[utoipa::path(
    tag = "elysia",
    put,
    path = "/api/users/{id}/banned",
    description = "Ban or unban a user, local or SSO. A banned user's sessions, logins and \
        delegated uploads are refused at once. Admins cannot ban themselves.",
    params(("id" = String, Path, description = "User id")),
    request_body = SetUserBannedRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Ban status saved"),
        (status = 400, description = "Tried to ban themselves"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "No such user"),
    )
)]
pub async fn set_user_banned(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
    body: Json<SetUserBannedRequest>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    if *id == authenticated.user_id {
        return Err(Error::BadRequest("you cannot ban yourself".into()));
    }
    if !users::set_banned(pool.get_ref(), *id, body.banned).await? {
        return Err(Error::NotFound);
    }
    Ok(HttpResponse::NoContent().finish())
}
