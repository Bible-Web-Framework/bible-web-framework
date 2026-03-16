use crate::book_data::Book;
use crate::book_data::BookParseOptions;
use crate::nz_u8;
use crate::utils::normalize_str;
use crate::utils::serde_as::VerseRangeAsTuple;
use crate::verse_range::VerseRange;
use rangemap::StepLite;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::fmt::{Debug, Display, Formatter};
use std::num::NonZeroU8;
use std::ops::{Deref, RangeInclusive};
use std::str::FromStr;
use strum::VariantArray;
use subslice_offset::SubsliceOffset;
use thiserror::Error;

#[derive(Copy, Clone, Default, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BibleReference {
    pub book: Book,
    #[serde(flatten)]
    pub reference: BookReference,
}

impl BibleReference {
    pub const fn split_to_range(self) -> RangeInclusive<Self> {
        let split = self.reference.split_to_range();
        Self {
            book: self.book,
            reference: *split.start(),
        }..=Self {
            book: self.book,
            reference: *split.end(),
        }
    }
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

impl StepLite for BibleReference {
    fn add_one(&self) -> Self {
        assert!(
            self.verses.is_single_verse(),
            "{self} is a verse range, not a single verse, cannot add_one",
        );
        assert!(
            self.book.chapter_count().is_some(),
            "Cannot call add_one on non-book reference {self}",
        );
        if self.verses.first() < self.book.verse_count(self.chapter).unwrap() {
            Self {
                book: self.book,
                reference: BookReference {
                    chapter: self.chapter,
                    verses: VerseRange::new_single_verse(self.verses.first().saturating_add(1)),
                },
            }
        } else if self.chapter < self.book.chapter_count().unwrap() {
            Self {
                book: self.book,
                reference: BookReference {
                    chapter: self.chapter.saturating_add(1),
                    verses: VerseRange::new_single_verse(nz_u8!(1)),
                },
            }
        } else if self.book < Book::LetterToTheLaodiceans {
            Self {
                book: Book::VARIANTS[self.book as usize + 1],
                reference: BookReference {
                    chapter: nz_u8!(1),
                    verses: VerseRange::new_single_verse(nz_u8!(1)),
                },
            }
        } else {
            #[cfg(debug_assertions)]
            panic!("Cannot add_one to {self}, as it would go into FrontMatter");
            #[cfg(not(debug_assertions))]
            *self
        }
    }

    fn sub_one(&self) -> Self {
        assert!(
            self.verses.is_single_verse(),
            "{self:?} is a verse range, not a single verse, cannot sub_one",
        );
        assert!(
            self.book.chapter_count().is_some(),
            "Cannot call sub_one on non-book reference {self}",
        );
        if self.verses.first_u8() > 1 {
            Self {
                book: self.book,
                reference: BookReference {
                    chapter: self.chapter,
                    verses: VerseRange::new_single_verse(
                        NonZeroU8::new(self.verses.first_u8() - 1).unwrap(),
                    ),
                },
            }
        } else if self.chapter.get() > 1 {
            let new_chapter = NonZeroU8::new(self.chapter.get() - 1).unwrap();
            Self {
                book: self.book,
                reference: BookReference {
                    chapter: new_chapter,
                    verses: VerseRange::new_single_verse(
                        self.book.verse_count(new_chapter).unwrap(),
                    ),
                },
            }
        } else if self.book > Book::Genesis {
            let new_book = Book::VARIANTS[self.book as usize - 1];
            let new_chapter = new_book.chapter_count().unwrap();
            Self {
                book: new_book,
                reference: BookReference {
                    chapter: new_chapter,
                    verses: VerseRange::new_single_verse(
                        new_book.verse_count(new_chapter).unwrap(),
                    ),
                },
            }
        } else {
            #[cfg(debug_assertions)]
            panic!("Cannot sub_one from {self}, as it would go out of bounds");
            #[cfg(not(debug_assertions))]
            *self
        }
    }
}

impl FromStr for BibleReference {
    type Err = ParseReferenceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_reference_part(s, &mut ParseState::default(), &())
    }
}

#[serde_as]
#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BookReference {
    pub chapter: NonZeroU8,
    #[serde_as(as = "VerseRangeAsTuple")]
    pub verses: VerseRange,
}

impl BookReference {
    pub const fn split_to_range(self) -> RangeInclusive<Self> {
        let split = self.verses.split_to_range();
        Self {
            chapter: self.chapter,
            verses: *split.start(),
        }..=Self {
            chapter: self.chapter,
            verses: *split.end(),
        }
    }

