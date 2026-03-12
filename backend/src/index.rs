use crate::book_data::Book;
use crate::reference::BookReference;
use crate::usj::{ParaContent, UsjContent, UsjRoot};
use crate::verse_range::VerseRange;
use charabia::Tokenizer;
use dashmap::DashMap;
use memory_stats::memory_stats;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use smallvec::{SmallVec, smallvec};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, LinkedList};
use std::num::NonZeroU8;
use std::ops::Range;
use std::time::Instant;
use string_interner::StringInterner;
use string_interner::backend::StringBackend;
use string_interner::symbol::SymbolU32;

pub type SearchResultMap = HashMap<Book, Box<[(BookReference, TextLocation)]>>;

type InternerSymbol = SymbolU32;
type InternerBackend = StringBackend<InternerSymbol>;
type Interner = StringInterner<InternerBackend>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReindexType {
    PartialReindex(SmallVec<[Book; 2]>),
    Unindex(Book),
    FullReindex,
}

pub struct BibleIndex {
    pub log_marker: Option<String>,
    interner: Interner,
    references_and_names_by_word: HashMap<InternerSymbol, (BookReferenceMap, Option<Box<str>>)>,
    words_by_book: HashMap<Book, Box<[InternerSymbol]>>,
}

impl Default for BibleIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Default)]
struct BookReferenceMap {
    total: usize,
    by_book: SearchResultMap,
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

impl BibleIndex {
    pub fn new() -> Self {
        Self {
            log_marker: None,
            interner: Interner::new(),
            references_and_names_by_word: HashMap::new(),
            words_by_book: HashMap::default(),
        }
    }

    pub fn find<'a, 'b: 'a>(&'a self, lemma: &'b str) -> Option<(&'a SearchResultMap, &'a str)> {
        match self
            .interner
            .get(lemma)
            .and_then(|s| self.references_and_names_by_word.get(&s))
        {
            Some((references, name)) => {
                Some((&references.by_book, name.as_deref().unwrap_or(lemma)))
            }
            None => None,
        }
    }

    pub fn iter_names_and_counts(&self) -> impl Iterator<Item = (&str, usize)> {
        self.references_and_names_by_word
            .iter()
            .map(|(symbol, (references, name))| {
                (
                    name.as_deref()
                        .unwrap_or_else(|| self.interner.resolve(*symbol).unwrap()),
                    references.total,
                )
            })
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
                let (old_map, _) = old_map_entry.get_mut();
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
            let (references, name) = self
                .references_and_names_by_word
                .entry(self.interner.get_or_intern(word))
                .or_default();
            if name.is_none() {
                *name = new_name;
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
        book_content: &DashMap<Book, UsjContent>,
        tokenizer: &Tokenizer,
    ) {
        match reindex_type {
            ReindexType::PartialReindex(books) => {
                let book_count = books.len();
                tracing::info!("Reindexing {book_count} book(s){}", format_marker!(self));
                for book in books {
                    if let Some(usj) = book_content.get(&book) {
                        self.reindex_usj(book, &usj, tokenizer);
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
                        indexer.index_usj(entry.value(), tokenizer);
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
        if let Some(memory) = memory_stats() {
            const MIB: usize = 1024 * 1024;
            tracing::info!(
                "Process memory usage: physical: {} MiB | virtual: {} MiB",
                memory.physical_mem / MIB,
                memory.virtual_mem / MIB,
            );
        }
    }
}

type ReferenceLocationVec = Vec<(BookReference, TextLocation)>;

pub struct BookIndexer {
    results: HashMap<String, (Option<Box<str>>, ReferenceLocationVec)>,
    current_chapter: Option<NonZeroU8>,
    current_verses: Option<VerseRange>,
    current_path: SmallVec<[usize; 4]>,
}

impl BookIndexer {
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
            current_chapter: None,
            current_verses: None,
            current_path: smallvec![],
        }
    }

    pub fn indexed_words(&self) -> usize {
        self.results.len()
    }

    // This is the function that decides what gets indexed and what doesn't
    pub fn index_usj(&mut self, usj: &UsjContent, tokenizer: &Tokenizer) {
        match usj {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. } => {
                self.for_with_path(content, |this, child| this.index_usj(child, tokenizer));
            }

            UsjContent::Paragraph { content, .. }
            | UsjContent::Character { content, .. }
            | UsjContent::TableCell { content, .. } => {
                if !usj.is_title_para() {
                    self.for_with_path(content, |this, child| match child {
                        ParaContent::Usj(usj) => this.index_usj(usj, tokenizer),
                        ParaContent::Plain(text) => this.index_text(text, tokenizer),
                    });
                }
            }

            UsjContent::Chapter { number, .. } => {
                self.current_chapter = Some(*number);
                self.current_verses = None;
            }
            UsjContent::Verse { number, .. } => self.current_verses = Some(*number),

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

    fn index_text(&mut self, text: &str, tokenizer: &Tokenizer) {
        let Some(reference) = self.current_chapter.and_then(|chapter| {
            Some(BookReference {
                chapter,
                verses: self.current_verses?,
            })
        }) else {
            return;
        };
        for token in tokenizer.tokenize(text) {
            if !token.is_word() {
                continue;
            }
            let name =
                Some(&text[token.byte_start..token.byte_end]).take_if(|x| *x != token.lemma());
            let (name_result, result) = self.results.entry(token.lemma.into_owned()).or_default();
            if let Some(name) = name {
                name_result.get_or_insert_with(|| name.to_string().into_boxed_str());
            }
            result.push((
                reference,
                TextLocation {
                    usj_path: self.current_path.clone(),
                    char_range: token.char_start..token.char_end,
                },
            ));
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextLocation {
    pub usj_path: SmallVec<[usize; 4]>,
    pub char_range: Range<usize>,
}

impl TextLocation {
    pub fn resolve_text_section<'a>(&self, content: &'a UsjContent) -> Option<&'a str> {
        let mut current = content;
        for &index in self.usj_path.iter().take(self.usj_path.len() - 1) {
            current = current.get_content(index)?.left()?;
        }
        current.get_content(*self.usj_path.last()?)?.right()
    }
}
