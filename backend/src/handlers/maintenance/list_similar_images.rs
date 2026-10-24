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
use crate::services::similar_images;

/// Similar images page query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SimilarImagesQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    /// Groups per page.
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
}

/// Images that look alike, largest first.
#[derive(Serialize, ToSchema)]
pub struct SimilarImagesGroup {
    pub images: Vec<ImageSummary>,
}

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/similar-images",
    description = "A page of the last scan's groups of similar images, only those still holding \
        two available images. `total_items` counts groups.",
    params(SimilarImagesQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of similar images groups", body = PaginatedResults<SimilarImagesGroup>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn list_similar_images(
    authenticated: AuthUser,
    config: Data<Config>,
    pool: Data<PgPool>,
    query: Query<SimilarImagesQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let (groups, total) =
        similar_images::list_similar(pool.get_ref(), query.page, query.page_size).await?;
    let public_url = &config.server.public_url;
    Ok(HttpResponse::Ok().json(PaginatedResults::new(
        groups
            .into_iter()
            .map(|rows| SimilarImagesGroup {
                images: rows
                    .into_iter()
                    .map(|row| ImageSummary::from_row(row, public_url))
                    .collect(),
            })
            .collect(),
        query.page,
        query.page_size,
        total,
    )))
}