    pub fn is_single_verse(&self) -> bool {
        self.verses.is_single_verse()
    }
}

impl Default for BookReference {
    fn default() -> Self {
        Self {
            chapter: nz_u8!(1),
            verses: VerseRange::default(),
        }
    }
}

impl Debug for BookReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}:{:?}", self.chapter, self.verses))
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

pub fn parse_references(reference: &str, options: &impl BookParseOptions) -> Vec<ReferenceResult> {
    let mut state = ParseState::default();
    normalize_str(reference)
        .split([';', ','])
        .filter(|x| !x.is_empty())
        .map(|x| parse_reference_part(x, &mut state, options))
        .collect()
}

#[derive(Default)]
struct ParseState {
    book: Option<Book>,
    chapter: Option<NonZeroU8>,
}

fn parse_reference_part(
    reference: &str,
    state: &mut ParseState,
    options: &impl BookParseOptions,
) -> ReferenceResult {
    let book_data = {
        let without_prefix_nums = reference.trim_start_matches(char::is_numeric);
        let num_index = without_prefix_nums
            .find(|c: char| c.is_numeric() || c == ':')
            .take_if(|i| *i > 0);
        num_index.map(|x| {
            reference.split_at(reference.subslice_offset(without_prefix_nums).unwrap() + x)
        })
    };

    let remainder = if let Some((book_str, remainder)) = book_data {
        state.book = Some(Book::parse(book_str, options).ok_or_else(|| {
            ParseReferenceError::UnknownBook {
                book: book_str.to_string(),
                valid_otherwise: parse_book_reference(
                    Book::default(),
                    state,
                    reference,
                    remainder,
                    options,
                )
                .is_ok(),
            }
        })?);
        state.chapter = None;
        remainder
    } else if state.book.is_none() {
        return Err(Book::parse(reference, options).map_or_else(
            || ParseReferenceError::UnknownBook {
                book: reference.to_string(),
                valid_otherwise: false,
            },
            |_| ParseReferenceError::MissingChapter,
        ));
    } else {
        reference
    };

    parse_book_reference(state.book.unwrap(), state, reference, remainder, options)
}

