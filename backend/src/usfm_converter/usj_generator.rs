use crate::usj::UsjContent;
use std::num::NonZeroU8;
use tree_sitter::TreeCursor;

pub struct UsjGenerator<'a> {
    book_slug: Option<&'a str>,
    current_chapter: Option<NonZeroU8>,
}

impl UsjGenerator<'_> {
    pub fn new() -> Self {
        Self {
            book_slug: None,
            current_chapter: None,
        }
    }

    pub fn convert_node(&self, cursor: &mut TreeCursor, into: &mut UsjContent) {
        let node_kind = cursor.node().kind().trim_start_matches('\\');
        if let Some(handler) = DISPATCH_MAP.get(node_kind) {
            handler(self, cursor, into);
        } else if node_kind.ends_with("Attribute") {
            self.convert_node_attrib(cursor, into);
        } else if !node_kind.is_empty() && node_kind != "|" && cursor.goto_first_child() {
            loop {
                self.convert_node(cursor, into);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    pub fn convert_node_attrib(&self, cursor: &mut TreeCursor, into: &mut UsjContent) {}
}

const DISPATCH_MAP: phf::Map<&'static str, fn(&UsjGenerator, &mut TreeCursor, &mut UsjContent)> =
    phf::phf_map!();
