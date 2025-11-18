use hashlink::LinkedHashMap;
use serde::Deserialize;
use std::fmt::Write;
use std::num::NonZeroU8;
use std::path::Path;
use std::{env, fs};
use unicase::UniCase;

#[derive(Debug, Deserialize)]
struct BookInfo<'a> {
    usfm_id: &'a str,
    aliases: Vec<&'a str>,
    verse_counts: Vec<NonZeroU8>,
}

fn main() {
    println!("cargo::rerun-if-changed=src/books.json");

    let books = fs::read("src/books.json").unwrap();
    let books: LinkedHashMap<&str, BookInfo> = serde_json::from_slice(&books).unwrap();

    let mut book_names = r#"
        #[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, enum_map::Enum)]
        pub enum Book {
    "#
    .to_string();

    let mut verse_counts_result = "&[".to_string();
    let mut usfm_ids_result = "match self {".to_string();
    let mut book_aliases_result = phf_codegen::Map::new();
    for (book_name, book) in books {
        let _ = writeln!(book_names, "{book_name},");

        let _ = writeln!(verse_counts_result, "&[");
        for length in book.verse_counts {
            let _ = writeln!(verse_counts_result, "{length},");
        }
        let _ = writeln!(verse_counts_result, "],");

        let _ = writeln!(usfm_ids_result, "Book::{book_name} => {:?},", book.usfm_id);

        let book_str = format!("Book::{book_name}");
        for alias in book.aliases {
            book_aliases_result.entry(UniCase::new(alias), book_str.clone());
        }

        let usfm_id = UniCase::new(book.usfm_id);
        let book_name = UniCase::new(book_name);
        if usfm_id != book_name {
            book_aliases_result.entry(usfm_id, book_str.clone());
        }
        book_aliases_result.entry(book_name, book_str);
    }
    book_names.push('}');
    verse_counts_result.push(']');
    usfm_ids_result.push('}');

    fs::write(
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("book.rs"),
        book_names,
    )
    .unwrap();
    fs::write(
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("verse_counts.rs"),
        verse_counts_result,
    )
    .unwrap();
    fs::write(
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("usfm_ids.rs"),
        usfm_ids_result,
    )
    .unwrap();
    fs::write(
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("book_aliases.rs"),
        book_aliases_result.build().to_string(),
    )
    .unwrap();
}
