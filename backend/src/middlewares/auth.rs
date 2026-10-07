use std::future::{Ready, ready};

use actix_web::{FromRequest, HttpRequest, dev::Payload, web::Data};

use crate::config::Config;
use crate::error::{Error, Result};

/// Extractor that requires `Authorization: Bearer <admin_token>`.
///
/// Placeholder auth until SSO lands: a handler that takes an `AdminAuth`
/// argument is gated behind the single admin token from `config.yml`.
pub struct AdminAuth;

impl FromRequest for AdminAuth {
    type Error = Error;
    type Future = Ready<Result<Self>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let expected = &req
            .app_data::<Data<Config>>()
            .expect("Config not configured")
            .auth
            .admin_token;
        let ok = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .is_some_and(|t| t == expected);
        ready(if ok {
            Ok(AdminAuth)
        } else {
            Err(Error::Unauthorized)
        })
    }
}
