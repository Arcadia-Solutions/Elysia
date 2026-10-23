use actix_web::{
    HttpResponse,
    web::{Data, Json},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::services::files;
use crate::storage::Storage;

#[derive(Deserialize, ToSchema)]
pub struct ImageIdsRequest {
    /// Ids of the images to act on.
    pub ids: Vec<String>,
}

#[utoipa::path(
    tag = "Images",
    post,
    path = "/api/images/trash",
    description = "Move images to the trash: they stop being served to other sites at once \
        (Elysia's own pages still show them to admins) and are wiped from disk 30 days later. Admins may trash any image, uploaders their own during \
        the hour after upload. Unknown or already trashed ids are skipped.",
    request_body = ImageIdsRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Images trashed"),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin, and one of the images is someone else's or over an hour old"),
    )
)]
pub async fn trash_images(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    storage: Data<Storage>,
    body: Json<ImageIdsRequest>,
) -> Result<HttpResponse> {
    files::trash(
        pool.get_ref(),
        storage.get_ref(),
        authenticated.user_id,
        authenticated.is_admin,
        &body.ids,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
