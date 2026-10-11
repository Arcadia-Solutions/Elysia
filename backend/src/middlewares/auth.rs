use std::future::Future;
use std::pin::Pin;

use actix_web::{FromRequest, HttpRequest, dev::Payload, web::Data};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::repository::users;
use crate::services::auth;

/// Authenticated caller: the bearer must be a JWT session token for a live,
/// unbanned user row.
pub struct AuthUser {
    pub user_id: Uuid,
    pub is_admin: bool,
}

impl AuthUser {
    /// Reject callers without admin rights. The settings endpoints use this.
    pub fn require_admin(&self) -> Result<()> {
        if self.is_admin {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
}

/// Pull the bearer token out of the request headers, if present.
fn bearer_token(request: &HttpRequest) -> Option<String> {
    request
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::to_string)
}

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self>>>>;

    fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let config = request
            .app_data::<Data<Config>>()
            .expect("Config not configured")
            .clone();
        let pool = request
            .app_data::<Data<PgPool>>()
            .expect("PgPool not configured")
            .clone();
        let token = bearer_token(request);
        Box::pin(async move {
            let token = token.ok_or(Error::Unauthorized)?;
            let claims = auth::decode_jwt(&token, &config.auth.jwt_secret)?;
            let user_id = claims.user_id()?;
            let row = users::find_by_id(pool.get_ref(), user_id)
                .await?
                .ok_or(Error::Unauthorized)?;
            if row.banned {
                return Err(Error::Unauthorized);
            }
            Ok(AuthUser {
                user_id: row.id,
                is_admin: row.is_admin,
            })
        })
    }
}
