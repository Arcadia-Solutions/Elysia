use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use sqlx::PgPool;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::PaginatedResults;
use crate::repository::users::{self, UserSummary, UsersQuery};
use crate::settings::SharedElysiaSettings;

#[utoipa::path(
    tag = "Users",
    get,
    path = "/api/users",
    description = "Users, local and SSO, optionally searched by username or email. Admin only.",
    params(UsersQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of matching users", body = PaginatedResults<UserSummary>),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn list_users(
    authenticated: AuthUser,
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
    query: Query<UsersQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let default_quota_bytes = settings
        .read()
        .expect("settings lock poisoned")
        .default_storage_quota_bytes;
    let (users, total) = tokio::try_join!(
        users::list(pool.get_ref(), &query, default_quota_bytes),
        users::count_users(pool.get_ref(), query.search()),
    )?;
    Ok(HttpResponse::Ok().json(PaginatedResults::new(
        users,
        query.page,
        query.page_size,
        total,
    )))
}
