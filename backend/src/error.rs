use actix_web::{HttpResponse, ResponseError, http::StatusCode};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("conflict")]
    Conflict,

    #[error("not found")]
    NotFound,

    #[error("unsupported media type")]
    UnsupportedMediaType,

    #[error("{0}")]
    BadRequest(String),

    #[error("internal server error")]
    Internal,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

impl Error {
    /// Variant name, used as a metric attribute.
    fn kind(&self) -> &'static str {
        match self {
            Error::Unauthorized => "unauthorized",
            Error::Forbidden => "forbidden",
            Error::Conflict => "conflict",
            Error::NotFound => "not_found",
            Error::UnsupportedMediaType => "unsupported_media_type",
            Error::BadRequest(_) => "bad_request",
            Error::Internal => "internal",
            Error::Io(_) => "io",
            Error::Db(_) => "db",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::Forbidden => StatusCode::FORBIDDEN,
            Error::Conflict => StatusCode::CONFLICT,
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Error::Io(_) | Error::Db(_) | Error::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status_code = self.status_code();
        if status_code.is_server_error() {
            tracing::error!("request failed: {self}");
        }
        crate::telemetry::record_http_error(self.kind(), status_code.as_u16());
        HttpResponse::build(status_code).json(serde_json::json!({ "error": self.to_string() }))
    }
}
