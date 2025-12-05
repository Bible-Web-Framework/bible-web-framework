use crate::book_data::AdditionalAliases;
use crate::book_data::Book;
use crate::nz_u8;
use crate::utils::with_normalized_str;
use crate::verse_range::VerseRange;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::num::NonZeroU8;
use std::ops::Deref;
use thiserror::Error;

#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BibleReference {
    pub book: Book,
    #[serde(flatten)]
    pub reference: BookReference,
}

impl Deref for BibleReference {
    type Target = BookReference;

    fn deref(&self) -> &Self::Target {
        &self.reference
    }
}

impl Debug for BibleReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?} {:?}", self.book, self.reference))
    }
}

impl Display for BibleReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} {}", self.book, self.reference.chapter))?;
        if self.reference.verses.first_u8() != 1
            || Some(self.reference.verses.last()) != self.book.verse_count(self.reference.chapter)
        {
            f.write_fmt(format_args!(":{}", self.reference.verses))
        } else {
            Ok(())
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BookReference {
    pub chapter: NonZeroU8,
    pub verses: VerseRange,
}

impl Debug for BookReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}:{:?}", self.chapter, self.verses,))
    }
}

impl Display for BookReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.chapter))?;
        if self.verses.first_u8() != 1 {
            f.write_fmt(format_args!(":{}", self.verses))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParseReferenceError {
    #[error("No chapter specified")]
    MissingChapter,
    #[error("Invalid chapter '{chapter}'")]
    InvalidChapter { chapter: String },
    #[error("Invalid verse number '{verse}'")]
    InvalidVerse { verse: String },
    #[error("Unknown book '{book}'")]
    UnknownBook { book: String, valid_otherwise: bool },
    #[error("Unknown chapter {chapter} for book {book}")]
    OutOfBoundsChapter { book: Book, chapter: NonZeroU8 },
    #[error("Unknown verse {verse} for chapter {book} {chapter}")]
    OutOfBoundsVerse {
        book: Book,
        chapter: NonZeroU8,
        verse: NonZeroU8,
    },
    #[error("Verse {} is larger than verse {}", verses.0, verses.1)]
    OutOfOrderVerses { verses: (NonZeroU8, NonZeroU8) },
}

impl ParseReferenceError {
    pub fn is_syntax(&self) -> bool {
        use ParseReferenceError::*;
        matches!(
            self,
            MissingChapter
                | InvalidChapter { .. }
                | InvalidVerse { .. }
                | UnknownBook {
                    valid_otherwise: false,
                    ..
                },
        )
    }
}

pub type ReferenceResult = Result<BibleReference, ParseReferenceError>;

pub fn parse_references(
    reference: &str,
    additional_aliases: AdditionalAliases,
) -> Vec<ReferenceResult> {
    with_normalized_str(reference, |reference| {
        let mut book = None;
        reference
            .split([';', ','])
            .filter(|x| !x.is_empty())
            .map(|x| parse_reference_part(x, &mut book, additional_aliases))
            .collect()
    })
}

fn parse_reference_part(
    reference: &str,
    book: &mut Option<Book>,
    additional_aliases: AdditionalAliases,
) -> ReferenceResult {
    let book_data = {
        let without_prefix_nums = reference.trim_start_matches(char::is_numeric);
        let num_index = without_prefix_nums.find(char::is_numeric);
        num_index.map(|x| without_prefix_nums.split_at(x))
    };

    let remainder = if let Some((book_str, remainder)) = book_data {
        *book = Some(Book::parse(book_str, additional_aliases).ok_or_else(|| {
            ParseReferenceError::UnknownBook {
                book: book_str.to_string(),
                valid_otherwise: parse_book_reference(
                    Book::Genesis,
                    reference,
                    remainder,
                    additional_aliases,
                )
                .is_ok(),
            }
        })?);
        remainder
    } else if book.is_none() {
        return Err(Book::parse(reference, additional_aliases).map_or_else(
            || ParseReferenceError::UnknownBook {
                book: reference.to_string(),
                valid_otherwise: false,
            },
            |_| ParseReferenceError::MissingChapter,
        ));
    } else {
        reference
    };

    parse_book_reference(book.unwrap(), reference, remainder, additional_aliases)
}

fn parse_book_reference(
    book: Book,
    full_reference: &str,
    reference_remainder: &str,
    additional_aliases: AdditionalAliases,
) -> ReferenceResult {
    Ok(
        if let Some((chapter, verses)) = reference_remainder.split_once(':') {
            let chapter =
                chapter
                    .parse::<NonZeroU8>()
                    .map_err(|_| ParseReferenceError::InvalidChapter {
                        chapter: chapter.to_string(),
                    })?;
            let verse_count = book
                .verse_count(chapter)
                .ok_or(ParseReferenceError::OutOfBoundsChapter { book, chapter })?;
            let parse_verse = |verse: &str, default_verse: Option<NonZeroU8>| {
                if let Some(default_verse) = default_verse
                    && verse.is_empty()
                {
                    return Ok(default_verse);
                }
                let verse = verse
                    .parse()
                    .map_err(|_| ParseReferenceError::InvalidVerse {
                        verse: verse.to_string(),
                    })?;
                if verse > verse_count {
                    return Err(ParseReferenceError::OutOfBoundsVerse {
                        book,
                        chapter,
                        verse,
                    });
                }
                Ok(verse)
            };
            BibleReference {
                book,
                reference: BookReference {
                    chapter,
                    verses: if let Some((verse_start, verse_end)) = verses.split_once('-') {
                        VerseRange::new(
                            parse_verse(verse_start, Some(nz_u8!(1)))?,
                            parse_verse(verse_end, Some(verse_count))?,
                        )
                        .map_err(|verses| ParseReferenceError::OutOfOrderVerses { verses })?
                    } else {
                        let verse = parse_verse(verses, None)?;
                        VerseRange::new(verse, verse).unwrap()
                    },
                },
            }
        } else {
            let chapter = reference_remainder.parse::<NonZeroU8>().map_err(|_| {
                Book::parse(full_reference, additional_aliases).map_or_else(
                    || ParseReferenceError::UnknownBook {
                        book: reference_remainder.to_string(),
                        valid_otherwise: false,
                    },
                    |_| ParseReferenceError::MissingChapter,
                )
            })?;
            let verse_count = book
                .verse_count(chapter)
                .ok_or(ParseReferenceError::OutOfBoundsChapter { book, chapter })?;
            BibleReference {
                book,
                reference: BookReference {
                    chapter,
                    verses: VerseRange::new(nz_u8!(1), verse_count).unwrap(),
                },
            }
        },
    )
}

#[cfg(test)]
#[macro_export]
macro_rules! reference_value {
    ($book:ident $chapter:literal:$verse_start:literal-$verse_end:literal) => {
        $crate::reference::BibleReference {
            book: $crate::book_data::Book::$book,
            reference: $crate::reference::BookReference {
                chapter: $crate::nz_u8!($chapter),
                verses: $crate::verse_range::VerseRange::new(
                    $crate::nz_u8!($verse_start),
                    $crate::nz_u8!($verse_end),
                )
                .expect("Invalid verse range as expected value in test"),
            },
        }
    };
}

#[cfg(test)]
mod tests {
    use super::ParseReferenceError::*;
    use super::parse_references;
    use crate::book_data::Book::*;
    use crate::nz_u8;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use unicase::UniCase;

    macro_rules! reference_result {
        (Ok($book:ident $chapter:literal:$verse_start:literal-$verse_end:literal)) => {
            Ok(reference_value!($book $chapter:$verse_start-$verse_end))
        };

        (Err($error:expr)) => {
            Err($error)
        };
    }

    macro_rules! parse_references {
        ($reference:literal,) => {
            parse_references($reference, None)
        };

        ($reference:literal, $($name:literal => $book:ident),+) => {
            parse_references($reference, Some(&HashMap::from([$((UniCase::new(Cow::Borrowed($name)), $book)),+])))
        };
    }

    macro_rules! assert_parse {
        ($reference:literal, $($name:literal => $book:ident,)* $($result_type:ident$result_value:tt),+ $(,)?) => {
            assert_eq!(
                parse_references!($reference, $($name => $book),*),
                vec![$(reference_result!($result_type$result_value)),+],
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
        assert_parse!(
            "ヨハネ 1:1",
            "ヨハネ" => John,
            Ok(John 1:1-1)
        );
    }

    #[test]
    fn test_parse_failure() {
        assert_parse!(
            "John 50",
            Err(OutOfBoundsChapter {
                book: John,
                chapter: nz_u8!(50),
            }),
        );
        assert_parse!(
            "John 1:134",
            Err(OutOfBoundsVerse {
                book: John,
                chapter: nz_u8!(1),
                verse: nz_u8!(134),
            }),
        );
        assert_parse!(
            "Beginning",
            Err(UnknownBook {
                book: "Beginning".to_string(),
                valid_otherwise: false,
            }),
        );
        assert_parse!(
            "Beginning 1:1",
            Err(UnknownBook {
                book: "Beginning".to_string(),
                valid_otherwise: true,
            }),
        );
        assert_parse!(
            "Beginning 1",
            Err(UnknownBook {
                book: "Beginning".to_string(),
                valid_otherwise: true,
            }),
        );
        assert_parse!("John", Err(MissingChapter));
        assert_parse!(
            "John 1:1;Hello",
            Ok(John 1:1-1),
            Err(UnknownBook {
                book: "Hello".to_string(),
                valid_otherwise: false,
            }),
        );
        assert_parse!(
            "John 1:1;Acts",
            Ok(John 1:1-1),
            Err(MissingChapter),
        );
        assert_parse!(
            "John1;:3",
            Ok(John 1:1-51),
            Err(InvalidChapter { chapter: "".to_string() }),
        );
        assert_parse!(
            "John:3",
            Err(InvalidChapter {
                chapter: "".to_string(),
            }),
        );
        assert_parse!(
            "John1:1:;4",
            Err(InvalidVerse { verse: "1:".to_string() }),
            Ok(John 4:1-54),
        );
        assert_parse!(
            "John 1:1;3:",
            Ok(John 1:1-1),
            Err(InvalidVerse { verse: "".to_string() }),
        );
        assert_parse!(
            "John 1:6-3",
            Err(OutOfOrderVerses {
                verses: (nz_u8!(6), nz_u8!(3)),
            }),
        );
    }
}
