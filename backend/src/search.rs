use crate::bible_data::{BibleData, FootnotesConfig, FootnotesTree};
use crate::book_data::Book;
use crate::index::BibleIndex;
use crate::reference::{BibleReference, ParseReferenceError, parse_references};
use crate::usj::{UsjContent, UsjRoot};
use charabia::Tokenize;
use dashmap::DashMap;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    generate_footnotes: bool,
    bible: &BibleData,
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
            &bible.index.read(),
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
        let references = references
            .into_iter()
            .map(|x| match x {
                Ok(reference) => {
                    let usj = bible.files.get(&reference.book);
                    let usj = usj.as_deref().map(UsjContent::unwrap_root);
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
            .collect_vec();
        if generate_footnotes {}
        SearchResponse {
            response_type: SearchResponseType::ScripturePassages,
            search_term: term,
            total_results: references.len(),
            references,
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
    usjs: &DashMap<Book, UsjContent>,
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
                if let Some(usj) = &usj {
                    for location in locations {
                        if let Some(text) = location.resolve_text_section(usj) {
                            highlights
                                .entry(text.to_string())
                                .or_default()
                                .push(location.char_range);
                        }
                    }
                }
                let usj = usj.as_deref().map(UsjContent::unwrap_root);
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

// fn generate_footnotes_recursive(usj: &UsjContent, finder: &mut PhraseFinder) {}

struct PhraseFinder<'a> {
    location: BibleReference,
    tree_stack: Vec<&'a FootnotesTree>,
    current_phrase: VecDeque<String>,
}

impl<'a> PhraseFinder<'a> {
    fn new(tree: &'a FootnotesTree) -> Self {
        Self {
            location: BibleReference::default(),
            tree_stack: vec![tree],
            current_phrase: VecDeque::new(),
        }
    }
}

impl PhraseFinder<'_> {
    fn reset_to_location(&mut self, location: BibleReference) {
        self.location = location;
        self.reset();
    }

    fn push(&mut self, mut lemma: String) -> Option<&FootnotesConfig> {
        lemma = self.continue_phrase(lemma)?;
        let mut result = None;
        if let Some(value) = self.tree_stack.last().unwrap().value() {
            result = value.get(&self.location);
        } else {
            while self.current_phrase.pop_front().is_some() {
                if !self.attempt_reload_from_phrase() {
                    continue;
                }
                result = if let Some(map) = self.tree_stack.last().unwrap().value() {
                    map.get(&self.location)
                } else {
                    None
                };
                break;
            }
        }
        self.reset();
        self.continue_phrase(lemma);
        result
    }

    fn attempt_finish(&mut self) -> Option<&FootnotesConfig> {
        let result = self
            .tree_stack
            .last()
            .unwrap()
            .value()
            .and_then(|x| x.get(&self.location));
        self.tree_stack.truncate(1);
        self.current_phrase.clear();
        result
    }

    fn reset(&mut self) {
        self.tree_stack.truncate(1);
        self.current_phrase.clear();
    }

    fn continue_phrase(&mut self, lemma: String) -> Option<String> {
        if let Some(child) = self.tree_stack.last().unwrap().child(&lemma) {
            self.tree_stack.push(child);
            self.current_phrase.push_back(lemma);
            None
        } else {
            Some(lemma)
        }
    }

    fn attempt_reload_from_phrase(&mut self) -> bool {
        self.tree_stack.truncate(1);
        for lemma in &self.current_phrase {
            if let Some(child) = self.tree_stack.last().unwrap().child(lemma) {
                self.tree_stack.push(child);
            } else {
                self.tree_stack.truncate(1);
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod test {
    use crate::bible_data::{FootnotesConfig, FootnotesTree};
    use crate::reference::BibleReference;
    use crate::reference_value;
    use crate::search::PhraseFinder;
    use crate::usj::{ParaContent, UsjContent};
    use charabia::{Language, Token, TokenizerBuilder};
    use itertools::Itertools;
    use multiset::HashMultiSet;
    use pretty_assertions::assert_eq;
    use rangemap::RangeInclusiveMap;
    use std::borrow::Cow;
    use std::ops::RangeInclusive;
    use std::vec;

    const LOREM_IPSUM: &str = "\
        Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore \
        et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut \
        aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
        cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
        culpa qui officia deserunt mollit anim id est laborum.\
        ";
    const WHOLE_BIBLE: RangeInclusive<BibleReference> =
        reference_value!(Genesis 1:1)..=reference_value!(Revelation 22:21);

    const FOOTNOTE_ALPHA: &str = "Test footnote alpha";
    const FOOTNOTE_BRAVO: &str = "Test footnote bravo";
    const FOOTNOTE_CHARLIE: &str = "Test footnote charlie";
    const FOOTNOTE_DELTA: &str = "Test footnote delta";
    const FOOTNOTE_ECHO: &str = "Test footnote echo";
    const FOOTNOTE_FOXTROT: &str = "Test footnote foxtrot";
    const FOOTNOTE_GOLF: &str = "Test footnote golf";
    const FOOTNOTE_HOTEL: &str = "Test footnote hotel";

    fn key<const N: usize>(arr: [&str; N]) -> Vec<String> {
        arr.map(str::to_string).into()
    }

    fn footnote_value(x: &str) -> FootnotesConfig {
        FootnotesConfig {
            footnote: UsjContent::Paragraph {
                marker: "p".to_string(),
                content: vec![ParaContent::Plain(x.to_string())],
            },
        }
    }

    fn footnote(
        range: RangeInclusive<BibleReference>,
        x: &str,
    ) -> RangeInclusiveMap<BibleReference, FootnotesConfig> {
        RangeInclusiveMap::from([(range, footnote_value(x))])
    }

    fn tokens() -> vec::IntoIter<Cow<'static, str>> {
        let mut builder = TokenizerBuilder::<Vec<u8>>::new();
        builder.allow_list(&[Language::Lat]);
        builder
            .into_tokenizer()
            .tokenize(LOREM_IPSUM)
            .filter(Token::is_word)
            .map(|x| x.lemma)
            .collect_vec()
            .into_iter()
    }

    #[test]
    fn test_phrase_finder() {
        let footnotes_config = FootnotesTree::from([
            (key(["eiusmod"]), footnote(WHOLE_BIBLE, FOOTNOTE_ALPHA)),
            (
                key(["minim", "veniam"]),
                footnote(WHOLE_BIBLE, FOOTNOTE_BRAVO),
            ),
            (key(["quis"]), footnote(WHOLE_BIBLE, FOOTNOTE_CHARLIE)),
            (key(["laborum"]), footnote(WHOLE_BIBLE, FOOTNOTE_DELTA)),
            (key(["ut"]), footnote(WHOLE_BIBLE, FOOTNOTE_ECHO)),
            (key(["ut", "enim"]), footnote(WHOLE_BIBLE, FOOTNOTE_FOXTROT)),
            (
                key(["quid", "nostrud", "hominem"]),
                footnote(WHOLE_BIBLE, FOOTNOTE_GOLF),
            ), // Impossible
            (
                key(["nostrud", "exercitation"]),
                footnote(WHOLE_BIBLE, FOOTNOTE_HOTEL),
            ),
        ]);

        let mut remaining_footnotes = HashMultiSet::from_iter([
            footnote_value(FOOTNOTE_ALPHA),
            footnote_value(FOOTNOTE_ECHO),
            footnote_value(FOOTNOTE_ECHO),
            footnote_value(FOOTNOTE_FOXTROT),
            footnote_value(FOOTNOTE_BRAVO),
            footnote_value(FOOTNOTE_CHARLIE),
            footnote_value(FOOTNOTE_HOTEL),
            footnote_value(FOOTNOTE_DELTA),
        ]);
        fn assert_footnote(
            remaining_footnotes: &mut HashMultiSet<FootnotesConfig>,
            footnote: &FootnotesConfig,
            expected: &str,
        ) {
            assert_eq!(footnote, &footnote_value(expected));
            assert!(
                remaining_footnotes.remove(footnote),
                "{footnote:?} found more than expected",
            );
        }

        let mut finder = PhraseFinder::new(&footnotes_config);
        finder.reset_to_location(BibleReference::default());
        for (index, token) in tokens().enumerate() {
            if let Some(footnote) = finder.push(token.into_owned()) {
                match index {
                    11 => assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_ALPHA),
                    14 | 31 => assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_ECHO),
                    21 => assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_FOXTROT),
                    24 => assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_BRAVO),
                    25 => assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_CHARLIE),
                    27 => assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_HOTEL),
                    _ => panic!("Found unknown at index {index}: {footnote:?}"),
                }
            }
        }
        if let Some(footnote) = finder.attempt_finish() {
            assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_DELTA);
        }

        assert!(
            remaining_footnotes.is_empty(),
            "Missing footnotes:\n  - {}",
            remaining_footnotes
                .iter()
                .map(|x| format!("{x:?}"))
                .join("\n  - ")
        );
    }

    #[test]
    fn test_verse_ranges_and_shadow() {
        const REF_ALPHA: BibleReference = reference_value!(Genesis 1:1);
        const REF_BETA: BibleReference = reference_value!(Genesis 1:2);
        const REF_GAMMA: BibleReference = reference_value!(Genesis 1:3);
        const REF_DELTA: BibleReference = reference_value!(Genesis 1:4);
        let footnotes_config = FootnotesTree::from([
            (
                key(["one"]),
                footnote(REF_ALPHA..=REF_ALPHA, FOOTNOTE_ALPHA),
            ),
            (key(["two"]), footnote(REF_BETA..=REF_BETA, FOOTNOTE_BRAVO)),
            (
                key(["three"]),
                footnote(REF_GAMMA..=REF_DELTA, FOOTNOTE_CHARLIE),
            ),
            (
                key(["three", "four"]),
                footnote(REF_GAMMA..=REF_GAMMA, FOOTNOTE_DELTA),
            ),
        ]);

        let mut finder = PhraseFinder::new(&footnotes_config);

        // "one" should exist, "two" should not
        finder.reset_to_location(REF_ALPHA);
        assert_eq!(finder.push("one".into()), None);
        assert_eq!(
            finder.push("two".into()),
            Some(&footnote_value(FOOTNOTE_ALPHA)),
        );
        assert_eq!(finder.attempt_finish(), None);

        // "two" should exist, "one" should not
        finder.reset_to_location(REF_BETA);
        assert_eq!(finder.push("one".into()), None);
        assert_eq!(finder.push("two".into()), None);
        assert_eq!(
            finder.attempt_finish(),
            Some(&footnote_value(FOOTNOTE_BRAVO)),
        );

        // "three" and "three four" should both exist
        finder.reset_to_location(REF_GAMMA);
        assert_eq!(finder.push("three".into()), None);
        assert_eq!(
            finder.push("three".into()),
            Some(&footnote_value(FOOTNOTE_CHARLIE)),
        );
        assert_eq!(finder.push("four".into()), None);
        assert_eq!(
            finder.attempt_finish(),
            Some(&footnote_value(FOOTNOTE_DELTA)),
        );

        // "three" should exist, but "three four" should still shadow and prevent "three" from
        // showing when followed by "four"
        finder.reset_to_location(REF_DELTA);
        assert_eq!(finder.push("three".into()), None);
        assert_eq!(
            finder.push("three".into()),
            Some(&footnote_value(FOOTNOTE_CHARLIE)),
        );
        assert_eq!(finder.push("four".into()), None);
        assert_eq!(finder.attempt_finish(), None);
    }
}
