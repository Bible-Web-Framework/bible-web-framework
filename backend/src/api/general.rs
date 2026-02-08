use crate::api::ApiResult;
use crate::bible_data::MultiBibleData;
use actix_web::{HttpResponse, get, web};
use serde::Serialize;
use std::collections::BTreeMap;

#[get("/bibles")]
pub async fn bibles(bibles: web::Data<MultiBibleData>) -> ApiResult<HttpResponse> {
    #[derive(Serialize)]
    struct Response {
        bibles: BTreeMap<String, BibleResponseData>,
    }

    #[derive(Serialize)]
    struct BibleResponseData {}

    Ok(HttpResponse::Ok().json(Response {
        bibles: bibles
            .bibles
            .iter()
            .map(|bible| (bible.key().to_string(), BibleResponseData {}))
            .collect(),
    }))
}
