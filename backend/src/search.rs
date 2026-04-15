use crate::bible_data::BibleData;
use crate::bible_data::config::{FootnotesConfig, FootnotesTree};
use crate::bible_data::expanded::ExpandedBibleData;
use crate::book_data::Book;
use crate::index::{BibleIndex, TextRange};
use crate::reference::{BibleReference, ParseReferenceError, parse_references};
use crate::usj::content::{AttributesMap, ParaContent};
use crate::usj::marker::ContentMarker;
use crate::usj::root::UsjRoot;
use crate::usj::{TranslatedBookInfo, content::UsjContent, is_title_marker};
use crate::utils::ToOwnedStatic;
use crate::verse_range::VerseRange;
use charabia::{SeparatorKind, Tokenize, Tokenizer};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::num::NonZeroU8;
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
#[allow(clippy::large_enum_variant)]
pub enum SearchResponseResult {
    ReferenceContent {
        reference: BibleReference,
        translated_book_info: Option<TranslatedBookInfo<'static>>,
        previous_chapter: Option<ChapterReference>,
        next_chapter: Option<ChapterReference>,
        content: Option<Vec<UsjContent>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        highlights: Option<Vec<TextRange>>,
    },
    InvalidReference {
        invalid_reference: String,
        source_reference: String,
        #[serde(flatten)]
        details: ParseReferenceError,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChapterReference {
    pub book: Book,
    pub translated_book_info: TranslatedBookInfo<'static>,
    pub chapter: NonZeroU8,
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
        .all(|r| matches!(r, Err((e, _)) if e.is_syntax()))
    {
        let start_time = Instant::now();
        let (total_results, results) = search_for_terms(
            &term,
            search_start,
            search_max_count,
            bible,
            todo!("Implement index"),
        );
        tracing::debug!(
            "Search for \"{term}\" (max {search_max_count} results) took {:?}",
            start_time.elapsed(),
        );
        SearchResponse {
            response_type: SearchResponseType::SearchResults,
            search_term: term,
            total_results,
            references: results,
        }
    } else {
        let mut references = references
            .into_iter()
            .map(|x| match x {
                Ok(reference) => {
                    let book_data = bible.book(reference.book);
                    SearchResponseResult::ReferenceContent {
                        reference,
                        translated_book_info: book_data
                            .as_ref()
                            .map(|d| d.translated_book_info().to_owned_static()),
                        previous_chapter: get_nearby_book(
                            bible,
                            reference.book,
                            reference.chapter,
                            NearbyDir::Previous,
                        ),
                        next_chapter: get_nearby_book(
                            bible,
                            reference.book,
                            reference.chapter,
                            NearbyDir::Next,
                        ),
                        content: book_data
                            .and_then(|data| {
                                data.find_reference(reference.chapter, reference.verses)
                            })
                            .map(|(_, c)| c),
                        highlights: None,
                    }
                }
                Err((error, source)) => SearchResponseResult::InvalidReference {
                    invalid_reference: error.to_string(),
                    source_reference: source,
                    details: error,
                },
            })
            .collect_vec();
        if generate_footnotes {
            let config = bible.config();
            for reference in &mut references {
                if let SearchResponseResult::ReferenceContent {
                    reference,
                    content: Some(content),
                    ..
                } = reference
                {
                    FootnoteGenerator::new(
                        *reference,
                        config.search.create_tokenizer(),
                        &config.footnotes,
                    )
                    .generate_footnotes(content);
                }
            }
        }
        SearchResponse {
            response_type: SearchResponseType::ScripturePassages,
            search_term: term,
            total_results: references.len(),
            references,
        }
    }
}

fn search_for_terms(
    terms: &str,
    start: usize,
    max_count: usize,
    bible: &BibleData,
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
                let reference = BibleReference::new(*book, *reference);
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
                let mut highlights = vec![];
                let book_data = bible.book(reference.book);
                let content = if let Some(book_data) = &book_data {
                    let content = book_data.find_reference(reference.chapter, reference.verses);
                    if let Some((offset, _)) = &content {
                        for mut location in locations {
                            location.start -= *offset;
                            location.end -= *offset;
                            highlights.push(location);
                        }
                    }
                    content
                } else {
                    None
                };
                SearchResponseResult::ReferenceContent {
                    reference,
                    translated_book_info: book_data
                        .map(|d| d.translated_book_info().to_owned_static()),
                    previous_chapter: get_nearby_book(
                        bible,
                        reference.book,
                        reference.chapter,
                        NearbyDir::Previous,
                    ),
                    next_chapter: get_nearby_book(
                        bible,
                        reference.book,
                        reference.chapter,
                        NearbyDir::Next,
                    ),
                    content: content.map(|(_, c)| c),
                    highlights: Some(highlights),
                }
            })
            .collect(),
    )
}

