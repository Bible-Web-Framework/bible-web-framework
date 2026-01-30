use crate::api::{ApiError, ApiResult};
use crate::book_data::Book;
use crate::config_new::MultiBibleData;
use crate::index::BibleIndexLock;
use crate::search::{SearchResponse, search_bible};
use actix_web::{HttpResponse, get, web};
use actix_web_validator::Query;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use unicase::UniCase;
use validator::Validate;

#[get("/book/{book}")]
pub async fn book(
    bible: web::Path<String>,
    book: web::Path<String>,
    config: web::Data<MultiBibleData>,
) -> ApiResult<HttpResponse> {
    let bible = config.get_or_api_error(bible.into_inner())?;
    let book = book.into_inner();
    let Some(book) = Book::parse(&book, &bible.book_parse_options()) else {
        return Err(ApiError::InvalidBook(book));
    };
    let Some(usj) = bible.files.get(&book) else {
        return Err(ApiError::MissingUsj(book));
    };
    Ok(HttpResponse::Ok().json(usj))
}

#[derive(Debug, Deserialize, Validate)]
pub struct SearchQueryParams {
    term: String,
    start: Option<usize>,
    #[validate(range(max = 250))]
    count: Option<usize>,
}

#[get("/search")]
pub async fn search(
    bible: web::Path<String>,
    query: Query<SearchQueryParams>,
    config: web::Data<MultiBibleData>,
    index: web::Data<BibleIndexLock>,
) -> ApiResult<web::Json<SearchResponse>> {
    let query = query.into_inner();
    Ok(web::Json(search_bible(
        query.term,
        query.start.unwrap_or(0),
        query.count.unwrap_or(50),
        &*config.get_or_api_error(bible.into_inner())?,
        &index,
    )))
}

#[get("/index")]
pub async fn index_route(index: web::Data<BibleIndexLock>) -> ApiResult<HttpResponse> {
    #[derive(Serialize)]
    struct Response<'a> {
        words: WordsResponse<'a>,
    }

    #[derive(Serialize)]
    struct WordsResponse<'a>(#[serde(with = "tuple_vec_map")] Vec<(&'a str, usize)>);

    let index = index.read().unwrap();
    let mut result = index.iter_names_and_counts().collect_vec();
    result.sort_by_cached_key(|(name, _)| UniCase::new(*name));
    Ok(HttpResponse::Ok().json(Response {
        words: WordsResponse(result),
    }))
}
