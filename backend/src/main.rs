use std::{error::Error, fmt::Debug, sync::LazyLock};

use regex::{Regex, RegexBuilder};

#[derive(Debug)]
pub struct ChapterReference {
    book: String,
    chapter: u8,
    verses: (u8, u8),
}

fn main() {
    let full_ref = "James 1:1-4;Hosea4;Lk6:1-14;7,9:1-9,10:16";
    // let full_ref = "Beginning";
    let references = parse_references(full_ref).expect("Broke");
    // println!("{references:#?}");
    for reference in references {
        println!(
            "{} {}:{}-{}",
            reference.book, reference.chapter, reference.verses.0, reference.verses.1
        );
    }
}

pub fn parse_references(reference: &str) -> Result<Vec<ChapterReference>, Box<dyn Error>> {
    static BOOK_DATA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        /*
        Do we even need this regex? We could just check everything up to the first number (presuming that's after the initial)
        and then check it against the possible book names/abbreviations. If it's not in there, assume it's a search term (e.g., "Jesus").
        But we also need to know if somebody searches for simply "John", which is a book name and a person's name and differentiate.
        */
        RegexBuilder::new("([1-3]?[a-z]+)(.*)")
            .case_insensitive(true)
            .build()
            .unwrap()
    });

    let reference = reference.replace(" ", "");
    let sections = reference.split([';', ',']);

    let mut references = Vec::new();
    let mut book = "";

    for reference in sections {
        let remainder: &str;
        if let Some(book_data) = BOOK_DATA_REGEX.captures(reference) {
            book = book_data.get(1).unwrap().as_str();
            remainder = book_data.get(2).unwrap().as_str();
        } else {
            if book.is_empty() {
                continue;
            };
            remainder = reference;
        };
        let book = book;
        if let Some((chapter, verses)) = remainder.split_once(':') {
            references.push(ChapterReference {
                book: book.to_string(),
                chapter: chapter.parse()?,
                verses: if let Some((verse_start, verse_end)) = verses.split_once('-') {
                    (verse_start.parse()?, verse_end.parse()?)
                } else {
                    let verse = verses.parse()?;
                    (verse, verse)
                },
            })
        } else {
            references.push(ChapterReference {
                book: book.to_string(),
                chapter: remainder.parse()?,
                verses: (1, 176),
            });
        }
    }

    Ok(references)
}
