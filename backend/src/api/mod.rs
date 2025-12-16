pub mod search;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, ResponseError};
use serde_json::json;
use thiserror::Error;
use crate::book_data::Book;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Invalid book '{0}'")]
    InvalidBook(String),
    #[error("No USJ found for {0:?}")]
    MissingUsj(Book),

    #[error("Missing 'term' query param")]
    MissingTermParam,

    #[error("Route not found: {0}")]
    RouteNotFound(String),
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + 'static>),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::InvalidBook(_) => StatusCode::BAD_REQUEST,
            ApiError::MissingUsj(_) => StatusCode::NOT_FOUND,

            ApiError::MissingTermParam => StatusCode::BAD_REQUEST,

            ApiError::RouteNotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Other(e) => e.status_code(),
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        HttpResponse::build(status).json(json!({
            "status": status.as_u16(),
            "status_message": status.canonical_reason(),
            "message": self.to_string(),
        }))
    }
}

pub async fn route_not_found(req: HttpRequest) -> ApiResult<()> {
    Err(ApiError::RouteNotFound(req.path().to_string()))
}
