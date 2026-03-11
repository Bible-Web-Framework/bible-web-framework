use crate::api::{ApiError, ApiResult};
use crate::bible_data::MultiBibleData;
use crate::book_data::Book;
use actix_web::{HttpResponse, get, web};

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

// #[get("/books")]
// pub async fn books(
//     bible: web::Path<String>,
//     bibles: web::Data<MultiBibleData>,
// ) -> ApiResult<HttpResponse> {
//     #[derive(Serialize)]
//     struct Response<'a> {
//         #[serde(with = "tuple_vec_map")]
//         words: Vec<(&'a str, usize)>,
//     }
//
//     let bible = bibles.get_or_api_error(bible.into_inner())?;
// }
