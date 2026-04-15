pub mod content;
pub mod identifier;
pub mod loader;
pub mod marker;
pub mod root;

use crate::book_data::Book;
use crate::usj::marker::ContentMarker;
use crate::utils::{AsBorrowed, CloneToOwned, ToOwnedStatic};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsjBookInfo {
    pub book: Book,
    pub description: Option<String>,
}

impl Display for UsjBookInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(description) = &self.description {
            f.write_fmt(format_args!("{} ({})", self.book, description))
        } else {
            self.book.fmt(f)
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TranslatedBookInfo<'a> {
    pub running_header: Option<Cow<'a, str>>,
    pub long_book_name: Option<Cow<'a, str>>,
    pub short_book_name: Option<Cow<'a, str>>,
    pub book_abbreviation: Option<Cow<'a, str>>,
}

impl TranslatedBookInfo<'_> {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        [
            &self.running_header,
            &self.long_book_name,
            &self.short_book_name,
            &self.book_abbreviation,
        ]
        .into_iter()
        .filter_map(Option::as_deref)
    }

    pub fn is_full(&self) -> bool {
        matches!(
            (
                &self.running_header,
                &self.long_book_name,
                &self.short_book_name,
                &self.book_abbreviation
            ),
            (Some(_), Some(_), Some(_), Some(_)),
        )
    }

    pub fn short_name(&self, book: Book) -> &str {
        self.short_book_name
            .as_ref()
            .or(self.running_header.as_ref())
            .or(self.book_abbreviation.as_ref())
            .or(self.long_book_name.as_ref())
            .map_or(book.usfm_id(), Cow::deref)
    }
}

impl CloneToOwned for TranslatedBookInfo<'_> {
    type Output = TranslatedBookInfo<'static>;

    fn clone_to_owned(&self) -> Self::Output {
        TranslatedBookInfo {
            running_header: self.running_header.clone_to_owned(),
            long_book_name: self.long_book_name.clone_to_owned(),
            short_book_name: self.short_book_name.clone_to_owned(),
            book_abbreviation: self.book_abbreviation.clone_to_owned(),
        }
    }
}

impl<'a, 'b: 'a> AsBorrowed<'a> for TranslatedBookInfo<'b> {
    type Output = TranslatedBookInfo<'a>;

    fn as_borrowed(&'a self) -> Self::Output {
        TranslatedBookInfo {
            running_header: self.running_header.as_borrowed(),
            long_book_name: self.long_book_name.as_borrowed(),
            short_book_name: self.short_book_name.as_borrowed(),
            book_abbreviation: self.book_abbreviation.as_borrowed(),
        }
    }
}

impl ToOwnedStatic for TranslatedBookInfo<'_> {
    type Output = TranslatedBookInfo<'static>;

    fn to_owned_static(self) -> Self::Output {
        TranslatedBookInfo {
            running_header: self.running_header.to_owned_static(),
            long_book_name: self.long_book_name.to_owned_static(),
            short_book_name: self.short_book_name.to_owned_static(),
            book_abbreviation: self.book_abbreviation.to_owned_static(),
        }
    }
}

pub fn is_title_marker(marker: ContentMarker) -> bool {
    use ContentMarker::*;
    matches!(
        marker,
        Mt(_) | Mte(_) | Ms(_) | Mr(_) | S(_) | Sr(_) | R(_) | D(_) | Sp(_) | Sd(_),
    )
}

pub type ParaIndex = (usize, usize);
