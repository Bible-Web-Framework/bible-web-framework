use crate::api::ApiResult;
use crate::bible_data::MultiBibleData;
use crate::book_data::Book;
use actix_web::{get, web};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct BiblesResponse {
    pub default_bible: String,
    pub bibles: BTreeMap<String, BiblesResponseData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BiblesResponseData {
    pub display_name: Option<String>,
    pub simple_book_names: BTreeMap<Book, String>,
}

#[get("/bibles")]
pub async fn bibles(bibles: web::Data<MultiBibleData>) -> ApiResult<web::Json<BiblesResponse>> {
    Ok(web::Json(BiblesResponse {
        default_bible: bibles.default_bible.clone(),
        bibles: bibles
            .bibles
            .iter()
            .map(|bible| {
                (
                    bible.key().to_string(),
                    BiblesResponseData {
                        display_name: bible.value().config.read().display_name.clone(),
                        simple_book_names: bible
                            .value()
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
                    },
                )
            })
            .collect(),
    }))
}
