use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Write};
use unicode_normalization::UnicodeNormalization;

include!(concat!(env!("OUT_DIR"), "/book.rs"));

impl Book {
    #[allow(unused_variables)]
    pub fn verse_count(&self, chapter: u8) -> Option<u8> {
        include!(concat!(env!("OUT_DIR"), "/verse_counts.rs"))
    }

    pub fn usfm_id(&self) -> &'static str {
        include!(concat!(env!("OUT_DIR"), "/usfm_ids.rs"))
    }

    /// Requires that `additional_aliases` be a map from lowercase NFKC-normalized strings with no spaces
    pub fn parse(book: &str, additional_aliases: Option<&HashMap<String, Self>>) -> Option<Self> {
        let mut real_book = String::with_capacity(book.len());
        for ch in book.nfkc() {
            if ch.is_whitespace() {
                continue;
            }
            let _ = write!(real_book, "{}", ch.to_lowercase());
        }
        let real_book = real_book.as_str();
        BOOK_ALIASES
            .get(real_book)
            .copied()
            .or_else(|| additional_aliases.and_then(|x| x.get(real_book).copied()))
    }
}

pub const BOOK_ALIASES: phf::Map<&str, Book> =
    include!(concat!(env!("OUT_DIR"), "/book_aliases.rs"));

impl Serialize for Book {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.usfm_id())
    }
}

impl<'de> Deserialize<'de> for Book {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{Error, Unexpected, Visitor};
        struct Deserializer;
        impl<'de> Visitor<'de> for Deserializer {
            type Value = Book;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a book name, USFM ID, or English alias")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Book::parse(v, None).ok_or_else(|| Error::invalid_value(Unexpected::Str(v), &self))
            }
        }
        deserializer.deserialize_str(Deserializer)
    }
}

impl Display for Book {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Book::SongOfSolomon => f.write_str("Song of Solomon"),
            Book::FirstSamuel => f.write_str("1 Samuel"),
            Book::SecondSamuel => f.write_str("2 Samuel"),
            Book::FirstKings => f.write_str("1 Kings"),
            Book::SecondKings => f.write_str("2 Kings"),
            Book::FirstChronicles => f.write_str("1 Chronicles"),
            Book::SecondChronicles => f.write_str("2 Chronicles"),
            Book::FirstCorinthians => f.write_str("1 Corinthians"),
            Book::SecondCorinthians => f.write_str("2 Corinthians"),
            Book::FirstThessalonians => f.write_str("1 Thessalonians"),
            Book::SecondThessalonians => f.write_str("2 Thessalonians"),
            Book::FirstTimothy => f.write_str("1 Timothy"),
            Book::SecondTimothy => f.write_str("2 Timothy"),
            Book::FirstPeter => f.write_str("1 Peter"),
            Book::SecondPeter => f.write_str("2 Peter"),
            Book::FirstJohn => f.write_str("1 John"),
            Book::SecondJohn => f.write_str("2 John"),
            Book::ThirdJohn => f.write_str("3 John"),
            _ => f.write_fmt(format_args!("{self:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::book_data::Book;

    fn assert_parse(name: &str, book: Book) {
        assert_eq!(Book::parse(name, None), Some(book));
    }

    fn assert_parse_fail(name: &str) {
        assert_eq!(Book::parse(name, None), None);
    }

    #[test]
    fn test_parse_book() {
        assert_parse("Genesis", Book::Genesis);
        assert_parse("1 John", Book::FirstJohn);
        assert_parse_fail("Beginning");
    }
}
