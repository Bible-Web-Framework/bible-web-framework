use crate::api::{ApiError, ApiResult};
use crate::book_data::Book;
use crate::config::{BibleConfigLock, BibleIndexLock};
use crate::search::{SearchResponse, search_bible};
use actix_web::{HttpResponse, get, web};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use unicase::UniCase;

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
pub struct SearchQueryParams {
    term: String,
}

#[get("/search")]
pub async fn search(
    query: web::Query<SearchQueryParams>,
    config: web::Data<BibleConfigLock>,
    index: web::Data<BibleIndexLock>,
) -> ApiResult<web::Json<SearchResponse>> {
    Ok(web::Json(search_bible(
        query.into_inner().term,
        &config.read().unwrap(),
        &index,
    )))
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
