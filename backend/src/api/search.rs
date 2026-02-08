use crate::api::{ApiError, ApiResult};
use crate::bible_data::MultiBibleData;
use crate::book_data::Book;
use crate::search::{SearchResponse, search_bible};
use actix_web::{HttpResponse, get, web};
use actix_web_validator::Query;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use unicase::UniCase;
use validator::Validate;

#[get("/book/{book}")]
pub async fn book(
    bible_and_book: web::Path<(String, String)>,
    bibles: web::Data<MultiBibleData>,
) -> ApiResult<HttpResponse> {
    let (bible, book) = bible_and_book.into_inner();
    let bible = bibles.get_or_api_error(bible)?;
    let Some(book) = Book::parse(&book, &bible.book_parse_options()) else {
        return Err(ApiError::InvalidBook(book));
    };
    let Some(usj) = bible.files.get(&book) else {
        return Err(ApiError::MissingUsj(book));
    };
    Ok(HttpResponse::Ok().json(&*usj))
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
    bibles: web::Data<MultiBibleData>,
) -> ApiResult<web::Json<SearchResponse>> {
    let query = query.into_inner();
    let bible = bibles.get_or_api_error(bible.into_inner())?;
    Ok(web::Json(search_bible(
        query.term,
        query.start.unwrap_or(0),
        query.count.unwrap_or(50),
        &bible,
    )))
}

#[get("/index")]
pub async fn index_route(
    bible: web::Path<String>,
    bibles: web::Data<MultiBibleData>,
) -> ApiResult<HttpResponse> {
    #[derive(Serialize)]
    struct Response<'a> {
        #[serde(with = "tuple_vec_map")]
        words: Vec<(&'a str, usize)>,
    }

    let bible = bibles.get_or_api_error(bible.into_inner())?;
    let index = bible.index.read();
    Ok(HttpResponse::Ok().json(Response {
        words: index
            .iter_names_and_counts()
            .sorted_by_cached_key(|(name, _)| UniCase::new(*name))
            .collect(),
    }))
}