fn get_nearby_book(
    bible: &BibleData,
    mut current_book: Book,
    current_chapter: NonZeroU8,
    nearby_dir: NearbyDir,
) -> Option<ChapterReference> {
    let mut current_chapter_count = current_book.chapter_count()?.get();
    let mut current_chapter = current_chapter.get();
    let mut current_book_data = bible.book(current_book);
    let book_order = bible.config().book_order;
    loop {
        match nearby_dir {
            NearbyDir::Previous => {
                if current_chapter > 1 {
                    current_chapter -= 1;
                } else if let Some(pred) = book_order.predecessor(current_book) {
                    current_book = pred;
                    current_chapter_count = pred.chapter_count().map_or(1, NonZeroU8::get);
                    current_chapter = current_chapter_count;
                    current_book_data = bible.book(current_book);
                } else {
                    return None;
                }
            }
            NearbyDir::Next => {
                if current_chapter < current_chapter_count {
                    current_chapter += 1;
                } else if let Some(succ) = book_order.successor(current_book) {
                    current_book = succ;
                    current_chapter_count = succ.chapter_count().map_or(1, NonZeroU8::get);
                    current_chapter = 1;
                    current_book_data = bible.book(current_book);
                } else {
                    return None;
                }
            }
        }
        let Some(current_book_data) = &current_book_data else {
            current_chapter = 1;
            current_chapter_count = 1;
            continue;
        };
        let chapter = NonZeroU8::new(current_chapter).unwrap();
        if current_book_data.has_chapter(chapter) {
            return Some(ChapterReference {
                book: current_book,
                translated_book_info: current_book_data.translated_book_info().to_owned_static(),
                chapter,
            });
        }
    }
}

enum NearbyDir {
    Previous,
    Next,
}

