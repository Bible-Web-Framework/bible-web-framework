use crate::book_data::Book;
use crate::config::{BibleConfigLock, BibleIndexLock};
use crate::search::search_bible;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, ResponseError, get, web};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use unicase::UniCase;

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

#[get("/book/{book}")]
pub async fn book(
    book: web::Path<String>,
    config: web::Data<BibleConfigLock>,
) -> ApiResult<HttpResponse> {
    let book = book.into_inner();
    let config = config.read().unwrap();
    let Some(book) = Book::parse(&book, Some(&config.additional_aliases)) else {
        return Err(ApiError::InvalidBook(book));
    };
    let Some(usj) = config.us.files.get(&book) else {
        return Err(ApiError::MissingUsj(book));
    };
    Ok(HttpResponse::Ok().json(usj))
}

#[derive(Debug, Deserialize)]
struct SearchQueryParams {
    term: Option<String>,
}

#[get("/search")]
pub async fn search(
    query: web::Query<SearchQueryParams>,
    config: web::Data<BibleConfigLock>,
    index: web::Data<BibleIndexLock>,
) -> ApiResult<HttpResponse> {
    let params = query.into_inner();
    let Some(term) = params.term else {
        return Err(ApiError::MissingTermParam);
    };
    Ok(HttpResponse::Ok().json(search_bible(term, &config.read().unwrap(), &index)))
}

#[get("/index")]
pub async fn index_route(index: web::Data<BibleIndexLock>) -> ApiResult<HttpResponse> {
    #[derive(Serialize)]
    struct Serialization<'a>(#[serde(with = "tuple_vec_map")] Vec<(&'a str, usize)>);

    let index = index.read().unwrap();
    let mut result = index.iter_names_and_counts().collect_vec();
    result.sort_by_cached_key(|(name, _)| UniCase::new(*name));
    Ok(HttpResponse::Ok().json(Serialization(result)))
}
