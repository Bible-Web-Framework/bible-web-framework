use crate::api::{ApiError, ApiResult};
use crate::bible_data::expanded::MultiExpandedBibleData;
use crate::bible_data::{BookData, ChapterInfo, DynMultiBibleData, MultiBibleData};
use crate::book_data::Book;
use crate::usj::TranslatedBookInfo;
use crate::utils::ToOwnedStatic;
use crate::utils::ordered_enum::EnumOrderMap;
use actix_web::{HttpResponse, get, web};
use serde::Serialize;
use strum::VariantArray;

#[get("/book/{book}")]
pub async fn book_usj(
    bible_and_book: web::Path<(String, String)>,
    bibles: web::Data<DynMultiBibleData>,
) -> ApiResult<HttpResponse> {
    let (bible, book_usj) = bible_and_book.into_inner();
    let bible = bibles.get_or_api_error(bible)?;
    let Some(book_usj) = Book::parse(&book_usj, &bible.book_parse_options()) else {
        return Err(ApiError::InvalidBook(book_usj));
    };
    let Some(book_usj) = bible.book(book_usj) else {
        return Err(ApiError::MissingUsj(book_usj));
    };
    Ok(HttpResponse::Ok().json(book_usj.to_usj()))
}

#[get("/books")]
pub async fn books(
    bible: web::Path<String>,
    bibles: web::Data<DynMultiBibleData>,
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
        chapters: Vec<ChapterInfo<'static>>,
    }
    impl From<BookData<'_>> for BookInfo {
        fn from(value: BookData<'_>) -> Self {
            Self {
                translated_book_info: value.translated_book_info().to_owned_static(),
                chapters: value
                    .chapter_infos()
                    .into_iter()
                    .map(|x| x.to_owned_static())
                    .collect(),
            }
        }
    }

    let bible = bibles.get_or_api_error(bible.into_inner())?;
    Ok(HttpResponse::Ok().json(Response {
        books: Book::VARIANTS
            .iter()
            .filter_map(|&x| Some((x, bible.book(x)?.into())))
            .collect(),
        book_order: bible.config().book_order,
    }))
}
