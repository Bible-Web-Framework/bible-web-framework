use crate::bible_data::BibleData;
use crate::book_data::Book;
use crate::index::{BibleIndex, BibleIndexLock};
use crate::reference::{BibleReference, ParseReferenceError, parse_references};
use crate::usj::{UsjContent, UsjRoot};
use charabia::Tokenize;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::time::Instant;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchResponse {
    pub response_type: SearchResponseType,
    pub search_term: String,
    pub total_results: usize,
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
        reference: BibleReference,
        translated_book_name: Option<String>,
        content: Option<Vec<UsjContent>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        highlights: Option<HashMap<String, Vec<Range<usize>>>>,
    },
    InvalidReference {
        invalid_reference: String,
        details: ParseReferenceError,
    },
}

pub fn search_bible(
    term: String,
    search_start: usize,
    search_max_count: usize,
    bible: &BibleData,
    index: &BibleIndexLock,
) -> SearchResponse {
    let references = parse_references(&term, &bible.book_parse_options());
    if references
        .iter()
        .all(|r| matches!(r, Err(e) if e.is_syntax()))
    {
        let start_time = Instant::now();
        let (total_results, results) = search_for_terms(
            &term,
            search_start,
            search_max_count,
            &bible.files,
            &index.read().unwrap(),
        );
        tracing::debug!(
            "Search for \"{term}\" (max {search_max_count} results) took {:?}",
            start_time.elapsed()
        );
        SearchResponse {
            response_type: SearchResponseType::SearchResults,
            search_term: term,
            total_results,
            references: results,
        }
    } else {
        SearchResponse {
            response_type: SearchResponseType::ScripturePassages,
            search_term: term,
            total_results: references.len(),
            references: references
                .into_iter()
                .map(|x| match x {
                    Ok(reference) => {
                        let usj = bible
                            .files
                            .get(&reference.book)
                            .map(UsjContent::unwrap_root);
                        SearchResponseResult::ReferenceContent {
                            reference,
                            translated_book_name: get_translated_book_name(usj),
                            content: usj.and_then(|usj| {
                                usj.find_reference(reference.chapter, reference.verses)
                            }),
                            highlights: None,
                        }
                    }
                    Err(error) => SearchResponseResult::InvalidReference {
                        invalid_reference: error.to_string(),
                        details: error,
                    },
                })
                .collect(),
        }
    }
}

fn get_translated_book_name(usj: Option<&UsjRoot>) -> Option<String> {
    usj.and_then(|x| x.translated_book_name())
        .map(str::to_string)
}

fn search_for_terms(
    terms: &str,
    start: usize,
    max_count: usize,
    usjs: &HashMap<Book, UsjContent>,
    index: &BibleIndex,
) -> (usize, Vec<SearchResponseResult>) {
    let mut result: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut reference_counts: HashMap<_, u32> = HashMap::new();

    let mut counted_terms = 0u32;
    let mut counted_references = HashSet::new();
    for term in terms.tokenize() {
        let Some((single_result, _)) = index.find(term.lemma()) else {
            continue;
        };
        counted_terms += 1;
        for (book, references) in single_result {
            counted_references.clear();
            for (reference, text_location) in references {
                let reference = BibleReference {
                    book: *book,
                    reference: *reference,
                };
                result
                    .entry(reference)
                    .or_default()
                    .push(text_location.clone());
                counted_references.insert(reference);
            }
            for reference in &counted_references {
                *reference_counts.entry(*reference).or_default() += 1;
            }
        }
    }

    result.retain(|reference, _| reference_counts[reference] == counted_terms);
    (
        result.len(),
        result
            .into_iter()
            .skip(start)
            .take(max_count)
            .map(|(reference, locations)| {
                let mut highlights: HashMap<_, Vec<_>> = HashMap::new();
                let usj = usjs.get(&reference.book);
                if let Some(usj) = usj {
                    for location in locations {
                        if let Some(text) = location.resolve_text_section(usj) {
                            highlights
                                .entry(text.to_string())
                                .or_default()
                                .push(location.char_range);
                        }
                    }
                }
                let usj = usj.map(UsjContent::unwrap_root);
                SearchResponseResult::ReferenceContent {
                    reference,
                    translated_book_name: get_translated_book_name(usj),
                    content: usj
                        .and_then(|usj| usj.find_reference(reference.chapter, reference.verses)),
                    highlights: Some(highlights),
                }
            })
            .collect(),
    )
}
