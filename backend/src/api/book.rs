use crate::api::{ApiError, ApiResult};
use crate::bible_data::MultiBibleData;
use crate::book_data::Book;
use crate::usj::{TranslatedBookInfo, UsjContent};
use crate::utils::ordered_enum::EnumOrderMap;
use actix_web::{HttpResponse, get, web};
use serde::Serialize;
use std::num::NonZeroU8;
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
    let Some(usj) = bible.usj(book) else {
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
        chapters: Vec<ChapterInfo>,
    }
    impl From<&UsjContent> for BookInfo {
        fn from(value: &UsjContent) -> Self {
            let root = value.unwrap_root();
            Self {
                translated_book_info: root.translated_book_info().as_owned(),
                chapters: root.content.iter().filter_map(Into::into).collect(),
            }
        }
    }

    #[derive(Serialize)]
    struct ChapterInfo {
        number: NonZeroU8,
        #[serde(skip_serializing_if = "Option::is_none")]
        alt_number: Option<NonZeroU8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub_number: Option<String>,
    }
    impl From<&UsjContent> for Option<ChapterInfo> {
        fn from(value: &UsjContent) -> Self {
            let UsjContent::Chapter {
                number,
                alt_number,
                pub_number,
                ..
            } = value
            else {
                return None;
            };
            Some(ChapterInfo {
                number: *number,
                alt_number: *alt_number,
                pub_number: pub_number.clone(),
            })
        }
    }

    let bible = bibles.get_or_api_error(bible.into_inner())?;
    Ok(HttpResponse::Ok().json(Response {
        books: Book::VARIANTS
            .iter()
            .filter_map(|&x| bible.usj(x))
            .map(|x| (*x.key(), x.value().into()))
            .collect(),
        book_order: bible.config.read().book_order,
    }))
}
