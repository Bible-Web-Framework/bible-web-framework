use crate::usfm_converter::usj_generator::UsjGenerator;
use crate::usj::{UsjContent, UsjRoot};
use ere::compile_regex;
use miette::LabeledSpan;
use std::ops::{IndexMut, Range};
use std::string::ToString;
use std::sync::LazyLock;
use subslice_offset::SubsliceOffset;
use thiserror::Error;
use tree_sitter::{Language, Parser};

pub static LANGUAGE: LazyLock<Language> = LazyLock::new(tree_sitter_usfm3::language);

pub struct UsfmParser {
    pub usfm: String,
    syntax_tree: tree_sitter::Tree,
    pub warnings: Vec<UsfmWarning>,
    pub errors: Vec<LabeledSpan>,
}

impl UsfmParser {
    pub fn new(mut usfm: String) -> Result<UsfmParser, UsfmNewError> {
        if !usfm.starts_with('\\') {
            return Err(UsfmNewError::NoBackslashes);
        }
        let mut warnings = vec![];

        if let Some(lowercase_id) = find_lowercase_id(&usfm) {
            warnings.push(UsfmWarning::LowercaseBookCode);
            usfm.index_mut(lowercase_id).make_ascii_uppercase();
        }

        let mut parser = Parser::new();
        parser.set_language(&LANGUAGE).unwrap();
        let syntax_tree = parser.parse(&usfm, None).unwrap();

        let mut errors = vec![];
        let mut missing_walker = syntax_tree.walk();
        'walk_loop: loop {
            let node = missing_walker.node();
            if node.is_error() || node.is_missing() {
                let mut sexp = node.to_sexp();
                sexp.remove(0);
                sexp.remove(sexp.len() - 1);
                errors.push(LabeledSpan::at(node.start_byte()..node.end_byte(), sexp));
            } else if missing_walker.goto_first_child() {
                continue;
            }
            while !missing_walker.goto_next_sibling() {
                if !missing_walker.goto_parent() {
                    break 'walk_loop;
                }
            }
        }
        drop(missing_walker);

        Ok(UsfmParser {
            usfm,
            syntax_tree,
            warnings,
            errors,
        })
    }

    pub fn to_usj(&self) -> UsjContent {
        let mut result = UsjContent::Root(UsjRoot {
            version: "3.1".to_string(),
            content: vec![],
        });
        UsjGenerator::new().convert_node(&mut self.syntax_tree.walk(), &mut result);
        result
    }
}

#[derive(Debug, Error)]
pub enum UsfmNewError {
    #[error("Invalid input for USFM. Expected a string with \\ markups.")]
    NoBackslashes,
}

#[derive(Debug, Error)]
pub enum UsfmWarning {
    #[error("Found book code in lowercase")]
    LowercaseBookCode,
}

fn find_lowercase_id(usfm: &str) -> Option<Range<usize>> {
    for line in usfm.lines() {
        const LOWER_CASE_BOOK_CODE: ere::Regex<2> = compile_regex!(r"^\\id ([a-z0-9][a-z][a-z])");
        if let Some([_, Some(lower_case)]) = LOWER_CASE_BOOK_CODE.exec(line) {
            let offset = usfm.subslice_offset(lower_case).unwrap();
            return Some(offset..offset + lower_case.len());
        }
    }
    None
}
