use actix_web::{
    HttpResponse,
    web::{Data, Json, Path},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::reports;

#[derive(Deserialize, ToSchema)]
pub struct ReportImageRequest {
    /// Why the image should be looked at, 1 to 500 characters.
    pub reason: String,
}

#[utoipa::path(
    tag = "Images",
    post,
    path = "/api/i/{id}/report",
    description = "Flag an image for the admins' moderation queue. Reporting the same image again \
        adds another report.",
    params(("id" = String, Path, description = "File id")),
    request_body = ReportImageRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Image reported"),
        (status = 400, description = "Empty or too long reason"),
        (status = 401, description = "Missing or invalid session"),
        (status = 404, description = "No such available image"),
    )
)]
pub async fn report_image(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    id: Path<String>,
    body: Json<ReportImageRequest>,
) -> Result<HttpResponse> {
    let reason = body.reason.trim();
    if !(1..=500).contains(&reason.chars().count()) {
        return Err(Error::BadRequest(
            "the reason must be 1 to 500 characters long".into(),
        ));
    }
    if !reports::create(pool.get_ref(), &id, authenticated.user_id, reason).await? {
        return Err(Error::NotFound);
    }
    Ok(HttpResponse::NoContent().finish())
}
