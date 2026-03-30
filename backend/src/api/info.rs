use crate::api::ApiResult;
use crate::bible_data::{BibleData, MultiBibleData, TextDirection};
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
pub async fn all_bibles(bibles: web::Data<MultiBibleData>) -> ApiResult<web::Json<BiblesResponse>> {
    Ok(web::Json(BiblesResponse {
        default_bible: bibles.default_bible.clone(),
        bibles: bibles
            .bibles
            .iter()
            .map(|bible| (bible.key().to_string(), bible.value().into()))
            .collect(),
    }))
}

#[get("/info")]
pub async fn bible_info(
    bible: web::Path<String>,
    bibles: web::Data<MultiBibleData>,
) -> ApiResult<web::Json<BibleInfo>> {
    let bible = bibles.get_or_api_error(bible.into_inner())?;
    Ok(web::Json(bible.value().into()))
}

impl From<&BibleData> for BibleInfo {
    fn from(value: &BibleData) -> Self {
        let config = value.config.read();
        Self {
            display_name: config.display_name.clone(),
            text_direction: config.text_direction,
            simple_book_names: value
                .books
                .iter()
                .map(|book| {
                    (
                        *book.key(),
                        book.value()
                            .usj()
                            .unwrap_root()
                            .translated_book_info()
                            .short_name(*book.key())
                            .to_string(),
                    )
                })
                .collect(),
        }
    }
}
