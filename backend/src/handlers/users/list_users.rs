use actix_web::{
    HttpResponse,
    web::{Data, Query},
};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::Result;
use crate::middlewares::AuthUser;
use crate::repository::users::{self, USERS_PER_PAGE, UserSummary, UsersQuery};

/// A page of users in the requested order.
#[derive(Serialize, ToSchema)]
pub struct UsersPage {
    pub users: Vec<UserSummary>,
    pub per_page: i64,
    /// Users matching the search, across all pages.
    pub total: i64,
}

#[utoipa::path(
    tag = "elysia",
    get,
    path = "/api/users",
    description = "Users, local and SSO, optionally searched by username or email. Admin only.",
    params(UsersQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "A page of matching users", body = UsersPage),
        (status = 401, description = "Missing or invalid session"),
        (status = 403, description = "Not an admin"),
    )
)]
pub async fn list_users(
    authenticated: AuthUser,
    pool: Data<PgPool>,
    query: Query<UsersQuery>,
) -> Result<HttpResponse> {
    authenticated.require_admin()?;
    let (users, total) = tokio::try_join!(
        users::list(pool.get_ref(), &query),
        users::count_users(pool.get_ref(), query.search()),
    )?;
    Ok(HttpResponse::Ok().json(UsersPage {
        users,
        per_page: USERS_PER_PAGE,
        total,
    }))
}
