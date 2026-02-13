use hashlink::LinkedHashMap;
use permutate::Permutator;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::num::NonZeroU8;
use std::path::Path;
use std::{env, fs};
use unicase::UniCase;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BooksFile<'a> {
    common_aliases: HashMap<&'a str, Vec<&'a str>>,
    #[serde(borrow)]
    books: LinkedHashMap<&'a str, PossiblySimpleBookInfo<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PossiblySimpleBookInfo<'a> {
    Simple(&'a str),
    Standard(BookInfo<'a>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BookInfo<'a> {
    _comment: Option<&'a [u8]>,
    usfm_id: &'a str,
    #[serde(default)]
    aliases: Vec<BookAlias<'a>>,
    #[serde(default)]
    exclude_aliases: HashSet<&'a str>,
    verse_counts: Vec<NonZeroU8>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BookAlias<'a> {
    Simple(&'a str),
    Permutations(Vec<BookVecOrAlias<'a>>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BookVecOrAlias<'a> {
    Alias(&'a str),
    Vec(Vec<&'a str>),
}

impl<'a> PossiblySimpleBookInfo<'a> {
    fn into_book_info(self) -> BookInfo<'a> {
        match self {
            Self::Simple(usfm_id) => BookInfo {
                _comment: None,
                usfm_id,
                aliases: vec![],
                exclude_aliases: HashSet::default(),
                verse_counts: vec![],
            },
            Self::Standard(info) => info,
        }
    }
}

impl<'a> BookAlias<'a> {
    fn permute(
        &self,
        common_aliases: &'a HashMap<&str, Vec<&str>>,
        mut handler: impl FnMut(Cow<'a, str>),
    ) {
        let get_alias = |alias| {
            common_aliases
                .get(alias)
                .unwrap_or_else(|| panic!("Unknown alias '{alias}'"))
        };
        match self {
            Self::Simple(alias) => handler(Cow::Borrowed(alias)),
            Self::Permutations(groups) if groups.len() > 1 => {
                let groups = groups
                    .iter()
                    .map(|x| match x {
                        BookVecOrAlias::Alias(alias) => get_alias(alias).as_slice(),
                        BookVecOrAlias::Vec(vec) => vec.as_slice(),
                    })
                    .collect::<Vec<_>>();
                let mut permutator = Permutator::new(&groups);
                let mut current_groups = vec![""; groups.len()];
                while permutator.next_with_buffer(&mut current_groups) {
                    let mut new_alias = String::new();
                    for alias in &current_groups {
                        new_alias.push_str(alias);
                    }
                    handler(Cow::Owned(new_alias));
                }
            }
            Self::Permutations(group) => match &group[0] {
                BookVecOrAlias::Alias(alias_group) => {
                    for alias in get_alias(alias_group) {
                        handler(Cow::Borrowed(alias));
                    }
                }
                BookVecOrAlias::Vec(aliases) => {
                    for alias in aliases {
                        handler(Cow::Owned(alias.to_string()));
                    }
                }
            },
        }
    }
}

fn main() {
    println!("cargo::rerun-if-changed=../books.json");

    let books = fs::read("../books.json").unwrap();
    let mut books: BooksFile = serde_json::from_slice(&books).unwrap();

    for (alias_name, aliases) in &mut books.common_aliases {
        aliases.insert(0, alias_name);
    }

    let mut book_names = r#"
        #[derive(Debug, Hash, Ord, PartialOrd, VariantArray, EnumSetType)]
        #[enumset(serialize_repr = "list")]
        pub enum Book {
    "#
    .to_string();

    let mut verse_counts_result = "&[".to_string();
    let mut usfm_ids_result = "match self {".to_string();
    let mut book_aliases_result = phf_codegen::Map::new();
    for (book_name, book) in books.books {
        let book = book.into_book_info();
        let _ = writeln!(book_names, "{book_name},");

        let _ = writeln!(verse_counts_result, "&[");
        for length in book.verse_counts {
            let _ = writeln!(verse_counts_result, "{length},");
        }
        let _ = writeln!(verse_counts_result, "],");

        let _ = writeln!(usfm_ids_result, "Book::{book_name} => {:?},", book.usfm_id);

        let book_str = format!("Book::{book_name}");

        let usfm_id = UniCase::new(Cow::Borrowed(book.usfm_id));
        let book_name = UniCase::new(Cow::Borrowed(book_name));

        let mut exclude_aliases = book.exclude_aliases;
        for alias in book.aliases {
            alias.permute(&books.common_aliases, |alias| {
                if !exclude_aliases.remove(alias.as_ref()) {
                    let alias = UniCase::new(alias);
                    if alias != usfm_id && alias != book_name {
                        book_aliases_result.entry(alias, book_str.clone());
                    }
                }
            });
        }

        assert!(
            exclude_aliases.is_empty(),
            "{book_name}.exclude_aliases failed to exclude the following aliases: {exclude_aliases:?}"
        );

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
