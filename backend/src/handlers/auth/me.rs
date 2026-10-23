use actix_web::{HttpResponse, web::Data};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::error::{Error, Result};
use crate::middlewares::AuthUser;
use crate::repository::users::{self, AuthUserRow, StorageUsage};
use crate::settings::SharedElysiaSettings;

/// The caller's own account.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ProfileResponse {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub is_admin: bool,
    pub provider: String,
}

impl From<AuthUserRow> for ProfileResponse {
    fn from(row: AuthUserRow) -> Self {
        Self {
            id: row.id.to_string(),
            username: row.username,
            email: row.email,
            is_admin: row.is_admin,
            provider: row.provider.as_str().to_string(),
        }
    }
}

/// The caller's own account and storage usage.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct MeResponse {
    #[serde(flatten)]
    pub profile: ProfileResponse,
    /// Storage the caller's available and trashed files use, and their effective quota.
    pub storage: StorageUsage,
}

#[utoipa::path(
    tag = "Auth",
    get,
    path = "/api/auth/me",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Own account", body = MeResponse),
        (status = 401, description = "Missing, invalid or banned session"),
    )
)]
pub async fn me(
    authenticated: AuthUser,
    settings: SharedElysiaSettings,
    pool: Data<PgPool>,
) -> Result<HttpResponse> {
    let row = users::find_by_id(pool.get_ref(), authenticated.user_id)
        .await?
        .ok_or(Error::Unauthorized)?;
    if row.banned {
        return Err(Error::Unauthorized);
    }
    let default_quota_bytes = settings
        .read()
        .expect("settings lock poisoned")
        .default_storage_quota_bytes;
    let storage = users::storage_usage(pool.get_ref(), row.id, default_quota_bytes).await?;
    Ok(HttpResponse::Ok().json(MeResponse {
        profile: ProfileResponse::from(row),
        storage,
    }))
}
