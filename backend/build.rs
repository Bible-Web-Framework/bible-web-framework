use hashlink::LinkedHashMap;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-changed=book_aliases.json");
    println!("cargo::rerun-if-changed=book_verse_counts.json");

    let book_aliases = fs::read("book_aliases.json").unwrap();
    let book_aliases: LinkedHashMap<&str, Vec<&str>> =
        serde_json::from_slice(&book_aliases).unwrap();

    let verse_counts = fs::read("book_verse_counts.json").unwrap();
    let verse_counts: HashMap<&str, LinkedHashMap<u8, u8>> =
        serde_json::from_slice(&verse_counts).unwrap();

    // let mut book_names = "#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]\npub enum Book {\n".to_string();
    let mut book_names = r#"
        #[derive(
            Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd,
            serde::Serialize, serde::Deserialize,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum Book {
    "#
    .to_string();

    let mut book_aliases_result = phf_codegen::Map::new();
    let mut verse_counts_result = "Some(match self {".to_string();
    for (book_name, aliases) in book_aliases {
        let _ = writeln!(book_names, "{book_name},");

        let book_str = format!("Book::{book_name}");
        for alias in aliases {
            book_aliases_result.entry(alias.to_lowercase(), book_str.clone());
        }
        book_aliases_result.entry(book_name.to_lowercase(), book_str);

        let _ = writeln!(verse_counts_result, "Book::{book_name} => match chapter {{");
        for (chapter, length) in &verse_counts[book_name] {
            let _ = writeln!(verse_counts_result, "{chapter} => {length},");
        }
        let _ = writeln!(verse_counts_result, "_ => return None,");
        let _ = writeln!(verse_counts_result, "}},");
    }
    book_names.push('}');
    verse_counts_result.push_str("})");

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
        Path::new(&env::var_os("OUT_DIR").unwrap()).join("book_aliases.rs"),
        book_aliases_result.build().to_string(),
    )
    .unwrap();
}