struct FootnoteGenerator<'a> {
    tokenizer: Tokenizer<'a>,
    phrase_finder: PhraseFinder<'a>,
    current_book: Book,
    current_chapter: Option<(NonZeroU8, String)>,
    current_verses: Option<(VerseRange, String)>,
    current_path: SmallVec<[usize; 4]>,
    last_text_path: Option<SmallVec<[usize; 4]>>,
    insert_at_paths: Vec<(SmallVec<[usize; 4]>, &'a UsjContent)>,
    found_footnotes: HashSet<(Vec<String>, &'a UsjContent)>,
}

impl<'a> FootnoteGenerator<'a> {
    fn new(
        initial_reference: BibleReference,
        tokenizer: Tokenizer<'a>,
        footnotes: &'a FootnotesTree,
    ) -> Self {
        Self {
            tokenizer,
            phrase_finder: PhraseFinder::new(footnotes, initial_reference),
            current_book: initial_reference.book,
            current_chapter: Some((
                initial_reference.chapter,
                initial_reference.chapter.to_string(),
            )),
            current_verses: Some((
                initial_reference.verses,
                initial_reference.chapter.to_string(),
            )),
            current_path: smallvec![],
            last_text_path: None,
            insert_at_paths: vec![],
            found_footnotes: HashSet::new(),
        }
    }
}

impl FootnoteGenerator<'_> {
    fn generate_footnotes(mut self, usjs: &mut [UsjContent]) {
        self.current_path.push(0);
        for usj in usjs.iter_mut() {
            self.generate_footnotes_recursive(usj);
            *self.current_path.last_mut().unwrap() += 1;
        }
        self.current_path.pop();

        for (path, footnote) in self.insert_at_paths.into_iter().rev() {
            let mut current = &mut usjs[path[0]];
            for &index in path.iter().skip(1).take(path.len() - 2) {
                current = current.get_content_mut(index).unwrap().left().unwrap();
            }
            current.insert_usj_content(*path.last().unwrap(), footnote.clone());
        }
    }

    fn generate_footnotes_recursive(&mut self, usj: &mut UsjContent) {
        let is_para = matches!(usj, UsjContent::Paragraph { .. });
        match usj {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. } => {
                self.current_path.push(0);
                for child in content {
                    self.generate_footnotes_recursive(child);
                    *self.current_path.last_mut().unwrap() += 1;
                }
                self.current_path.pop();
            }

            UsjContent::Paragraph {
                marker, content, ..
            }
            | UsjContent::Character {
                marker, content, ..
            }
            | UsjContent::TableCell {
                marker, content, ..
            } => {
                if is_para {
                    self.end_section();
                }
                if !is_title_marker(*marker) {
                    self.current_path.push(0);
                    let mut i = 0;
                    while i < content.len() {
                        let child = &mut content[i];
                        let new_children = match child {
                            ParaContent::Usj(usj) => {
                                self.generate_footnotes_recursive(usj);
                                None
                            }
                            ParaContent::Plain(text) => self.expand_text_section(text),
                        };
                        if let Some(to_add) = new_children {
                            let new_elements = to_add.len();
                            content.splice(i..=i, to_add);
                            i += new_elements;
                        } else {
                            i += 1;
                        }
                        *self.current_path.last_mut().unwrap() = i - 1;
                        self.last_text_path = Some(self.current_path.clone());
                        *self.current_path.last_mut().unwrap() += 1;
                    }
                    self.current_path.pop();
                }
            }

            UsjContent::Chapter {
                number, pub_number, ..
            } => {
                self.current_chapter = Some((
                    number.value,
                    pub_number.clone().unwrap_or_else(|| number.string.clone()),
                ));
                self.current_verses = None;
                self.end_section();
            }
            UsjContent::Verse {
                number, pub_number, ..
            } => {
                self.current_verses = Some((
                    number.value,
                    pub_number.clone().unwrap_or_else(|| number.string.clone()),
                ));
                self.end_section();
            }

            _ => {}
        }
    }

    fn end_section(&mut self) {
        let footnote = if let Some((chapter, _)) = self.current_chapter
            && let Some((verses, _)) = self.current_verses
        {
            self.phrase_finder.reset_to_location(BibleReference {
                book: self.current_book,
                chapter,
                verses,
            })
        } else {
            self.phrase_finder.attempt_finish()
        };
        if let Some((phrase, footnote)) = footnote
            && self.found_footnotes.insert((phrase, &footnote.footnote))
        {
            self.insert_at_paths
                .push((self.last_text_path.take().unwrap(), &footnote.footnote));
        }
    }

    fn expand_text_section(&mut self, text: &str) -> Option<Vec<ParaContent>> {
        let (Some((_, chapter)), Some((_, verses))) = (&self.current_chapter, &self.current_verses)
        else {
            return None;
        };
        let mut result: Option<Vec<ParaContent>> = None;
        let mut text_index = 0;
        for (token_index, token) in self.tokenizer.tokenize(text).enumerate() {
            let footnote = if token.separator_kind() == Some(SeparatorKind::Hard) {
                self.phrase_finder.attempt_finish()
            } else {
                if token.is_separator() {
                    continue;
                }
                self.phrase_finder.push(token.lemma.into_owned())
            };
            let Some((phrase, footnote)) = footnote else {
                continue;
            };
            if !self.found_footnotes.insert((phrase, &footnote.footnote)) {
                continue;
            }
            if token_index == 0 {
                self.insert_at_paths
                    .push((self.last_text_path.take().unwrap(), &footnote.footnote));
                continue;
            }
            let result = result.get_or_insert_default();
            if token.byte_end > text_index {
                let substr = &text[text_index..token.byte_start];
                if text_index > 0 {
                    result.push(ParaContent::Plain(format!(" {substr}")));
                } else {
                    result.push(ParaContent::Plain(substr.to_string()));
                }
                text_index = token.byte_start;
            }
            let mut note = footnote.footnote.clone();
            match &mut note {
                UsjContent::Note { content, .. } => {
                    content.insert(
                        0,
                        ParaContent::Usj(UsjContent::Character {
                            marker: ContentMarker::Fr(()),
                            content: vec![ParaContent::Plain(format!("{chapter}:{verses} "))],
                            attributes: AttributesMap::new(),
                        }),
                    );
                }
                _ => unreachable!("FootnoteUsfmAsUsj should have returned UsjContent::Note"),
            }
            result.push(ParaContent::Usj(note));
        }
        if let Some(result) = &mut result
            && text_index < text.len()
        {
            result.push(ParaContent::Plain(format!(" {}", &text[text_index..])));
        }
        result
    }
}

