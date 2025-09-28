use crate::book_data::Book;
use crate::reference::ParseReferenceError::OutOfBoundsChapter;
use regex::Regex;
use std::sync::LazyLock;
use thiserror::Error;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ChapterReference {
    pub book: Book,
    pub chapter: u8,
    pub verses: (u8, u8),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ParseReferenceError {
    #[error("No chapter specified")]
    MissingChapter,
    #[error("Invalid chapter '{0}'")]
    InvalidChapter(String),
    #[error("Invalid verse '{0}'")]
    InvalidVerse(String),
    #[error("Invalid book '{0}'")]
    InvalidBook(String),
    #[error("Unknown chapter {1} for book {0:?}")]
    OutOfBoundsChapter(Book, u8),
    #[error("Unknown verse {2} for chapter {0:?}:{1}")]
    OutOfBoundsVerse(Book, u8, u8),
}

pub fn parse_references(reference: &str) -> Result<Vec<ChapterReference>, ParseReferenceError> {
    static BOOK_DATA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        /*
        Do we even need this regex? We could just check everything up to the first number (presuming that's after the initial)
        and then check it against the possible book names/abbreviations. If it's not in there, assume it's a search term (e.g., "Jesus").
        But we also need to know if somebody searches for simply "John", which is a book name and a person's name and differentiate.

        Can find chapter number using regex "\p{N}" starting from index 1.
        */
        Regex::new(r"(^[\p{N}I]{0,3}\s*[\p{L}\s]+)(\p{N}*:?.*)").unwrap()
    });

    let reference = reference.replace(" ", "");
    let sections = reference.split([';', ',']);

    let mut references = Vec::new();
    let mut book = None;

    for reference in sections {
        if reference.is_empty() {
            continue;
        }
        let remainder = if let Some(book_data) = BOOK_DATA_REGEX.captures(reference) {
            let book_str = book_data.get(1).unwrap().as_str();
            book = Some(
                Book::parse(book_str, None)
                    .ok_or_else(|| ParseReferenceError::InvalidBook(book_str.to_string()))?,
            );
            book_data.get(2).unwrap().as_str()
        } else if book.is_none() {
            return Err(Book::parse(reference, None).map_or_else(
                || ParseReferenceError::InvalidBook(reference.to_string()),
                |_| ParseReferenceError::MissingChapter,
            ));
        } else {
            reference
        };
        let book = book.unwrap();
        if let Some((chapter, verses)) = remainder.split_once(':') {
            let chapter = chapter
                .parse()
                .map_err(|_| ParseReferenceError::InvalidChapter(chapter.to_string()))?;
            let verse_count = book
                .verse_count(chapter)
                .ok_or(OutOfBoundsChapter(book, chapter))?;
            let parse_verse = |verse: &str| {
                let verse = verse
                    .parse()
                    .map_err(|_| ParseReferenceError::InvalidVerse(verse.to_string()))?;
                if verse < 1 || verse > verse_count {
                    return Err(ParseReferenceError::OutOfBoundsVerse(book, chapter, verse));
                }
                Ok(verse)
            };
            references.push(ChapterReference {
                book,
                chapter,
                verses: if let Some((verse_start, verse_end)) = verses.split_once('-') {
                    (parse_verse(verse_start)?, parse_verse(verse_end)?)
                } else {
                    let verse = parse_verse(verses)?;
                    (verse, verse)
                },
            })
        } else {
            let chapter = remainder.parse().map_err(|_| {
                Book::parse(reference, None).map_or_else(
                    || ParseReferenceError::InvalidBook(remainder.to_string()),
                    |_| ParseReferenceError::MissingChapter,
                )
            })?;
            let verse_count = book
                .verse_count(chapter)
                .ok_or(OutOfBoundsChapter(book, chapter))?;
            references.push(ChapterReference {
                book,
                chapter,
                verses: (1, verse_count),
            });
        }
    }

    Ok(references)
}

#[cfg(test)]
mod tests {
    use super::{ChapterReference, ParseReferenceError, parse_references};
    use crate::book_data::Book::{FirstJohn, Hosea, James, John, Luke, Proverbs};

    macro_rules! assert_references_eq {
        ($reference:literal, $($book:ident $chapter:literal:$verse_start:literal-$verse_end:literal),+ $(,)?) => {
            assert_eq!(
                parse_references($reference).unwrap(),
                vec![$(ChapterReference {
                    book: $book,
                    chapter: $chapter,
                    verses: ($verse_start, $verse_end),
                }),+]
            )
        };
    }

    macro_rules! assert_invalid {
        ($reference:literal, $error:expr $(,)?) => {
            assert_eq!(parse_references($reference).unwrap_err(), $error)
        };
    }

    #[test]
    fn test_parse_success() {
        assert_references_eq!("1John1", FirstJohn 1:1-10);
        assert_references_eq!(
            "James 1:1-4;Hosea4;Lk6:1-14;7,9:1-9,10:16",
            James 1:1-4,
            Hosea 4:1-19,
            Luke 6:1-14,
            Luke 7:1-50,
            Luke 9:1-9,
            Luke 10:16-16,
        );
        assert_references_eq!(
            "Proverbs1,,3",
            Proverbs 1:1-33,
            Proverbs 3:1-35,
        );
        assert_references_eq!("John 1:1;,", John 1:1-1);
        assert_references_eq!("John 1:1;,3", John 1:1-1, John 3:1-36);
    }

    #[test]
    fn test_parse_failure() {
        assert_invalid!("John 50", ParseReferenceError::OutOfBoundsChapter(John, 50));
        assert_invalid!(
            "John 1:134",
            ParseReferenceError::OutOfBoundsVerse(John, 1, 134)
        );
        assert_invalid!(
            "Beginning",
            ParseReferenceError::InvalidBook("Beginning".to_string()),
        );
        assert_invalid!("John", ParseReferenceError::MissingChapter);
        assert_invalid!(
            "John 1:1;Hello",
            ParseReferenceError::InvalidBook("Hello".to_string()),
        );
        assert_invalid!("John 1:1;Acts", ParseReferenceError::MissingChapter);
        assert_invalid!(
            "John1;:3",
            ParseReferenceError::InvalidChapter("".to_string())
        );
        assert_invalid!(
            "John:3",
            ParseReferenceError::InvalidChapter("".to_string())
        );
        assert_invalid!(
            "John1:1:;4",
            ParseReferenceError::InvalidVerse("1:".to_string())
        );
        assert_invalid!(
            "John 1:1;3:",
            ParseReferenceError::InvalidVerse("".to_string())
        );
    }
}
