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
use crate::repository::files::{ImageListRow, ImagesQuery, ImagesStatus};
use crate::repository::{PaginatedResults, users};
use crate::services::files;

/// One image in a browse or unreferenced images page.
#[derive(Serialize, ToSchema)]
pub struct ImageSummary {
    pub id: String,
    /// Thumbnail URL from `/t/`. Trashed, it is only served to Elysia's own pages.
    pub thumbnail_url: String,
    pub original_name: Option<String>,
    /// Stored extension.
    pub ext: String,
    /// Bytes on disk.
    pub size: i64,
    pub width: i32,
    pub height: i32,
    pub views: i64,
    pub uploader_username: String,
    /// RFC 3339, UTC.
    pub created_at: String,
}

impl ImageSummary {
    /// Build from a listed row, with thumbnail URLs under `public_url`.
    pub fn from_row(row: ImageListRow, public_url: &str) -> Self {
        Self {
            thumbnail_url: files::thumbnail_url(public_url, &row.id, &row.ext, row.has_thumbnail),
            id: row.id,
            original_name: row.original_name,
            ext: row.ext,
            size: row.size,
            width: row.width,
            height: row.height,
            views: row.views,
            uploader_username: row.uploader_username,
            created_at: row.created_at,
        }
    }
}

#[utoipa::path(
    tag = "Images",
    get,
    path = "/api/images",
    description = "Admins browse all images, or one user's with `username`, other users only their own uploads.",
    params(ImagesQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of stored images", body = PaginatedResults<ImageSummary>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin, and asked for trashed images or filtered by username"),
    )
)]
pub async fn images(
    authenticated: AuthUser,
    config: Data<Config>,
    pool: Data<PgPool>,
    query: Query<ImagesQuery>,
) -> Result<HttpResponse> {
    // The trash and the username filter are admin-only.
    let uploader = match (authenticated.is_admin, query.status, &query.username) {
        (true, _, None) => None,
        (true, _, Some(username)) => {
            match users::find_id_by_username(pool.get_ref(), username).await? {
                Some(id) => Some(id),
                // No such user: nothing uploaded.
                None => {
                    return Ok(
                        HttpResponse::Ok().json(PaginatedResults::<ImageSummary>::new(
                            Vec::new(),
                            query.page,
                            query.page_size,
                            0,
                        )),
                    );
                }
            }
        }
        (false, ImagesStatus::Available, None) => Some(authenticated.user_id),
        (false, _, _) => return Err(Error::Forbidden),
    };
    let (rows, total) = files::list_images(pool.get_ref(), &query, uploader).await?;
    let images = rows
        .into_iter()
        .map(|row| ImageSummary::from_row(row, &config.server.public_url))
        .collect();
    Ok(HttpResponse::Ok().json(PaginatedResults::new(
        images,
        query.page,
        query.page_size,
        total,
    )))
}