struct PhraseFinder<'a> {
    location: BibleReference,
    tree_stack: Vec<&'a FootnotesTree>,
    current_phrase: VecDeque<String>,
}

impl<'a> PhraseFinder<'a> {
    fn new(tree: &'a FootnotesTree, initial_location: BibleReference) -> Self {
        Self {
            location: initial_location,
            tree_stack: vec![tree],
            current_phrase: VecDeque::new(),
        }
    }

    fn reset_to_location(
        &mut self,
        location: BibleReference,
    ) -> Option<(Vec<String>, &'a FootnotesConfig)> {
        let result = self
            .tree_stack
            .last()
            .unwrap()
            .value()
            .and_then(|x| x.get(&self.location));
        let phrase = self.reset();
        self.location = location;
        result.map(|f| (phrase, f))
    }

    fn push(&mut self, mut lemma: String) -> Option<(Vec<String>, &'a FootnotesConfig)> {
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
        let phrase = self.reset();
        self.continue_phrase(lemma);
        result.map(|f| (phrase, f))
    }

    fn attempt_finish(&mut self) -> Option<(Vec<String>, &'a FootnotesConfig)> {
        let result = self
            .tree_stack
            .last()
            .unwrap()
            .value()
            .and_then(|x| x.get(&self.location));
        let phrase = self.reset();
        result.map(|f| (phrase, f))
    }

