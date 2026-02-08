use crate::api::ApiResult;
use crate::bible_data::MultiBibleData;
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
                    },
                )
            })
            .collect(),
    }))
}
