use crate::book_data::Book;
use crate::reference::ParseReferenceError::OutOfBoundsChapter;
use regex::Regex;
use std::fmt::{Debug, Formatter};
use std::sync::LazyLock;
use thiserror::Error;

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ChapterReference {
    pub book: Book,
    pub chapter: u8,
    pub verses: (u8, u8),
}

impl Debug for ChapterReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{:?} {}:{}-{}",
            self.book, self.chapter, self.verses.0, self.verses.1
        ))
    }
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
    #[error("Verse {0} is larger than verse {1}")]
    OutOfOrderVerses(u8, u8),
}

pub fn parse_references(reference: &str) -> Vec<Result<ChapterReference, ParseReferenceError>> {
    let mut book = None;
    reference
        .replace(" ", "")
        .split([';', ','])
        .filter(|x| !x.is_empty())
        .map(|x| parse_reference_part(x, &mut book))
        .collect()
}

fn parse_reference_part(
    reference: &str,
    book: &mut Option<Book>,
) -> Result<ChapterReference, ParseReferenceError> {
    static BOOK_DATA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        /*
        Do we even need this regex? We could just check everything up to the first number (presuming that's after the initial)
        and then check it against the possible book names/abbreviations. If it's not in there, assume it's a search term (e.g., "Jesus").
        But we also need to know if somebody searches for simply "John", which is a book name and a person's name and differentiate.

        Can find chapter number using regex "\p{N}" starting from index 1.
        */
        Regex::new(r"(^[\p{N}I]{0,3}\s*[\p{L}\s]+)(\p{N}*:?.*)").unwrap()
    });

    let remainder = if let Some(book_data) = BOOK_DATA_REGEX.captures(reference) {
        let book_str = book_data.get(1).unwrap().as_str();
        *book = Some(
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
    Ok(if let Some((chapter, verses)) = remainder.split_once(':') {
        let chapter = chapter
            .parse()
            .map_err(|_| ParseReferenceError::InvalidChapter(chapter.to_string()))?;
        let verse_count = book
            .verse_count(chapter)
            .ok_or(OutOfBoundsChapter(book, chapter))?;
        let parse_verse = |verse: &str, default_verse| {
            if let Some(default_verse) = default_verse
                && verse.is_empty()
            {
                return Ok(default_verse);
            }
            let verse = verse
                .parse()
                .map_err(|_| ParseReferenceError::InvalidVerse(verse.to_string()))?;
            if verse < 1 || verse > verse_count {
                return Err(ParseReferenceError::OutOfBoundsVerse(book, chapter, verse));
            }
            Ok(verse)
        };
        ChapterReference {
            book,
            chapter,
            verses: if let Some((verse_start, verse_end)) = verses.split_once('-') {
                let start_verse = parse_verse(verse_start, Some(1))?;
                let end_verse = parse_verse(verse_end, Some(verse_count))?;
                if start_verse > end_verse {
                    return Err(ParseReferenceError::OutOfOrderVerses(
                        start_verse,
                        end_verse,
                    ));
                }
                (start_verse, end_verse)
            } else {
                let verse = parse_verse(verses, None)?;
                (verse, verse)
            },
        }
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
        ChapterReference {
            book,
            chapter,
            verses: (1, verse_count),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::ParseReferenceError::*;
    use super::{ChapterReference, parse_references};
    use crate::book_data::Book::*;

    macro_rules! reference_result {
        (Ok($book:ident $chapter:literal:$verse_start:literal-$verse_end:literal)) => {
            Ok(ChapterReference {
                book: $book,
                chapter: $chapter,
                verses: ($verse_start, $verse_end),
            })
        };

        (Err($error:expr)) => {
            Err($error)
        };
    }

    macro_rules! assert_parse {
        ($reference:literal, $($result_type:ident$result_value:tt),+ $(,)?) => {
            assert_eq!(
                parse_references($reference),
                vec![$(reference_result!($result_type$result_value)),+]
            )
        };
    }

    #[test]
    fn test_parse_success() {
        assert_parse!("1John1", Ok(FirstJohn 1:1-10));
        assert_parse!(
            "James 1:1-4;Hosea4;Lk6:1-14;7,9:1-9,10:16",
            Ok(James 1:1-4),
            Ok(Hosea 4:1-19),
            Ok(Luke 6:1-14),
            Ok(Luke 7:1-50),
            Ok(Luke 9:1-9),
            Ok(Luke 10:16-16),
        );
        assert_parse!(
            "Proverbs1,,3",
            Ok(Proverbs 1:1-33),
            Ok(Proverbs 3:1-35),
        );
        assert_parse!("John 1:1;,", Ok(John 1:1-1));
        assert_parse!("John 1:1;,3", Ok(John 1:1-1), Ok(John 3:1-36));
        assert_parse!("John 1:-3", Ok(John 1:1-3));
        assert_parse!("John 1:6-", Ok(John 1:6-51));
    }

    #[test]
    fn test_parse_failure() {
        assert_parse!("John 50", Err(OutOfBoundsChapter(John, 50)));
        assert_parse!("John 1:134", Err(OutOfBoundsVerse(John, 1, 134)));
        assert_parse!("Beginning", Err(InvalidBook("Beginning".to_string())));
        assert_parse!("John", Err(MissingChapter));
        assert_parse!(
            "John 1:1;Hello",
            Ok(John 1:1-1),
            Err(InvalidBook("Hello".to_string())),
        );
        assert_parse!(
            "John 1:1;Acts",
            Ok(John 1:1-1),
            Err(MissingChapter),
        );
        assert_parse!(
            "John1;:3",
            Ok(John 1:1-51),
            Err(InvalidChapter("".to_string())),
        );
        assert_parse!("John:3", Err(InvalidChapter("".to_string())));
        assert_parse!(
            "John1:1:;4",
            Err(InvalidVerse("1:".to_string())),
            Ok(John 4:1-54),
        );
        assert_parse!(
            "John 1:1;3:",
            Ok(John 1:1-1),
            Err(InvalidVerse("".to_string())),
        );
        assert_parse!("John 1:6-3", Err(OutOfOrderVerses(6, 3)));
    }
}
