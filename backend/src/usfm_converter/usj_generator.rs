use crate::book_data::Book;
use crate::usj::{TableCellAlignment, UsjContent};
use crate::{nz_u8, usfm_queries};
use miette::{LabeledSpan, MietteDiagnostic, Severity};
use monostate::MustBeStr;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::num::NonZeroU8;
use std::str::FromStr;
use tree_sitter::{Node, TreeCursor};

pub struct UsjGenerator<'a> {
    pub source: &'a str,
    pub diagnostics: Vec<MietteDiagnostic>,
    current_book: Option<Book>,
    current_chapter: Option<NonZeroU8>,
}

impl<'a> UsjGenerator<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            diagnostics: vec![],
            current_book: None,
            current_chapter: None,
        }
    }
}

impl UsjGenerator<'_> {
    pub fn convert_node(&mut self, cursor: &mut TreeCursor, into: &mut UsjContent) {
        let node_kind = cursor.node().kind().trim_start_matches('\\');
        if let Some(handler) = DISPATCH_MAP.get(node_kind) {
            handler(self, cursor, into);
        } else if node_kind.ends_with("Attribute") {
            self.convert_node_attrib(cursor, into);
        } else if !node_kind.is_empty() && node_kind != "|" {
            for_each_child(cursor, |c| self.convert_node(c, into));
        }
    }

    fn convert_node_attrib(&mut self, cursor: &mut TreeCursor, into: &mut UsjContent) {
        let node = cursor.node();
        let mut attrib_name = &self.source[node.child(0).unwrap().byte_range()];

        if attrib_name == "|" {
            let parent_type = node.parent().unwrap().kind();
            let parent_type = parent_type.strip_suffix("Nested").unwrap_or(parent_type);
            if let Some(new_attrib_name) = DEFAULT_ATTRIB_MAP.get(parent_type) {
                attrib_name = new_attrib_name;
            }
        }

        if attrib_name == "src" {
            attrib_name = "file";
        }

        let attrib_value = ATTRIB_VAL_QUERY
            .captures(node, self.source)
            .get("attrib-val")
            .map(|(_, x)| x.trim())
            .unwrap();

        if let Some(attributes) = into.attributes_mut() {
            attributes.insert(attrib_name.to_string(), attrib_value.to_string());
        } else {
            self.unsupported_child(cursor, into, "Attributes not supported in");
        }
    }

    fn diagnostic(&mut self, node: Node, severity: Severity, message: impl Into<String>) {
        self.diagnostics.push(
            MietteDiagnostic::new(message)
                .with_severity(severity)
                .with_label(LabeledSpan::new_with_span(None, node.byte_range())),
        );
    }

    fn error(&mut self, node: Node, message: impl Into<String>) {
        self.diagnostic(node, Severity::Error, message);
    }

    fn unsupported_child(&mut self, cursor: &TreeCursor, into: &UsjContent, message: &str) {
        self.error(
            cursor.node(),
            format!("{message} {}", into.marker().unwrap_or_default()),
        );
    }

    fn try_push_text(&mut self, cursor: &TreeCursor, into: &mut UsjContent, content: String) {
        if !into.push_text_content(content) {
            self.unsupported_child(cursor, into, "Unexpected plain text under");
        }
    }

    fn try_push_usj(
        &mut self,
        cursor: &TreeCursor,
        into: &mut UsjContent,
        error_message: &str,
        content: UsjContent,
    ) {
        if !into.push_usj_content(content) {
            self.unsupported_child(cursor, into, error_message);
        }
    }

    fn parse_from_query<T: FromStr>(
        &mut self,
        captures: &HashMap<&str, (Node, &str)>,
        key: &str,
        what: &str,
    ) -> Result<Option<T>, T::Err>
    where
        T::Err: Display,
    {
        match captures
            .get(key)
            .map(|x| x.1.trim().parse::<T>().map_err(|e| (e, x)))
        {
            Some(Ok(book)) => Ok(Some(book)),
            None => Ok(None),
            Some(Err((err, (error_node, value)))) => {
                self.error(*error_node, format!("Invalid {what} \"{value}\": {err}"));
                Err(err)
            }
        }
    }
}

