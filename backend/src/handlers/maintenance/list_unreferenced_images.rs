use std::num::NonZeroU32;

use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use crate::config::Config;
use crate::error::Result;
use crate::handlers::images::list_images::ImageSummary;
use crate::middlewares::AuthUser;
use crate::repository::PaginatedResults;
use crate::repository::files::{ImagesSortBy, SortOrder};
use crate::services::maintenance;

/// Unreferenced images page query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UnreferencedImagesQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
    pub sort_by: ImagesSortBy,
    pub order: SortOrder,
}

/// A page of the unreferenced images list.
#[derive(Serialize, ToSchema)]
pub struct UnreferencedImagesPage {
    #[serde(flatten)]
    pub page: PaginatedResults<ImageSummary>,
    /// When the scan started, RFC 3339, UTC. None when it found nothing.
    pub scanned_at: Option<String>,
    /// `server.public_url` without its scheme, the host links use by default.
    /// Empty when it is not set and links are relative to the frontend.
    pub public_host: String,
}

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/unreferenced-images",
    description = "A page of the last scan's unreferenced images that are still available.",
    params(UnreferencedImagesQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of unreferenced images", body = UnreferencedImagesPage),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn list_unreferenced_images(
    authenticated: AuthUser,
    config: Data<Config>,
    pool: Data<PgPool>,
    query: Query<UnreferencedImagesQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let (rows, total, scanned_at) = maintenance::list_unreferenced(
        pool.get_ref(),
        query.page,
        query.page_size,
        query.sort_by,
        query.order,
    )
    .await?;
    let public_url = &config.server.public_url;
    Ok(HttpResponse::Ok().json(UnreferencedImagesPage {
        page: PaginatedResults::new(
            rows.into_iter()
                .map(|row| ImageSummary::from_row(row, public_url))
                .collect(),
            query.page,
            query.page_size,
            total,
        ),
        scanned_at,
        public_host: public_url
            .split_once("://")
            .map_or(public_url.as_str(), |(_, rest)| rest)
            .to_owned(),
    }))
}
