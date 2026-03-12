use crate::api::ApiResult;
use crate::bible_data::MultiBibleData;
use crate::search::{SearchResponse, search_bible};
use actix_web::{HttpResponse, get, web};
use actix_web_validator::Query;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use unicase::UniCase;
use validator::Validate;

#[serde_as]
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "kebab-case")]
pub struct SearchQueryParams {
    term: String,
    #[serde(default)]
    start: usize,
    #[validate(range(max = 250))]
    #[serde(default = "default_result_count")]
    count: usize,
    #[serde(default = "default_generated_footnotes")]
    generate_footnotes: bool,
}

fn default_result_count() -> usize {
    50
}

fn default_generated_footnotes() -> bool {
    true
}

#[get("/search")]
pub async fn search(
    bible: web::Path<String>,
    query: Query<SearchQueryParams>,
    bibles: web::Data<MultiBibleData>,
) -> ApiResult<web::Json<SearchResponse>> {
    let query = query.into_inner();
    let bible = bibles.get_or_api_error(bible.into_inner())?;
    let results = search_bible(
        query.term,
        query.start,
        query.count,
        query.generate_footnotes,
        &bible,
    );
    Ok(web::Json(results))
}

#[get("/index")]
pub async fn index(
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
