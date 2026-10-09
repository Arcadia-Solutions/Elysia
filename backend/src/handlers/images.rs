use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use crate::error::Result;
use crate::middlewares::AdminAuth;
use crate::services::files::{self, IMAGES_PER_PAGE};

/// One image in a browse page.
#[derive(Serialize, ToSchema)]
pub struct ImageSummary {
    pub id: String,
    /// Thumbnail URL, served from `/t/`.
    pub thumbnail_url: String,
}

/// A page of stored images, newest first.
#[derive(Serialize, ToSchema)]
pub struct ImagesPage {
    pub images: Vec<ImageSummary>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImagesQuery {
    /// 1-based page number; absent or below 1 means the first page.
    #[param(minimum = 1)]
    pub page: Option<i64>,
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/images",
    params(ImagesQuery),
    responses(
        (status = 200, description = "A page of stored images", body = ImagesPage),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn images(
    pool: Data<PgPool>,
    query: Query<ImagesQuery>,
    _auth: AdminAuth,
) -> Result<HttpResponse> {
    let (rows, total, page) = files::list_images(pool.get_ref(), query.page.unwrap_or(1)).await?;
    let images = rows
        .into_iter()
        .map(|row| ImageSummary {
            thumbnail_url: files::thumbnail_url(&row.id, &row.ext, row.has_thumbnail),
            id: row.id,
        })
        .collect();
    Ok(HttpResponse::Ok().json(ImagesPage {
        images,
        page,
        per_page: IMAGES_PER_PAGE,
        total,
    }))
}
