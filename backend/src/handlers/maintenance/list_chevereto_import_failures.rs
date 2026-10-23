use std::num::NonZeroU32;

use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::IntoParams;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::PaginatedResults;
use crate::services::chevereto_import::{self, CheveretoImportFailure};

/// Failed Chevereto images page query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CheveretoImportFailuresQuery {
    /// 1-based page number.
    #[param(value_type = i64, minimum = 1)]
    pub page: NonZeroU32,
    #[serde(deserialize_with = "crate::repository::page_size")]
    #[param(minimum = 1, maximum = 500)]
    pub page_size: u32,
}

#[utoipa::path(
    tag = "Maintenance",
    get,
    path = "/api/maintenance/chevereto-import/failures",
    description = "A page of the Chevereto images that could not be imported, by image id. \
        Start the import with `retry_failed` to try them again.",
    params(CheveretoImportFailuresQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of failed images", body = PaginatedResults<CheveretoImportFailure>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn list_chevereto_import_failures(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    query: Query<CheveretoImportFailuresQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let (failures, total) =
        chevereto_import::list_failures(pool.get_ref(), query.page, query.page_size).await?;
    Ok(HttpResponse::Ok().json(PaginatedResults::new(
        failures,
        query.page,
        query.page_size,
        total,
    )))
}
