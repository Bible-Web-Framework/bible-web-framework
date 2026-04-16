use crate::api::ApiResult;
use crate::bible_data::config::TextDirection;
use crate::bible_data::{BibleData, DynMultiBibleData};
use crate::book_data::Book;
use actix_web::{get, web};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct BiblesResponse {
    pub default_bible: String,
    pub bibles: BTreeMap<String, BibleInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BibleInfo {
    pub display_name: Option<String>,
    pub text_direction: TextDirection,
    pub simple_book_names: BTreeMap<Book, String>,
}

#[get("/bibles")]
pub async fn all_bibles(
    bibles: web::Data<DynMultiBibleData>,
) -> ApiResult<web::Json<BiblesResponse>> {
    Ok(web::Json(BiblesResponse {
        default_bible: bibles.default_bible().to_string(),
        bibles: bibles
            .bibles()
            .into_iter()
            .filter_map(|id| {
                let bible = bibles.get_bible(&id)?;
                Some((id, bible.into()))
            })
            .collect(),
    }))
}

#[get("/info")]
pub async fn bible_info(
    bible: web::Path<String>,
    bibles: web::Data<DynMultiBibleData>,
) -> ApiResult<web::Json<BibleInfo>> {
    let bible = bibles.get_or_api_error(bible.into_inner())?;
    Ok(web::Json(bible.into()))
}

impl From<BibleData<'_>> for BibleInfo {
    fn from(value: BibleData<'_>) -> Self {
        let config = value.config();
        Self {
            display_name: config.display_name.clone(),
            text_direction: config.text_direction,
            simple_book_names: value
                .books()
                .into_iter()
                .filter_map(|book| {
                    let data = value.book(book)?;
                    Some((
                        book,
                        data.translated_book_info().short_name(book).to_string(),
                    ))
                })
                .collect(),
        }
    }
}
