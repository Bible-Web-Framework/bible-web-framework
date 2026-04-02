mod book;
mod info;
mod search;
mod short_url;

use crate::api::short_url::ShortUrlValue;
use crate::book_data::Book;
use crate::reference::ParseReferenceError;
use crate::reference_encoding::ReferenceEncodingError;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, ResponseError, Scope, web};
use actix_web_validator::QueryConfig;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Unknown bible translation '{0}'")]
    UnknownBible(String),
    #[error("Invalid book '{0}'")]
    InvalidBook(String),
    #[error("No USJ found for {0:?}")]
    MissingUsj(Book),
    #[error("Invalid reference {0}: {1}")]
    InvalidReference(String, #[source] ParseReferenceError),
    #[error("Invalid reference: {0}")]
    InvalidReferenceEncoding(#[from] ReferenceEncodingError),
    #[error("Short URL not found: {0}")]
    MissingShortReference(ShortUrlValue),

    #[error(transparent)]
    InvalidQueryParams(#[from] actix_web_validator::Error),
    #[error("Route not found: {0}")]
    RouteNotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Database serialization error: {0}")]
    Oxicode(#[from] oxicode::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::UnknownBible(_) => StatusCode::NOT_FOUND,
            ApiError::InvalidBook(_) => StatusCode::BAD_REQUEST,
            ApiError::MissingUsj(_) => StatusCode::NOT_FOUND,
            ApiError::InvalidReference(_, _) => StatusCode::BAD_REQUEST,
            ApiError::InvalidReferenceEncoding(_) => StatusCode::NOT_FOUND,
            ApiError::MissingShortReference(_) => StatusCode::NOT_FOUND,

            ApiError::InvalidQueryParams(_) => StatusCode::BAD_REQUEST,
            ApiError::RouteNotFound(_) => StatusCode::NOT_FOUND,

            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Oxicode(_) => StatusCode::INTERNAL_SERVER_ERROR,
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

pub fn scope() -> Scope {
    web::scope("/v1")
        .app_data(QueryConfig::default().error_handler(|e, _| ApiError::from(e).into()))
        .service(info::all_bibles)
        .service(
            web::scope("/short")
                .service(short_url::create)
                .service(short_url::resolve),
        )
        .service(
            web::scope("/bible/{bible}")
                .service(info::bible_info)
                .service(book::book)
                .service(book::books)
                .service(search::search)
                .service(search::index),
        )
}
