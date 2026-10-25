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
use crate::repository::reports::{self, Report};
use crate::services::paginate;

/// Moderation queue page query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportsQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    /// Reported images per page.
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
}

/// A reported image with every report on it, oldest first.
#[derive(Serialize, ToSchema)]
pub struct ReportedImage {
    pub image: ImageSummary,
    pub reports: Vec<Report>,
}

#[utoipa::path(
    tag = "Reports",
    get,
    path = "/api/reports",
    description = "A page of the moderation queue: reported available images, the most reported \
        first. Trashing an image takes it out of the queue, restoring it brings its reports back. \
        `total_items` counts images.",
    params(ReportsQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of reported images", body = PaginatedResults<ReportedImage>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn list_reports(
    authenticated: AuthUser,
    config: Data<Config>,
    pool: Data<PgPool>,
    query: Query<ReportsQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let (rows, total) = paginate(
        query.page,
        query.page_size,
        |offset, limit| reports::queue_page(pool.get_ref(), offset, limit),
        reports::queue_count(pool.get_ref()),
        |(row, _)| row.total,
    )
    .await?;
    let mut reported_images: Vec<ReportedImage> = Vec::new();
    for (row, report) in rows {
        match reported_images.last_mut() {
            Some(reported) if reported.image.id == row.id => reported.reports.push(report),
            _ => reported_images.push(ReportedImage {
                image: ImageSummary::from_row(row, &config.server.public_url),
                reports: vec![report],
            }),
        }
    }
    Ok(HttpResponse::Ok().json(PaginatedResults::new(
        reported_images,
        query.page,
        query.page_size,
        total,
    )))
}
