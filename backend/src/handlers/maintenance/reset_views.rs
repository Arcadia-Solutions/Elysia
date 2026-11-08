use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::files::{self, ViewKind};

#[derive(Deserialize, ToSchema)]
pub struct ResetViewsRequest {
    /// Image whose views are reset; omitted resets every image.
    pub id: Option<String>,
    /// Counter to zero, or `all` of them.
    pub kind: ViewKind,
    /// `Referer` hosts whose views are reset; empty resets every referrer,
    /// `""` matches views without a `Referer`.
    #[serde(default)]
    pub referrers: Vec<String>,
}

#[utoipa::path(
    tag = "Maintenance",
    post,
    path = "/api/maintenance/views/reset",
    description = "Zero one view counter, or all of them, of an image or of every image for the given \
        referrers, the totals losing what they counted. Unknown ids are ignored.",
    request_body = ResetViewsRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Counters reset"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn reset_views(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    body: Json<ResetViewsRequest>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    files::reset_views(
        pool.get_ref(),
        body.id.as_deref(),
        body.kind,
        &body.referrers,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