fn parse_book_reference(
    book: Book,
    state: &mut ParseState,
    full_reference: &str,
    reference_remainder: &str,
    options: &impl BookParseOptions,
) -> ReferenceResult {
    let process_chapter_number = |chapter| -> Result<_, ParseReferenceError> {
        let verse_count = book
            .verse_count(chapter)
            .ok_or(ParseReferenceError::OutOfBoundsChapter { book, chapter })?;
        let parse_verse = move |verse: &str, default_verse: Option<NonZeroU8>| {
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

        Ok(move |verses: &str| {
            if let Some((verse_start, verse_end)) = verses.split_once('-') {
                VerseRange::new(
                    parse_verse(verse_start, Some(nz_u8!(1)))?,
                    parse_verse(verse_end, Some(verse_count))?,
                )
                .map_err(|verses| ParseReferenceError::OutOfOrderVerses { verses })
            } else {
                let verse = parse_verse(verses, None)?;
                Ok(VerseRange::new(verse, verse).unwrap())
            }
        })
    };

    let verify_not_book = || {
        Book::parse(full_reference, options).map_or_else(
            || ParseReferenceError::UnknownBook {
                book: reference_remainder.to_string(),
                valid_otherwise: false,
            },
            |_| ParseReferenceError::MissingChapter,
        )
    };

    Ok(
        if let Some((chapter, verses)) = reference_remainder.split_once(':') {
            let chapter =
                chapter
                    .parse::<NonZeroU8>()
                    .map_err(|_| ParseReferenceError::InvalidChapter {
                        chapter: chapter.to_string(),
                    })?;
            state.chapter = Some(chapter);
            let parse_verses = process_chapter_number(chapter)?;
            BibleReference {
                book,
                reference: BookReference {
                    chapter,
                    verses: parse_verses(verses)?,
                },
            }
        } else if let Some(chapter) = state.chapter {
            let parse_verses = process_chapter_number(chapter)?;
            BibleReference {
                book,
                reference: BookReference {
                    chapter,
                    verses: parse_verses(reference_remainder).map_err(|_| verify_not_book())?,
                },
            }
        } else {
            let chapter = reference_remainder
                .parse::<NonZeroU8>()
                .map_err(|_| verify_not_book())?;
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
    ($book:ident $chapter:literal:$verse:literal) => {
        $crate::reference_value!($book $chapter:$verse-$verse)
    };

    ($book:ident $chapter:literal:$verse_start:literal-$verse_end:literal) => {
        $crate::reference::BibleReference {
            book: $crate::book_data::Book::$book,
            reference: $crate::reference::BookReference {
                chapter: $crate::nz_u8!($chapter),
                verses: $crate::verse_range::VerseRange::const_new(
                    $crate::nz_u8!($verse_start),
                    $crate::nz_u8!($verse_end),
                ),
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
    use cool_asserts::assert_panics;
    use pretty_assertions::assert_eq;
    use rangemap::StepLite;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use unicase::UniCase;

    macro_rules! reference_result {
        (Ok($book:ident $chapter:literal:$verse_start:literal$(-$verse_end:literal)?)) => {
            Ok(reference_value!($book $chapter:$verse_start$(-$verse_end)?))
        };

        (Err($error:expr)) => {
            Err($error)
        };
    }

    macro_rules! parse_references {
        ($reference:literal,) => {
            parse_references($reference, &())
        };

        ($reference:literal, $($name:literal => $book:ident),+) => {
            parse_references($reference, &&HashMap::from([$((UniCase::new(Cow::Borrowed($name)), $book)),+]))
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
            Ok(Luke 6:7-7),
            Ok(Luke 9:1-9),
            Ok(Luke 10:16-16),
        );
        assert_parse!(
            "Proverbs1,,3",
            Ok(Proverbs 1:1-33),
            Ok(Proverbs 3:1-35),
        );
        assert_parse!("John 1:1;,", Ok(John 1:1-1));
        assert_parse!("John 1:1;,3", Ok(John 1:1-1), Ok(John 1:3-3));
        assert_parse!("John 1:-3", Ok(John 1:1-3));
        assert_parse!("John 1:6-", Ok(John 1:6-51));
        assert_parse!(
            "ヨハネ 1:1",
            "ヨハネ" => John,
            Ok(John 1:1-1)
        );
        assert_parse!("acts2:1,3,5", Ok(Acts 2:1-1), Ok(Acts 2:3-3), Ok(Acts 2:5-5));
        assert_parse!("acts2,3,5", Ok(Acts 2:1-47), Ok(Acts 3:1-26), Ok(Acts 5:1-42));
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
            Ok(John 1:4-4),
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
        assert_parse!("John", Err(MissingChapter));
        assert_parse!("John;Acts", Err(MissingChapter), Err(MissingChapter));
    }

    #[test]
    fn test_reference_increment() {
        assert_panics!(reference_value!(Genesis 1:1-2).add_one(), |msg| assert_eq!(
            msg,
            "Genesis 1:1-2 is a verse range, not a single verse, cannot add_one"
        ));
        assert_panics!(
            reference_value!(FrontMatter 1:1-1).add_one(),
            |msg| assert_eq!(
                msg,
                "Cannot call add_one on non-book reference FrontMatter 1:1"
            )
        );
        assert_eq!(
            reference_value!(Genesis 1:1).add_one(),
            reference_value!(Genesis 1:2),
        );
        assert_eq!(
            reference_value!(Genesis 1:31).add_one(),
            reference_value!(Genesis 2:1),
        );
        assert_eq!(
            reference_value!(Genesis 50:26).add_one(),
            reference_value!(Exodus 1:1),
        );
        assert_panics!(
            reference_value!(LetterToTheLaodiceans 1:20).add_one(),
            |msg| assert_eq!(
                msg,
                "Cannot add_one to LetterToTheLaodiceans 1:20, as it would go into FrontMatter"
            )
        );
    }

    #[test]
    fn test_reference_decrement() {
        assert_panics!(reference_value!(Genesis 1:1-2).sub_one(), |msg| assert_eq!(
            msg,
            "Genesis 1:1-2 is a verse range, not a single verse, cannot sub_one"
        ));
        assert_panics!(
            reference_value!(FrontMatter 1:1-1).sub_one(),
            |msg| assert_eq!(
                msg,
                "Cannot call sub_one on non-book reference FrontMatter 1:1"
            )
        );
        assert_eq!(
            reference_value!(Genesis 1:2).sub_one(),
            reference_value!(Genesis 1:1),
        );
        assert_eq!(
            reference_value!(Genesis 2:1).sub_one(),
            reference_value!(Genesis 1:31),
        );
        assert_eq!(
            reference_value!(Exodus 1:1).sub_one(),
            reference_value!(Genesis 50:26),
        );
        assert_panics!(reference_value!(Genesis 1:1).sub_one(), |msg| assert_eq!(
            msg,
            "Cannot sub_one from Genesis 1:1, as it would go out of bounds"
        ));
    }
}
