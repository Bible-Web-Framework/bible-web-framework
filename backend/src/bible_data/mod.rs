use crate::api::{ApiError, ApiResult};
use crate::bible_data::baked::{BakedBibleData, BakedBookData};
use crate::bible_data::expanded::{ExpandedBibleData, ExpandedBookData};
use crate::book_data::{Book, BookParseOptions};
use crate::usj::content::UsjContent;
use crate::usj::{ParaIndex, TranslatedBookInfo};
use crate::utils::{ArcOrRef, AsBorrowed, CloneToOwned, ToOwnedStatic, ToUnicaseCow};
use crate::verse_range::VerseRange;
use auto_enums::auto_enum;
use charabia::Language;
use config::BibleConfig;
use dashmap::mapref;
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::num::NonZeroU8;
use std::ops::Deref;
use std::sync::Arc;
use unicase::UniCase;

pub mod baked;
pub mod config;
pub mod expanded;

pub type DynMultiBibleData = dyn MultiBibleData + Send + Sync;

pub trait MultiBibleData {
    fn default_bible(&self) -> &str;

    fn bibles(&self) -> Vec<String>;

    fn get_bible(&self, bible: &str) -> Option<BibleData<'_>>;

    fn get_or_api_error(&self, bible: String) -> ApiResult<BibleData<'_>> {
        self.get_bible(&bible)
            .ok_or_else(|| ApiError::UnknownBible(bible))
    }
}

impl<T: MultiBibleData> MultiBibleData for Arc<T> {
    fn default_bible(&self) -> &str {
        self.deref().default_bible()
    }

    fn bibles(&self) -> Vec<String> {
        self.deref().bibles()
    }

    fn get_bible(&self, bible: &str) -> Option<BibleData<'_>> {
        self.deref().get_bible(bible)
    }
}

pub enum BibleData<'a> {
    Expanded(mapref::one::Ref<'a, String, ExpandedBibleData>),
    Baked(&'a BakedBibleData),
}

impl BibleData<'_> {
    pub fn config(&self) -> ArcOrRef<'_, BibleConfig> {
        match self {
            Self::Expanded(data) => ArcOrRef::Arc(data.config.read().clone()),
            Self::Baked(data) => ArcOrRef::Ref(&data.config),
        }
    }

    pub fn book(&self, book: Book) -> Option<BookData<'_>> {
        match self {
            Self::Expanded(data) => data.books.get(&book).map(BookData::Expanded),
            Self::Baked(data) => data.books[book]
                .as_ref()
                .map(|book_data| BookData::Baked(data, book_data)),
        }
    }

    pub fn books(&self) -> EnumSet<Book> {
        match self {
            Self::Expanded(data) => data.books.iter().map(|book| *book.key()).collect(),
            Self::Baked(data) => data
                .books
                .iter()
                .filter_map(|(book, data)| data.is_some().then_some(book))
                .collect(),
        }
    }

    pub fn book_parse_options(&self) -> impl BookParseOptions {
        struct Options<'a> {
            config: ArcOrRef<'a, BibleConfig>,
            data: &'a BibleData<'a>,
        }

        impl BookParseOptions for Options<'_> {
            fn languages(&self) -> Option<&[Language]> {
                self.config.search.languages.as_deref()
            }

            fn lookup_book(&self, str: UniCase<&str>) -> Option<Book> {
                let str = str.to_cow();
                self.config
                    .book_aliases
                    .get(&str)
                    .copied()
                    .or_else(|| match self.data {
                        BibleData::Expanded(data) => data
                            .books
                            .iter()
                            .find_map(|book| book.names.contains(&str).then_some(*book.key())),
                        BibleData::Baked(data) => data.full_book_names.get(&str).copied(),
                    })
            }

            fn book_allowed(&self, book: Book) -> bool {
                match self.data {
                    BibleData::Expanded(data) => data.books.contains_key(&book),
                    BibleData::Baked(data) => data.books[book].is_some(),
                }
            }
        }

        Options {
            config: self.config(),
            data: self,
        }
    }
}

