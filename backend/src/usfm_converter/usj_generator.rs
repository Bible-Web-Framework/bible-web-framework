use crate::book_data::Book;
use crate::usfm_queries;
use crate::usj::{ParaContent, UsjContent, UsjContentValue, UsjRoot};
use miette::{LabeledSpan, MietteDiagnostic};
use monostate::MustBeStr;
use std::num::NonZeroU8;
use tree_sitter::TreeCursor;

pub struct UsjGenerator<'a> {
    pub source: &'a str,
    pub errors: Vec<MietteDiagnostic>,
    book_slug: Option<Book>,
    current_chapter: Option<NonZeroU8>,
}

impl<'a> UsjGenerator<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            errors: vec![],
            book_slug: None,
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

    fn convert_node_attrib(&self, cursor: &mut TreeCursor, into: &mut UsjContent) {
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
            .map_or("", |(_, x)| x.trim());

        into.attributes
            .insert(attrib_name.to_string(), attrib_value.to_string());
    }

    fn unexpected_under(&mut self, cursor: &mut TreeCursor, into: &mut UsjContent, what: &str) {
        self.errors.push(
            MietteDiagnostic::new(format!(
                "Unexpected {what} under {}",
                into.marker().unwrap_or_default()
            ))
            .with_label(LabeledSpan::new_with_span(None, cursor.node().byte_range())),
        );
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
    let text_val = generator.source[cursor.node().byte_range()].to_string();
    match &mut into.value {
        UsjContentValue::Paragraph { content, .. } => content.push(ParaContent::Plain(text_val)),
        UsjContentValue::Book { content, .. } => content.push(text_val),
        _ => generator.unexpected_under(cursor, into, "plaintext"),
    }
}

fn handle_verse_text(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    for_each_child(cursor, |c| generator.convert_node(c, into))
}

fn convert_node_verse(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
    // let verse_num_cap = VERSE_NUM_CAP_QUERY.captures(cursor.node(), generator.source);
    // let Some((_, verse_num)) = verse_num_cap.get("vnum") else {
    //     return;
    // };
    //
    // let mut verse_obj = UsjContent::new(UsjContentValue::Verse {
    // });
}

fn convert_node_id(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
    let id_captures = ID_QUERY.captures(cursor.node(), generator.source);
    let book = match id_captures
        .get("book-code")
        .map(|x| Book::parse(x.1, None).ok_or(x))
    {
        Some(Ok(book)) => book,
        Some(Err((error_node, code))) => {
            generator.errors.push(
                MietteDiagnostic::new(format!("Unknown book code \"{code}\""))
                    .with_label(LabeledSpan::new_with_span(None, error_node.byte_range())),
            );
            return;
        }
        None => {
            generator.errors.push(
                MietteDiagnostic::new("Missing book code in \\id")
                    .with_label(LabeledSpan::new_with_span(None, cursor.node().byte_range())),
            );
            return;
        }
    };
    let desc = id_captures
        .get("desc")
        .map(|(_, x)| x.trim())
        .take_if(|x| !x.is_empty());

    generator.book_slug = Some(book);
    if let UsjContent {
        value: UsjContentValue::Root(UsjRoot { content, .. }),
        ..
    } = into
    {
        content.push(UsjContent::new(UsjContentValue::Book {
            marker: MustBeStr,
            code: book,
            content: desc.into_iter().map(str::to_string).collect(),
        }));
    } else {
        generator.unexpected_under(cursor, into, "\\id");
    }
}

fn convert_node_chapter(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
}
fn convert_node_para(generator: &mut UsjGenerator, cursor: &mut TreeCursor, into: &mut UsjContent) {
}
fn convert_node_generic(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
}
fn convert_node_ca_va(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
}
fn convert_node_table(
    generator: &mut UsjGenerator,
    cursor: &mut TreeCursor,
    into: &mut UsjContent,
) {
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
    static ID_QUERY = "(id (bookcode) @book-code (description)? @desc)";
    static ATTRIB_VAL_QUERY = "((attributeValue) @attrib-val)";
    static VERSE_NUM_CAP_QUERY = r#"
        (v
            (verseNumber) @vnum
            (va (verseNumber) @alt)?
            (vp (text) @vp)?
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
    "ca" | "va" => convert_node_ca_va,
    "table" | "tr" => convert_node_table,
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
    "tc" | "th" | "tcr" | "thr" | "tcc" => convert_node_table,
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
