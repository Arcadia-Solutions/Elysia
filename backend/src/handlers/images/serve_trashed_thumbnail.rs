use actix_web::{
    HttpRequest, HttpResponse,
    web::{Data, Path},
};
use utoipa::ToSchema;

use super::{file_response, parse_filename};
use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::storage::Storage;

/// Raw image bytes, so the generated client types the response as a file.
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct ImageBytes(pub Vec<u8>);

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/images/trash/{filename}",
    description = "Thumbnail of a trashed image, for the browse page trash view. Falls back \
        to the trashed image itself when it has no thumbnail, like `/t/`.",
    params(("filename" = String, Path, description = "<id>.<ext>")),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The thumbnail bytes", content_type = "application/octet-stream", body = ImageBytes),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Authenticated but not an admin"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn serve_trashed_thumbnail(
    req: HttpRequest,
    authenticated: AuthUser,
    storage: Data<Storage>,
    filename: Path<String>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let (id, ext) = parse_filename(&filename)?;
    let file = storage
        .open_trashed_thumbnail(id, ext)
        .await
        .map_err(|_| Error::NotFound)?;
    Ok(file_response(&req, file, "private, no-store"))
}