    fn reset(&mut self) -> Vec<String> {
        self.tree_stack.truncate(1);
        self.current_phrase.drain(..).collect()
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
    use crate::bible_data::config::{FootnotesConfig, FootnotesTree};
    use crate::reference::BibleReference;
    use crate::reference_value;
    use crate::search::PhraseFinder;
    use crate::usj::content::ParaContent;
    use crate::usj::content::UsjContent;
    use crate::usj::marker::ContentMarker;
    use charabia::{Language, Token, TokenizerBuilder};
    use itertools::Itertools;
    use multiset::HashMultiSet;
    use pretty_assertions::assert_eq;
    use rangemap::RangeInclusiveMap;
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
                marker: ContentMarker::P(()),
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

    fn tokens() -> vec::IntoIter<String> {
        let mut builder = TokenizerBuilder::<Vec<u8>>::new();
        builder.allow_list(&[Language::Lat]);
        builder
            .into_tokenizer()
            .tokenize(LOREM_IPSUM)
            .filter(Token::is_word)
            .map(|x| x.lemma.into_owned())
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
            (phrase, footnote): (Vec<String>, &FootnotesConfig),
            expected_footnote: &str,
            expected_phrase: &[&str],
        ) {
            assert_eq!(footnote, &footnote_value(expected_footnote));
            assert_eq!(phrase.as_slice(), expected_phrase);
            assert!(
                remaining_footnotes.remove(footnote),
                "{footnote:?} found more than expected",
            );
        }

        let mut finder = PhraseFinder::new(&footnotes_config, BibleReference::default());
        finder.reset_to_location(BibleReference::default());
        for (index, token) in tokens().enumerate() {
            if let Some(footnote) = finder.push(token) {
                match index {
                    11 => assert_footnote(
                        &mut remaining_footnotes,
                        footnote,
                        FOOTNOTE_ALPHA,
                        &["eiusmod"],
                    ),
                    14 | 31 => {
                        assert_footnote(&mut remaining_footnotes, footnote, FOOTNOTE_ECHO, &["ut"])
                    }
                    21 => assert_footnote(
                        &mut remaining_footnotes,
                        footnote,
                        FOOTNOTE_FOXTROT,
                        &["ut", "enim"],
                    ),
                    24 => assert_footnote(
                        &mut remaining_footnotes,
                        footnote,
                        FOOTNOTE_BRAVO,
                        &["minim", "veniam"],
                    ),
                    25 => assert_footnote(
                        &mut remaining_footnotes,
                        footnote,
                        FOOTNOTE_CHARLIE,
                        &["quis"],
                    ),
                    27 => assert_footnote(
                        &mut remaining_footnotes,
                        footnote,
                        FOOTNOTE_HOTEL,
                        &["nostrud", "exercitation"],
                    ),
                    _ => panic!("Found unknown at index {index}: {footnote:?}"),
                }
            }
        }
        if let Some(footnote) = finder.attempt_finish() {
            assert_footnote(
                &mut remaining_footnotes,
                footnote,
                FOOTNOTE_DELTA,
                &["laborum"],
            );
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

        let mut finder = PhraseFinder::new(&footnotes_config, BibleReference::default());

        // "one" should exist, "two" should not
        finder.reset_to_location(REF_ALPHA);
        assert_eq!(finder.push("one".into()), None);
        assert_eq!(
            finder.push("two".into()),
            Some((vec!["one".into()], &footnote_value(FOOTNOTE_ALPHA))),
        );
        assert_eq!(finder.attempt_finish(), None);

        // "two" should exist, "one" should not
        finder.reset_to_location(REF_BETA);
        assert_eq!(finder.push("one".into()), None);
        assert_eq!(finder.push("two".into()), None);
        assert_eq!(
            finder.attempt_finish(),
            Some((vec!["two".into()], &footnote_value(FOOTNOTE_BRAVO))),
        );

        // "three" and "three four" should both exist
        finder.reset_to_location(REF_GAMMA);
        assert_eq!(finder.push("three".into()), None);
        assert_eq!(
            finder.push("three".into()),
            Some((vec!["three".into()], &footnote_value(FOOTNOTE_CHARLIE))),
        );
        assert_eq!(finder.push("four".into()), None);
        assert_eq!(
            finder.attempt_finish(),
            Some((
                vec!["three".into(), "four".into()],
                &footnote_value(FOOTNOTE_DELTA),
            )),
        );

        // "three" should exist, but "three four" should still shadow and prevent "three" from
        // showing when followed by "four"
        finder.reset_to_location(REF_DELTA);
        assert_eq!(finder.push("three".into()), None);
        assert_eq!(
            finder.push("three".into()),
            Some((vec!["three".into()], &footnote_value(FOOTNOTE_CHARLIE))),
        );
        assert_eq!(finder.push("four".into()), None);
        assert_eq!(finder.attempt_finish(), None);
    }
}
