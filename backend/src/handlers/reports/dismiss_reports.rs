use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::handlers::images::trash_images::ImageIdsRequest;
use crate::middlewares::AuthUser;
use crate::repository::reports;

#[utoipa::path(
    tag = "Reports",
    post,
    path = "/api/reports/dismiss",
    description = "Delete every report on these images, keeping the images. Unreported ids are skipped.",
    request_body = ImageIdsRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Reports dismissed"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn dismiss_reports(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<ImageIdsRequest>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    reports::dismiss(pool.get_ref(), &body.ids).await?;
    Ok(HttpResponse::NoContent().finish())
}
