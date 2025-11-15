use crate::book_data::Book;
use crate::reference::BookReference;
use crate::usj::{ParaContent, UsjContent, UsjRoot};
use crate::verse_range::VerseRange;
use charabia::Tokenize;
use enum_map::EnumMap;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::mem;
use std::num::NonZeroU8;
use std::ops::Range;
use std::time::Instant;

pub type SearchResultMap = EnumMap<Book, Vec<(BookReference, TextLocation)>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReindexType {
    NoReindex,
    PartialReindex(SmallVec<[Book; 2]>),
    Unindex(Book),
    FullReindex,
}

#[derive(Clone)]
pub struct BibleIndex {
    references_by_word: HashMap<Cow<'static, str>, BookReferenceMap>,
    words_by_book: EnumMap<Book, Vec<Cow<'static, str>>>,
}

#[derive(Clone, Default)]
struct BookReferenceMap {
    total: usize,
    by_book: SearchResultMap,
}

impl BibleIndex {
    pub fn new() -> Self {
        Self {
            references_by_word: HashMap::new(),
            words_by_book: EnumMap::default(),
        }
    }

    pub fn find<'a, 'b: 'a>(&'a self, lemma: &'b str) -> Option<&'a SearchResultMap> {
        match self.references_by_word.get(&Cow::Borrowed(lemma)) {
            Some(x) => Some(&x.by_book),
            None => None,
        }
    }

    pub fn replace_from_indexer(&mut self, book: Book, indexer: BookIndexer) {
        let old_words = mem::replace(
            &mut self.words_by_book[book],
            indexer.results.keys().cloned().collect(),
        );
        for word in old_words {
            if let Entry::Occupied(mut old_map_entry) = self.references_by_word.entry(word) {
                let old_map = old_map_entry.get_mut();
                old_map.total -= mem::take(&mut old_map.by_book[book]).len();
                if old_map.total == 0 {
                    old_map_entry.remove();
                }
            }
        }
        for (word, new_references) in indexer.results {
            let references = self.references_by_word.entry(word).or_default();
            references.total += new_references.len();
            references.by_book[book] = new_references;
        }
    }

    pub fn reindex_usj(&mut self, book: Book, usj: &UsjContent) {
        let start = Instant::now();
        let mut indexer = BookIndexer::new();
        indexer.index_usj(usj);
        let words = indexer.indexed_words();
        self.replace_from_indexer(book, indexer);
        tracing::info!("Reindexed {book} ({words} words) in {:?}", start.elapsed());
    }

    pub fn update_index(
        &mut self,
        reindex_type: ReindexType,
        book_content: &HashMap<Book, UsjContent>,
    ) {
        match reindex_type {
            ReindexType::NoReindex => {}
            ReindexType::PartialReindex(books) => {
                let book_count = books.len();
                tracing::info!("Reindexing {book_count} book(s)");
                for book in books {
                    if let Some(usj) = book_content.get(&book) {
                        self.reindex_usj(book, usj);
                    }
                }
            }
            ReindexType::Unindex(book) => {
                self.replace_from_indexer(book, BookIndexer::new());
            }
            ReindexType::FullReindex => {
                tracing::info!("Reindexing all books");
                let start = Instant::now();
                self.references_by_word.clear();
                self.words_by_book.clear();
                book_content
                    .par_iter()
                    .map(|(book, content)| {
                        let mut indexer = BookIndexer::new();
                        indexer.index_usj(content);
                        (*book, indexer)
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .for_each(|(book, indexer)| self.replace_from_indexer(book, indexer));
                tracing::info!(
                    "Reindexed all books ({} words) in {:?}",
                    self.references_by_word.len(),
                    start.elapsed()
                );
            }
        }
    }
}

pub struct BookIndexer {
    results: HashMap<Cow<'static, str>, Vec<(BookReference, TextLocation)>>,
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
            current_path: SmallVec::new(),
        }
    }

    pub fn indexed_words(&self) -> usize {
        self.results.len()
    }

    // This is the function that decides what gets indexed and what doesn't
    pub fn index_usj(&mut self, usj: &UsjContent) {
        match usj {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. } => {
                self.for_with_path(content, Self::index_usj);
            }

            UsjContent::Paragraph { content, .. }
            | UsjContent::Milestone { content, .. }
            | UsjContent::TableCell { content, .. } => {
                self.for_with_path(content, |this, child| match child {
                    ParaContent::Usj(usj) => this.index_usj(usj),
                    ParaContent::Plain(text) => this.index_text(text),
                });
            }

            UsjContent::Character { content, .. } => {
                if let Some(content) = content {
                    self.index_text(content);
                }
            }
            UsjContent::Chapter { number, .. } => self.current_chapter = Some(*number),
            UsjContent::Verse { number, .. } => self.current_verses = Some(*number),

            UsjContent::Book { .. }
            | UsjContent::Note { .. }
            | UsjContent::Sidebar { .. }
            | UsjContent::Figure { .. }
            | UsjContent::Reference { .. } => {}
        }
    }

    fn for_with_path<T>(&mut self, content: &Vec<T>, action: impl Fn(&mut Self, &T)) {
        self.current_path.push(0);
        for child in content {
            action(self, child);
            *self.current_path.last_mut().unwrap() += 1;
        }
        self.current_path.remove(self.current_path.len() - 1);
    }

    fn index_text(&mut self, text: &str) {
        let Some(reference) = self.current_chapter.and_then(|chapter| {
            Some(BookReference {
                chapter,
                verses: self.current_verses?,
            })
        }) else {
            return;
        };
        for token in text.tokenize() {
            if !token.is_word() {
                continue;
            }
            self.results
                .entry(Cow::Owned(token.lemma.into_owned()))
                .or_default()
                .push((
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
        for index in self.usj_path.iter().take(self.usj_path.len() - 1) {
            current = current.get_content(*index)?.left()?;
        }
        current.get_content(*self.usj_path.last()?)?.right()
    }
}
