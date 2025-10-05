use crate::config::BibleConfig;
use crate::reference::{ChapterReference, ParseReferenceError, parse_references};
use crate::usj::UsjContent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchResponse {
    pub response_type: SearchResponseType,
    pub search_term: String,
    pub references: Vec<SearchResponseResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResponseType {
    SearchResults,
    ScripturePassages,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SearchResponseResult {
    ReferenceContent {
        reference: String,
        #[serde(flatten)]
        reference_details: ChapterReference,
        content: Vec<UsjContent>,
    },
    InvalidReference {
        invalid_reference: String,
        details: ParseReferenceError,
    },
}

pub fn search_bible(term: String, config: &BibleConfig) -> SearchResponse {
    let references = parse_references(&term, Some(&config.additional_aliases));
    if references.iter().all(Result::is_err) {
        SearchResponse {
            response_type: SearchResponseType::SearchResults,
            search_term: term,
            references: vec![], // TODO: Keyword search
        }
    } else {
        SearchResponse {
            response_type: SearchResponseType::ScripturePassages,
            search_term: term,
            references: references
                .into_iter()
                .map(|x| match x {
                    Ok(reference) => SearchResponseResult::ReferenceContent {
                        reference: reference.to_string(),
                        reference_details: reference,
                        content: vec![], // TODO: Bible content search
                    },
                    Err(error) => SearchResponseResult::InvalidReference {
                        invalid_reference: error.to_string(),
                        details: error,
                    },
                })
                .collect(),
        }
    }
}
