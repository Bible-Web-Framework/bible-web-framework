use crate::api::{ApiError, ApiResult};
use crate::bible_data::MultiBibleData;
use crate::book_data::Book;
use crate::usj::TranslatedBookInfo;
use crate::utils::ordered_enum::EnumOrderMap;
use actix_web::{HttpResponse, get, web};
use serde::Serialize;
use strum::VariantArray;

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

#[get("/books")]
pub async fn books(
    bible: web::Path<String>,
    bibles: web::Data<MultiBibleData>,
) -> ApiResult<HttpResponse> {
    #[derive(Serialize)]
    struct Response {
        #[serde(with = "tuple_vec_map")]
        books: Vec<(Book, BookInfo)>,
        book_order: EnumOrderMap<Book>,
    }

    #[derive(Serialize)]
    struct BookInfo {
        translated_book_info: TranslatedBookInfo<'static>,
    }

    let bible = bibles.get_or_api_error(bible.into_inner())?;
    Ok(HttpResponse::Ok().json(Response {
        books: Book::VARIANTS
            .iter()
            .filter_map(|x| bible.files.get(x))
            .map(|x| {
                (
                    *x.key(),
                    BookInfo {
                        translated_book_info: x
                            .value()
                            .unwrap_root()
                            .translated_book_info()
                            .as_owned(),
                    },
                )
            })
            .collect(),
        book_order: bible.config.read().book_order,
    }))
}
