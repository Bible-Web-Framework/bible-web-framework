use std::collections::HashMap;
use std::fmt::Write;
use unicode_normalization::UnicodeNormalization;

include!(concat!(env!("OUT_DIR"), "/book.rs"));

impl Book {
    #[allow(unused_variables)]
    pub fn verse_count(&self, chapter: u8) -> Option<u8> {
        include!(concat!(env!("OUT_DIR"), "/verse_counts.rs"))
    }

    pub fn parse(book: &str, additional_aliases: Option<&HashMap<&str, Self>>) -> Option<Self> {
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