pub enum BookData<'a> {
    Expanded(mapref::one::Ref<'a, Book, ExpandedBookData>),
    Baked(&'a BakedBibleData, &'a BakedBookData),
}

impl BookData<'_> {
    pub fn to_usj(&self) -> Cow<'_, UsjContent> {
        match self {
            Self::Expanded(data) => Cow::Borrowed(&data.usj),
            Self::Baked(bible_data, book_data) => Cow::Owned(book_data.load_full_usj(bible_data)),
        }
    }

    pub fn translated_book_info(&self) -> TranslatedBookInfo<'_> {
        match self {
            Self::Expanded(data) => data.usj.unwrap_root().translated_book_info(),
            Self::Baked(_, book_data) => book_data.translated_book_info.as_borrowed(),
        }
    }

    pub fn chapter_infos(&self) -> Vec<ChapterInfo> {
        match self {
            Self::Expanded(data) => data
                .usj
                .unwrap_root()
                .content
                .iter()
                .filter_map(|value| {
                    let UsjContent::Chapter {
                        number,
                        alt_number,
                        pub_number,
                        ..
                    } = value
                    else {
                        return None;
                    };
                    Some(ChapterInfo {
                        number: Cow::Borrowed(&number.string),
                        alt_number: alt_number.as_deref().map(Cow::Borrowed),
                        pub_number: pub_number.as_deref().map(Cow::Borrowed),
                    })
                })
                .collect(),
            Self::Baked(bible_data, book_data) => book_data
                .list_chapter_usjs(bible_data)
                .map(|value| {
                    let UsjContent::Chapter {
                        number,
                        alt_number,
                        pub_number,
                        ..
                    } = value
                    else {
                        panic!("Non chapter data returned");
                    };
                    ChapterInfo {
                        number: Cow::Owned(number.string),
                        alt_number: alt_number.map(Cow::Owned),
                        pub_number: pub_number.map(Cow::Owned),
                    }
                })
                .collect(),
        }
    }

    pub fn find_reference(
        &self,
        chapter: NonZeroU8,
        verse_range: VerseRange,
    ) -> Option<(ParaIndex, Vec<UsjContent>)> {
        match self {
            Self::Expanded(data) => data.usj.unwrap_root().find_reference(chapter, verse_range),
            Self::Baked(bible_data, book_data) => {
                book_data.find_reference(bible_data, chapter, verse_range)
            }
        }
    }

    pub fn has_chapter(&self, chapter: NonZeroU8) -> bool {
        match self {
            Self::Expanded(data) => data.usj.unwrap_root().content.iter().any(
                |x| matches!(x, UsjContent::Chapter { number, .. } if number.value == chapter),
            ),
            Self::Baked(_, book_data) => book_data.has_chapter(chapter),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChapterInfo<'a> {
    pub number: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_number: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pub_number: Option<Cow<'a, str>>,
}

impl CloneToOwned for ChapterInfo<'_> {
    type Output = ChapterInfo<'static>;

    fn clone_to_owned(&self) -> Self::Output {
        ChapterInfo {
            number: self.number.clone_to_owned(),
            alt_number: self.alt_number.clone_to_owned(),
            pub_number: self.pub_number.clone_to_owned(),
        }
    }
}

impl<'a, 'b: 'a> AsBorrowed<'a> for ChapterInfo<'b> {
    type Output = ChapterInfo<'a>;

    fn as_borrowed(&'a self) -> Self::Output {
        ChapterInfo {
            number: self.number.as_borrowed(),
            alt_number: self.alt_number.as_borrowed(),
            pub_number: self.pub_number.as_borrowed(),
        }
    }
}

impl ToOwnedStatic for ChapterInfo<'_> {
    type Output = ChapterInfo<'static>;

    fn to_owned_static(self) -> Self::Output {
        ChapterInfo {
            number: self.number.to_owned_static(),
            alt_number: self.alt_number.to_owned_static(),
            pub_number: self.pub_number.to_owned_static(),
        }
    }
}
