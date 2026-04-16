use crate::bible_data::expanded::ExpandedBookData;
use crate::book_data::Book;
use crate::reference::{BibleReference, BookReference};
use crate::usj::content::ParaContent;
use crate::usj::marker::ContentMarker;
use crate::usj::root::UsjRoot;
use crate::usj::{ParaIndex, content::UsjContent};
use crate::utils::print_memory_stats;
use crate::verse_range::VerseRange;
use charabia::Tokenizer;
use dashmap::DashMap;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rkyv::Archive;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, LinkedList, hash_map};
use std::iter::FusedIterator;
use std::num::NonZeroU8;
use std::ops::{Range, SubAssign};
use std::time::Instant;
use std::{mem, slice};
use string_interner::StringInterner;
use string_interner::backend::{Backend, StringBackend};
use string_interner::symbol::SymbolU32;
use tinyvec::{TinyVec, tiny_vec};
use unicode_normalization::UnicodeNormalization;

pub type BookSearchResult = (BookReference, TextRange);
pub type ArchivedBookSearchResult = <BookSearchResult as Archive>::Archived;
pub type SearchResultMap = HashMap<Book, Box<[BookSearchResult]>>;

type InternerSymbol = SymbolU32;
type InternerBackend = StringBackend<InternerSymbol>;
type Interner = StringInterner<InternerBackend>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReindexType {
    PartialReindex(SmallVec<[Book; 2]>),
    Unindex(Book),
    FullReindex,
}

pub struct ExpandedBibleIndex {
    pub log_marker: Option<String>,
    interner: Interner,
    references_and_names_by_word: HashMap<InternerSymbol, IndexedWord>,
    words_by_book: HashMap<Book, Box<[InternerSymbol]>>,
}

impl Default for ExpandedBibleIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Default, rkyv::Serialize, Archive)]
pub struct IndexedWord {
    pub total: usize,
    pub by_book: SearchResultMap,
    #[rkyv(with = rkyv::with::Niche)]
    pub name: Option<Box<str>>,
}

macro_rules! format_marker {
    ($self:ident) => {
        if let Some(marker) = $self.log_marker.as_ref() {
            format!(" from {marker}")
        } else {
            "".to_string()
        }
    };
}

impl ExpandedBibleIndex {
    pub fn new() -> Self {
        Self {
            log_marker: None,
            interner: Interner::new(),
            references_and_names_by_word: HashMap::new(),
            words_by_book: HashMap::default(),
        }
    }

    pub fn find_by_lemma<'a, 'b: 'a>(
        &'a self,
        lemma: &'b str,
    ) -> Option<(&'a str, ExpandedReferencesIter<'a>)> {
        let word = self
            .interner
            .get(lemma)
            .and_then(|s| self.references_and_names_by_word.get(&s))?;

        let name = word.name.as_deref().unwrap_or(lemma);

        let mut map_iter = word.by_book.iter();
        let (first_book, first_book_results) = map_iter.next().unwrap();
        let references = ExpandedReferencesIter {
            map_iter,
            book_iter: first_book_results.iter(),
            current_book: *first_book,
        };

        Some((name, references))
    }

    pub fn iter_names_and_counts(&self) -> ExpandedNamesAndCountsIter<'_> {
        ExpandedNamesAndCountsIter {
            iter: self.references_and_names_by_word.iter(),
            interner: &self.interner,
        }
    }

    pub fn iter_lemmas_and_ids(&self) -> <InternerBackend as Backend>::Iter<'_> {
        self.interner.iter()
    }

    pub fn word_from_symbol(&self, symbol: InternerSymbol) -> Option<&IndexedWord> {
        self.references_and_names_by_word.get(&symbol)
    }

    pub fn replace_from_indexer(&mut self, book: Book, indexer: BookIndexer) {
        let old_words = self
            .words_by_book
            .insert(
                book,
                indexer
                    .results
                    .keys()
                    .map(|word| self.interner.get_or_intern(word))
                    .collect(),
            )
            .unwrap_or_default();
        for word in old_words {
            if let Entry::Occupied(mut old_map_entry) =
                self.references_and_names_by_word.entry(word)
            {
                let old_map = old_map_entry.get_mut();
                old_map.total -= old_map
                    .by_book
                    .remove(&book)
                    .map(|x| x.len())
                    .unwrap_or_default();
                if old_map.total == 0 {
                    old_map_entry.remove();
                }
            }
        }
        for (word, (new_name, new_references)) in indexer.results {
            let references = self
                .references_and_names_by_word
                .entry(self.interner.get_or_intern(word))
                .or_default();
            if references.name.is_none() {
                references.name = new_name;
            }
            references.total += new_references.len();
            references
                .by_book
                .insert(book, new_references.into_boxed_slice());
        }
    }

    pub fn reindex_usj(&mut self, book: Book, usj: &UsjContent, tokenizer: &Tokenizer) {
        let start = Instant::now();
        let mut indexer = BookIndexer::new();
        indexer.index_usj(usj, tokenizer);
        let words = indexer.indexed_words();
        self.replace_from_indexer(book, indexer);
        tracing::info!(
            "Reindexed {book}{} ({words} words) in {:?}",
            format_marker!(self),
            start.elapsed(),
        );
    }

    pub fn update_index(
        &mut self,
        reindex_type: ReindexType,
        book_content: &DashMap<Book, ExpandedBookData>,
        tokenizer: &Tokenizer,
    ) {
        match reindex_type {
            ReindexType::PartialReindex(books) => {
                let book_count = books.len();
                tracing::info!("Reindexing {book_count} book(s){}", format_marker!(self));
                for book in books {
                    if let Some(data) = book_content.get(&book) {
                        self.reindex_usj(book, data.usj(), tokenizer);
                    }
                }
            }
            ReindexType::Unindex(book) => {
                self.replace_from_indexer(book, BookIndexer::new());
            }
            ReindexType::FullReindex => {
                tracing::info!("Reindexing all books{}", format_marker!(self));
                let start = Instant::now();
                self.interner = Interner::new();
                self.references_and_names_by_word.clear();
                self.words_by_book.clear();
                book_content
                    .par_iter()
                    .map(|entry| {
                        let mut indexer = BookIndexer::new();
                        indexer.index_usj(entry.value().usj(), tokenizer);
                        (*entry.key(), indexer)
                    })
                    .collect::<LinkedList<_>>()
                    .into_iter()
                    .for_each(|(book, indexer)| self.replace_from_indexer(book, indexer));
                self.interner.shrink_to_fit();
                self.references_and_names_by_word.shrink_to_fit();
                self.words_by_book.shrink_to_fit();
                tracing::info!(
                    "Reindexed all books{} ({} words) in {:?}",
                    format_marker!(self),
                    self.references_and_names_by_word.len(),
                    start.elapsed(),
                );
            }
        }
        print_memory_stats();
    }

    pub fn clear_index(&mut self) {
        *self = Self {
            log_marker: mem::take(&mut self.log_marker),
            ..Default::default()
        };
        tracing::info!("Cleared index{}", format_marker!(self));
        print_memory_stats();
    }
}

