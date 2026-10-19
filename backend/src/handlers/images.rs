use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::files::{ImagesQuery, ImagesStatus};
use crate::services::files::{self, IMAGES_PER_PAGE};

/// One image in a browse page.
#[derive(Serialize, ToSchema)]
pub struct ImageSummary {
    pub id: String,
    /// Thumbnail URL: from `/t/`, or for a trashed image from the admin-only
    /// trash, fetched with the session token.
    pub thumbnail_url: String,
}

/// A page of stored images in the requested order.
#[derive(Serialize, ToSchema)]
pub struct ImagesPage {
    pub images: Vec<ImageSummary>,
    pub per_page: i64,
    pub total: i64,
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/images",
    description = "Admins browse all images, other users only their own uploads.",
    params(ImagesQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of stored images", body = ImagesPage),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin, and asked for trashed images"),
    )
)]
pub async fn images(
    authenticated: AuthUser,
    config: Data<Config>,
    pool: Data<PgPool>,
    query: Query<ImagesQuery>,
) -> Result<HttpResponse> {
    // The trash is admin-only.
    let uploader = match (authenticated.is_admin, query.status) {
        (true, _) => None,
        (false, ImagesStatus::Available) => Some(authenticated.user_id),
        (false, ImagesStatus::Trashed) => return Err(Error::Forbidden),
    };
    let (rows, total) = files::list_images(pool.get_ref(), &query, uploader).await?;
    let public_url = &config.server.public_url;
    let images = rows
        .into_iter()
        .map(|row| ImageSummary {
            thumbnail_url: match query.status {
                ImagesStatus::Available => {
                    files::thumbnail_url(public_url, &row.id, &row.ext, row.has_thumbnail)
                }
                ImagesStatus::Trashed => {
                    files::trashed_thumbnail_url(public_url, &row.id, &row.ext)
                }
            },
            id: row.id,
        })
        .collect();
    Ok(HttpResponse::Ok().json(ImagesPage {
        images,
        per_page: IMAGES_PER_PAGE,
        total,
    }))
}
