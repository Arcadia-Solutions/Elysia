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
pub struct SetUserAdminRequest {
    pub is_admin: bool,
}

#[utoipa::path(
    tag = "Users",
    put,
    path = "/api/users/{id}/admin",
    description = "Grant or revoke admin rights, local or SSO. Takes effect on the user's next \
        request. Admins cannot change their own rights, so at least one admin always remains.",
    params(("id" = String, Path, description = "User id")),
    request_body = SetUserAdminRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Admin rights saved"),
        (status = 400, description = "Tried to change their own rights"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
        (status = 404, description = "No such user"),
    )
)]
pub async fn set_user_admin(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<Uuid>,
    body: Json<SetUserAdminRequest>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    if *id == authenticated.user_id {
        return Err(Error::BadRequest(
            "you cannot change your own admin rights".into(),
        ));
    }
    if !users::set_admin(pool.get_ref(), *id, body.is_admin).await? {
        return Err(Error::NotFound);
    }
    Ok(HttpResponse::NoContent().finish())
}