type ReferenceLocationVec = Vec<(BookReference, TextRange)>;

pub struct BookIndexer {
    results: HashMap<String, (Option<Box<str>>, ReferenceLocationVec)>,
    current_chapter: Option<NonZeroU8>,
    current_verses: Option<VerseRange>,
    current_path: UsjPath,
    current_text: String,
    current_paths: Vec<TextLocation>,
}

impl BookIndexer {
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
            current_chapter: None,
            current_verses: None,
            current_path: tiny_vec![],
            current_text: String::new(),
            current_paths: vec![],
        }
    }

    pub fn indexed_words(&self) -> usize {
        self.results.len()
    }

    // This is the function that decides what gets indexed and what doesn't
    pub fn index_usj(&mut self, usj: &UsjContent, tokenizer: &Tokenizer) {
        match usj {
            UsjContent::Root(UsjRoot { content, .. }) => {
                self.for_with_path(content, |this, child| this.index_usj(child, tokenizer));
            }

            UsjContent::Paragraph { content, .. } | UsjContent::Character { content, .. }
                if !usj.is_title_para() && self.current_chapter.is_some() =>
            {
                let start_idx = self.current_text.len();
                self.for_with_path(content, |this, child| match child {
                    ParaContent::Usj(usj) => this.index_usj(usj, tokenizer),
                    ParaContent::Plain(text) => this.push_text(text),
                });
                match usj {
                    UsjContent::Paragraph { .. } => self.flush_text(tokenizer),
                    UsjContent::Character {
                        marker: ContentMarker::W(()),
                        attributes,
                        ..
                    } => {
                        if self.current_verses.is_some()
                            && let Some(lemma) = attributes.get("lemma")
                        {
                            let start_char = self.current_text[..start_idx].chars().count();
                            let end_char =
                                start_char + self.current_text[start_idx..].chars().count();
                            let reference = self.unwrap_current_reference();
                            for token in tokenizer.tokenize(lemma) {
                                if !token.is_word() {
                                    continue;
                                }
                                self.insert_lemma(
                                    token.lemma,
                                    &lemma[token.byte_start..token.byte_end],
                                    start_char..end_char,
                                    reference,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }

            UsjContent::Chapter { number, .. } => {
                self.current_chapter = Some(number.value);
                self.current_verses = None;
            }
            UsjContent::Verse { number, .. } => {
                self.flush_text(tokenizer);
                self.current_verses = Some(number.value);
            }

            _ => {}
        }
    }

    fn for_with_path<T>(&mut self, content: &Vec<T>, action: impl Fn(&mut Self, &T)) {
        self.current_path.push(0);
        for child in content {
            action(self, child);
            *self.current_path.last_mut().unwrap() += 1;
        }
        self.current_path.pop();
    }

    fn push_text(&mut self, text: &str) {
        if self.current_chapter.is_none() || self.current_verses.is_none() {
            return;
        }
        self.current_text.push_str(text);
        self.current_paths
            .extend(text.chars().enumerate().map(|(idx, _)| TextLocation {
                usj_path: self.current_path.clone(),
                char: idx as u16,
            }));
    }

    fn flush_text(&mut self, tokenizer: &Tokenizer) {
        if self.current_text.is_empty() {
            return;
        }
        // unwrap() is safe because current_text cannot be non-empty while either is None
        let reference = self.unwrap_current_reference();
        let mut text = mem::take(&mut self.current_text);
        for token in tokenizer.tokenize(&text) {
            if !token.is_word() {
                continue;
            }
            self.insert_lemma(
                token.lemma,
                &text[token.byte_start..token.byte_end],
                token.char_start..token.char_end,
                reference,
            );
        }
        text.clear();
        self.current_text = text;
        self.current_paths.clear();
    }

    fn unwrap_current_reference(&self) -> BookReference {
        BookReference {
            chapter: self.current_chapter.unwrap(),
            verses: self.current_verses.unwrap(),
        }
    }

    fn insert_lemma(
        &mut self,
        lemma: Cow<'_, str>,
        name: &str,
        char_range: Range<usize>,
        reference: BookReference,
    ) {
        let has_name = name != lemma && !name.nfd().eq(lemma.chars());
        let range = self.get_text_location(char_range.start, false)
            ..self.get_text_location(char_range.end, true);
        let (name_result, result) = self.results.entry(lemma.into_owned()).or_default();
        if has_name {
            name_result.get_or_insert_with(|| name.to_string().into_boxed_str());
        }
        result.push((reference, range));
    }

    fn get_text_location(&self, char_idx: usize, is_end: bool) -> TextLocation {
        if char_idx == self.current_paths.len() {
            let last = &self.current_paths[char_idx - 1];
            TextLocation {
                usj_path: last.usj_path.clone(),
                char: last.char + 1,
            }
        } else {
            let mut element = &self.current_paths[char_idx];
            if is_end && element.char == 0 && char_idx > 0 {
                element = &self.current_paths[char_idx - 1];
                TextLocation {
                    usj_path: element.usj_path.clone(),
                    char: element.char + 1,
                }
            } else {
                element.clone()
            }
        }
    }
}

pub type UsjPath = TinyVec<[u16; 4]>;

#[derive(Clone, Debug, Serialize, Deserialize, rkyv::Serialize, rkyv::Deserialize, Archive)]
pub struct TextLocation {
    pub usj_path: UsjPath,
    pub char: u16,
}

pub type TextRange = Range<TextLocation>;

impl SubAssign<ParaIndex> for TextLocation {
    fn sub_assign(&mut self, rhs: ParaIndex) {
        let root = rhs.0 as u16;
        let inner = rhs.1 as u16;
        if self.usj_path[0] == root {
            self.usj_path[0] -= root;
            self.usj_path[1] -= inner;
        } else {
            self.usj_path[0] -= root;
            self.usj_path[1] = 0;
        }
    }
}

pub struct ExpandedReferencesIter<'a> {
    map_iter: hash_map::Iter<'a, Book, Box<[BookSearchResult]>>,
    book_iter: slice::Iter<'a, BookSearchResult>,
    current_book: Book,
}

impl<'a> Iterator for ExpandedReferencesIter<'a> {
    type Item = (BibleReference, &'a TextRange);

    fn next(&mut self) -> Option<Self::Item> {
        match self.book_iter.next() {
            Some(result) => Some((BibleReference::new(self.current_book, result.0), &result.1)),
            None => match self.map_iter.next() {
                Some((book, result_box)) => {
                    self.current_book = *book;
                    self.book_iter = result_box.iter();
                    let result = self.book_iter.next().unwrap();
                    Some((BibleReference::new(*book, result.0), &result.1))
                }
                None => None,
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let books_remaining = self.map_iter.len();
        let refs_remaining = self.book_iter.len();
        let lower = books_remaining.saturating_add(refs_remaining);
        let upper = if books_remaining == 0 {
            Some(refs_remaining)
        } else {
            None
        };
        (lower, upper)
    }
}

impl FusedIterator for ExpandedReferencesIter<'_> {}

pub struct ExpandedNamesAndCountsIter<'a> {
    iter: hash_map::Iter<'a, InternerSymbol, IndexedWord>,
    interner: &'a Interner,
}

impl<'a> ExpandedNamesAndCountsIter<'a> {
    fn map_to_item(
        interner: &'a Interner,
        (symbol, word): (&'a InternerSymbol, &'a IndexedWord),
    ) -> (&'a str, usize) {
        (
            word.name
                .as_deref()
                .unwrap_or_else(|| interner.resolve(*symbol).unwrap()),
            word.total,
        )
    }
}

impl<'a> Iterator for ExpandedNamesAndCountsIter<'a> {
    type Item = (&'a str, usize);

    fn next(&mut self) -> Option<Self::Item> {
        Some(Self::map_to_item(self.interner, self.iter.next()?))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.len()
    }

    fn fold<B, F>(self, init: B, f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let interner = self.interner;
        self.iter
            .map(|x| Self::map_to_item(interner, x))
            .fold(init, f)
    }
}

impl ExactSizeIterator for ExpandedNamesAndCountsIter<'_> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl FusedIterator for ExpandedNamesAndCountsIter<'_> {}