fn for_each_child(cursor: &mut TreeCursor, mut action: impl FnMut(&mut TreeCursor)) {
    if cursor.goto_first_child() {
        loop {
            action(cursor);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn push_text_node(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    let text_val = generator.source[cursor.node().byte_range()]
        .trim_end_matches(['\r', '\n'])
        .to_string();
    generator.try_push_text(cursor, into, text_val);
}

fn handle_verse_text(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    for_each_child(cursor, |c| generator.convert_node(c, into))
}

fn convert_node_verse(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
    let captures = VERSE_NUM_CAP_QUERY.captures(cursor.node(), generator.source);
    let Ok(Some(verse_num)) = generator.parse_from_query(&captures, "vnum", "verse number/range")
    else {
        return;
    };
    let alt_number = generator
        .parse_from_query(&captures, "alt-num", "verse number/range")
        .unwrap_or(None);

    let content = UsjContent::Verse {
        marker: MustBeStr,
        number: verse_num,
        alt_number,
        pub_number: captures.get("pub-num").map(|(_, x)| x.trim().to_string()),
        sid: if let Some(current_book) = generator.current_book
            && let Some(current_chapter) = generator.current_chapter
        {
            format!("{} {current_chapter}:{verse_num}", current_book.usfm_id())
        } else {
            generator.error(cursor.node(), "\\v outside of book or chapter");
            format!(
                "{} {}:{verse_num}",
                // Ugly fallbacks, but they're what we have available
                generator.current_book.unwrap_or(Book::Genesis).usfm_id(),
                generator.current_chapter.unwrap_or(nz_u8!(1))
            )
        },
    };
    generator.try_push_usj(cursor, into, "Unexpected \\v under", content);
}

fn convert_node_id(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    let captures = ID_QUERY.captures(cursor.node(), generator.source);
    let Ok(Some(book)) = generator.parse_from_query(&captures, "book-code", "book code") else {
        return;
    };
    let desc = captures
        .get("desc")
        .map(|(_, x)| x.trim())
        .take_if(|x| !x.is_empty());

    generator.current_book = Some(book);
    generator.try_push_usj(
        cursor,
        into,
        "Unexpected \\id under",
        UsjContent::Book {
            marker: MustBeStr,
            code: book,
            content: desc.into_iter().map(str::to_string).collect(),
        },
    );
}

fn convert_node_chapter(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
    for_each_child(cursor, |cursor| {
        let node = cursor.node();
        if node.kind() != "c" {
            return generator.convert_node(cursor, into);
        }

        let captures = CHAPTER_QUERY.captures(node, generator.source);
        let Ok(Some(chapter_num)) = generator.parse_from_query(&captures, "cnum", "chapter number")
        else {
            return;
        };
        let alt_number = generator
            .parse_from_query(&captures, "alt-num", "chapter number")
            .unwrap_or(None);

        generator.current_chapter = Some(chapter_num);
        let content = UsjContent::Chapter {
            marker: MustBeStr,
            number: chapter_num,
            alt_number,
            pub_number: captures.get("pub-num").map(|(_, x)| x.trim().to_string()),
            sid: if let Some(current_book) = generator.current_book {
                format!("{} {chapter_num}", current_book.usfm_id())
            } else {
                generator.error(cursor.node(), "\\v outside of book or chapter");
                format!(
                    // Ugly fallback, but it's what we have available
                    "{} {chapter_num}",
                    generator.current_book.unwrap_or(Book::Genesis).usfm_id(),
                )
            },
        };
        generator.try_push_usj(cursor, into, "Unexpected \\c under", content);

        for_each_child(cursor, |c| generator.convert_node(c, into));
    });
}

fn convert_node_para(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    let node = cursor.node();
    if node.child(0).is_some_and(|x| x.kind().ends_with("Block")) {
        return for_each_child(cursor, |c| convert_node_para(generator, c, into));
    }
    let para = match node.kind() {
        "paragraph" => {
            if !cursor.goto_first_child() {
                return generator.diagnostic(node, Severity::Warning, "Empty \\p");
            }
            let para_node = cursor.node();

            let para_marker = para_node.kind();
            let mut para = UsjContent::Paragraph {
                marker: para_marker.to_string(),
                content: vec![],
            };
            if para_marker.ends_with("Block") {
                cursor.goto_parent();
                return;
            }
            if para_marker != "b" {
                loop {
                    generator.convert_node(cursor, &mut para);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            cursor.goto_parent();
            para
        }
        "pi" | "ph" => {
            if !cursor.goto_first_child() {
                return generator.error(node, format!("\\{} missing marker", node.kind()));
            }
            let mut para = UsjContent::Paragraph {
                marker: generator.source[cursor.node().byte_range()].to_string(),
                content: vec![],
            };
            while cursor.goto_next_sibling() {
                generator.convert_node(cursor, &mut para);
            }
            cursor.goto_parent();
            para
        }
        unknown => {
            return generator.diagnostic(
                node,
                Severity::Warning,
                format!("Unknown para block type {unknown}"),
            );
        }
    };
    generator.try_push_usj(cursor, into, "Unexpected \\p under", para);
}

fn convert_node_generic(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
    let node = cursor.node();
    cursor.goto_first_child();
    let mut style = Cow::Borrowed(
        generator.source[cursor.node().byte_range()]
            .strip_prefix('\\')
            .unwrap_or_else(|| node.kind()),
    );

    if cursor.goto_next_sibling() {
        if cursor.node().kind().starts_with("numbered") {
            style += &generator.source[cursor.node().byte_range()];
        } else {
            cursor.goto_previous_sibling();
        }
    }

    let mut para = UsjContent::Paragraph {
        marker: style.trim().to_string(),
        content: vec![],
    };

    while cursor.goto_next_sibling() {
        match cursor.node().kind() {
            "add" | "bk" | "dc" | "ior" | "iqt" | "k" | "litl" | "nd" | "ord" |
            "pn" | "png" | "qac" | "qs" | "qt" | "rq" | "sig" | "sls" | "tl" | "wj" | // Special-text
            "em" | "bd" | "bdit" | "it" | "no" | "sc" | "sup" | // character styling
            "rb" | "pro" | "w" | "wh" | "wa" | "wg" | // special-features
            "lik" | "liv" | // structred list entries
            "jmp" | "fr" | "ft" | "fk" | "fq" | "fqa" | "fl" | "fw" | "fp" | "fv" | "fdc" | // footnote-content
            "xo" | "xop" | "xt" | "xta" | "xk" | "xq" | "xot" | "xnt" | "xdc" | // crossref-content
            "addNested" | "bkNested" | "dcNested" | "iorNested" | "iqtNested" | "kNested" | "litlNested" | "ndNested" | "ordNested" |
            "pnNested" | "pngNested" | "qacNested" | "qsNested" | "qtNested" | "rqNested" | "sigNested" | "slsNested" | "tlNested" | "wjNested" | // Special-text
            "emNested" | "bdNested" | "bditNested" | "itNested" | "noNested" | "scNested" | "supNested" | // character styling
            "rbNested" | "proNested" | "wNested" | "whNested" | "waNested" | "wgNested" | // special-features
            "likNested" | "livNested" | // structred list entries
            "jmpNested" | "frNested" | "ftNested" | "fkNested" | "fqNested" | "fqaNested" | "flNested" | "fwNested" | "fpNested" | "fvNested" | "fdcNested" | // footnote-content
            "xoNested" | "xopNested" | "xtNested" | "xtaNested" | "xkNested" | "xqNested" | "xotNested" | "xntNested" | "xdcNested" | // crossref-content
            "text" | "footnote" | "crossref" | "verseText" | "v" | "b" | "milestone" | "zNameSpace" => {
                generator.convert_node(cursor, &mut para);
            }
            _ => {
                generator.convert_node(cursor, into);
            }
        }
    }

    cursor.goto_parent();
}

fn convert_node_table(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
    let mut table = UsjContent::Table { content: vec![] };
    for_each_child(cursor, |c| generator.convert_node(c, &mut table));
    generator.try_push_usj(cursor, into, "Unexpected table under", table);
}

fn convert_node_tr(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    let mut row = UsjContent::TableRow {
        marker: MustBeStr,
        content: vec![],
    };
    if cursor.goto_first_child() {
        while cursor.goto_next_sibling() {
            generator.convert_node(cursor, &mut row);
        }
    }
    generator.try_push_usj(cursor, into, "Unexpected \\tr under", row);
}

fn convert_node_table_cell(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
    if !cursor.goto_first_child() {
        return generator.error(cursor.node(), "Missing content in \\tr");
    }

    let style = generator.source[cursor.node().byte_range()]
        .trim()
        .trim_start_matches('\\');
    let mut cell = UsjContent::TableCell {
        marker: style.to_string(),
        content: vec![],
        align: if style.ends_with('r') {
            TableCellAlignment::End
        } else if style.contains("tcc") {
            TableCellAlignment::Center
        } else {
            TableCellAlignment::Start
        },
    };
    while cursor.goto_next_sibling() {
        generator.convert_node(cursor, &mut cell);
    }

    cursor.goto_parent();
}

fn convert_node_milestone(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
}
fn convert_node_special(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
}
fn convert_node_notes(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
}
fn convert_node_char(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
}

usfm_queries! {
    static ATTRIB_VAL_QUERY = "((attributeValue) @attrib-val)";
    static CHAPTER_QUERY = r#"
        (c
            (chapterNumber) @cnum
            (ca (chapterNumber) @alt-num)?
            (cp (text) @pub-num)?
        )
    "#;
    static ID_QUERY = "(id (bookcode) @book-code (description)? @desc)";
    static VERSE_NUM_CAP_QUERY = r#"
        (v
            (verseNumber) @vnum
            (va (verseNumber) @alt-num)?
            (vp (text) @pub-num)?
        )
    "#;
}

const DISPATCH_MAP: phf::Map<&str, fn(&mut UsjGenerator, &mut TreeCursor, &mut UsjContent)> = phf::phf_map!(
    "text" => push_text_node,
    "verseText" => handle_verse_text,
    "v" => convert_node_verse,
    "id" => convert_node_id,
    "chapter" => convert_node_chapter,
    "paragraph" => convert_node_para,
    "cp" | "vp" => convert_node_generic,
    "table" => convert_node_table,
    "tr" => convert_node_tr,
    "milestone" | "zNameSpace" => convert_node_milestone,
    "esb" | "cat" | "fig" | "ref" => convert_node_special,
    "f" | "fe" | "ef" | "efe" | "x" | "ex" => convert_node_notes,
    "add" | "bk" | "dc" | "ior" | "iqt" | "k" | "litl" | "nd" | "ord" |
        "pn" | "png" | "qac" | "qs" | "qt" | "rq" | "sig" | "sls" | "tl" | "wj" | // Special-text
        "em" | "bd" | "bdit" | "it" | "no" | "sc" | "sup" | // character styling
        "rb" | "pro" | "w" | "wh" | "wa" | "wg" | // special-features
        "lik" | "liv" | // structred list entries
        "jmp" | "fr" | "ft" | "fk" | "fq" | "fqa" | "fl" | "fw" | "fp" | "fv" | "fdc" | // footnote-content
        "xo" | "xop" | "xt" | "xta" | "xk" | "xq" | "xot" | "xnt" | "xdc" | // crossref-content
        "addNested" | "bkNested" | "dcNested" | "iorNested" | "iqtNested" | "kNested" | "litlNested" | "ndNested" | "ordNested" |
        "pnNested" | "pngNested" | "qacNested" | "qsNested" | "qtNested" | "rqNested" | "sigNested" | "slsNested" | "tlNested" | "wjNested" | // Special-text
        "emNested" | "bdNested" | "bditNested" | "itNested" | "noNested" | "scNested" | "supNested" | // character styling
        "rbNested" | "proNested" | "wNested" | "whNested" | "waNested" | "wgNested" | // special-features
        "likNested" | "livNested" | // structred list entries
        "jmpNested" | "frNested" | "ftNested" | "fkNested" | "fqNested" | "fqaNested" | "flNested" | "fwNested" | "fpNested" | "fvNested" | "fdcNested" | // footnote-content
        "xoNested" | "xopNested" | "xtNested" | "xtaNested" | "xkNested" | "xqNested" | "xotNested" | "xntNested" | "xdcNested" | // crossref-content
        "xt_standalone" => convert_node_char,
    "tc" | "th" | "tcr" | "thr" | "tcc" => convert_node_table_cell,
    "ide" | "h" | "toc" | "toca" | // identification
        "imt" | "is" | "ip" | "ipi" | "im" | "imi" | "ipq" | "imq" |
        "ipr" | "iq" | "ib" | "ili" | "iot" | "io" | "iex" | "imte" | "ie" | // intro
        "mt" | "mte" | "cl" | "cd" | "ms" | "mr" | "s" | "sr" | "r" | "d" | "sp" | "sd" | // titles
        "q" | "qr" | "qc" | "qa" | "qm" | "qd" | // poetry
        "lh" | "li" | "lf" | "lim" | // lists
        "sts" | "rem" | "lit" | "restore" | // comments
        "b" => convert_node_generic,
    "usfm" => |_, _, _| {},
);

const DEFAULT_ATTRIB_MAP: phf::Map<&str, &str> = phf::phf_map!(
    "w" => "lemma",
    "rb" => "gloss",
    "xt" => "href",
    "fig" => "alt",
    "xt_standalone" => "href",
    "xtNested" => "href",
    "ref" => "loc",
    "milestone" => "who",
    "k" => "key",
);
