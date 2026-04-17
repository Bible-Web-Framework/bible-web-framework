use crate::book_data::{Book, BookFromStrError};
use crate::verse_range::{VerseRange, VerseRangeParseError};
use oxicode::{Decode, Encode};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::fmt::{Display, Formatter};
use std::num::{NonZeroU8, ParseIntError};
use std::str::FromStr;
use thiserror::Error;

#[derive(
    Debug, Clone, Eq, PartialEq, Hash, SerializeDisplay, DeserializeFromStr, Encode, Decode,
)]
pub struct UsjIdentifier {
    pub book: Book,
    pub chapter: NonZeroU8,
    pub verse: Option<VerseRange>,
}

impl Display for UsjIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} {}", self.book.usfm_id(), self.chapter))?;
        if let Some(verse) = self.verse {
            f.write_fmt(format_args!(":{verse}"))?;
        }
        Ok(())
    }
}

impl FromStr for UsjIdentifier {
    type Err = IdentifierParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const CHAPTER_PATTERN: ere::Regex<3> = ere::compile_regex!(r"^([A-Z1-4]{3}) ?([0-9]+)$");
        const VERSE_PATTERN: ere::Regex<3> = ere::compile_regex!(r"^([A-Z1-4]{3}) ?([a-z0-9:-]*)$");
        if let Some([_, Some(book), Some(chapter)]) = CHAPTER_PATTERN.exec(s) {
            let book = book.parse()?;
            let chapter = chapter.parse()?;
            Ok(UsjIdentifier {
                book,
                chapter,
                verse: None,
            })
        } else if let Some([_, Some(book), Some(rest)]) = VERSE_PATTERN.exec(s) {
            let book = book.parse()?;
            if let Some((chapter, verse)) = rest.split_once(':') {
                let chapter = chapter.parse()?;
                let verse = verse.parse()?;
                Ok(UsjIdentifier {
                    book,
                    chapter,
                    verse: Some(verse),
                })
            } else {
                Err(IdentifierParseError::NoColon)
            }
        } else {
            Err(IdentifierParseError::InvalidIdentifier)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum IdentifierParseError {
    #[error("Identifier isn't a valid chapter or verse identifier")]
    InvalidIdentifier,
    #[error("Invalid book in identifier: {0}")]
    InvalidBook(#[from] BookFromStrError),
    #[error("Invalid chapter in identifier: {0}")]
    InvalidChapter(#[from] ParseIntError),
    #[error("Missing : in verse identifier")]
    NoColon,
    #[error("Invalid verse in identifier: {0}")]
    InvalidVerse(#[from] VerseRangeParseError),
}

#[cfg(test)]
mod test {
    use crate::book_data::Book;
    use crate::nz_u8;
    use crate::usj::identifier::UsjIdentifier;
    use crate::verse_range::VerseRange;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse() {
        assert_eq!(
            "GEN 1".parse(),
            Ok(UsjIdentifier {
                book: Book::Genesis,
                chapter: nz_u8!(1),
                verse: None,
            }),
        );
        assert_eq!(
            "GEN 5:10-12".parse(),
            Ok(UsjIdentifier {
                book: Book::Genesis,
                chapter: nz_u8!(5),
                verse: Some(VerseRange::const_new(nz_u8!(10), nz_u8!(12))),
            }),
        );
    }
}
