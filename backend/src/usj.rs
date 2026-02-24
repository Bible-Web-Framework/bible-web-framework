use crate::bible_data::BibleDataError;
use crate::book_data::Book;
use crate::usfm_converter::UsfmParser;
use crate::utils::option_as_vec;
use crate::verse_range::VerseRange;
use crate::{nz_u8, serde_display_and_parse};
use either::Either;
use ere::compile_regex;
use itertools::Itertools;
use miette::{LabeledSpan, MietteDiagnostic, Severity};
use monostate::{MustBe, MustBeStr};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io::BufRead;
use std::num::NonZeroU8;
use std::slice::SliceIndex;
use std::str::FromStr;
use usfm3::ast::{Attribute, Node};
use usfm3::diagnostics::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsjBookInfo {
    pub book: Book,
    pub description: Option<String>,
}

impl Display for UsjBookInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(description) = &self.description {
            f.write_fmt(format_args!("{} ({})", self.book, description))
        } else {
            self.book.fmt(f)
        }
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum UsjContent {
    #[serde(rename = "USJ")]
    Root(UsjRoot),

    #[serde(rename = "para")]
    Paragraph {
        marker: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<ParaContent>,
    },

    #[serde(rename = "char")]
    Character {
        marker: String,
        content: Vec<ParaContent>,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    Book {
        marker: MustBe!("id"),
        #[serde(with = "option_as_vec")]
        content: Option<String>,
        code: Book,
    },

    Chapter {
        marker: MustBe!("c"),
        #[serde_as(as = "DisplayFromStr")]
        number: NonZeroU8,
        #[serde(rename = "altnumber")]
        alt_number: Option<NonZeroU8>,
        #[serde(rename = "pubnumber")]
        pub_number: Option<String>,
        sid: String,
    },

    Verse {
        marker: MustBe!("v"),
        number: VerseRange,
        #[serde(rename = "altnumber")]
        alt_number: Option<VerseRange>,
        #[serde(rename = "pubnumber")]
        pub_number: Option<String>,
        sid: String,
    },

    #[serde(rename = "ms")]
    Milestone {
        marker: String,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    Note {
        marker: String,
        content: Vec<ParaContent>,
        caller: NoteCaller,
        category: Option<String>,
    },

    Table {
        content: Vec<UsjContent>,
    },

    #[serde(rename = "table:row")]
    TableRow {
        marker: MustBe!("tr"),
        content: Vec<UsjContent>,
    },

    #[serde(rename = "table:cell")]
    TableCell {
        marker: String,
        content: Vec<ParaContent>,
        align: TableCellAlignment,
    },

    Sidebar {
        marker: MustBe!("esb"),
        content: Vec<UsjContent>,
        category: Option<String>,
    },

    Figure {
        marker: MustBe!("fig"),
        #[serde(with = "option_as_vec")]
        content: Option<String>,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    #[serde(rename = "ref")]
    Reference {
        #[serde(with = "option_as_vec")]
        content: Option<String>,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    OptBreak,
    // TODO: \periph
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParaContent {
    Usj(UsjContent),
    Plain(String),
}

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableCellAlignment {
    #[default]
    Start,
    Center,
    End,
}

serde_display_and_parse!(TableCellAlignment);

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum NoteCaller {
    #[serde(rename = "+")]
    #[default]
    Generated,
    #[serde(rename = "-")]
    None,
    #[serde(untagged)]
    Other(char),
}

serde_display_and_parse!(NoteCaller);

pub type AttributesMap = BTreeMap<String, String>;

impl UsjContent {
    pub fn as_root(&self) -> Option<&UsjRoot> {
        if let Self::Root(root) = &self {
            Some(root)
        } else {
            None
        }
    }

    pub fn unwrap_root(&self) -> &UsjRoot {
        self.as_root()
            .expect("unwrap_root() called on a non-Root UsjContent")
    }

    pub fn marker(&self) -> Option<&str> {
        #[inline(always)]
        const fn get_value<T: MustBe>(_: &T) -> <T as MustBe>::Type {
            <T as MustBe>::VALUE
        }
        match &self {
            Self::Root(_) => None,
            Self::Paragraph { marker, .. } => Some(marker),
            Self::Character { marker, .. } => Some(marker),
            Self::Book { marker, .. } => Some(get_value(marker)),
            Self::Chapter { marker, .. } => Some(get_value(marker)),
            Self::Verse { marker, .. } => Some(get_value(marker)),
            Self::Milestone { marker, .. } => Some(marker),
            Self::Note { marker, .. } => Some(marker),
            Self::Table { .. } => None,
            Self::TableRow { marker, .. } => Some(get_value(marker)),
            Self::TableCell { marker, .. } => Some(marker),
            Self::Sidebar { marker, .. } => Some(get_value(marker)),
            Self::Figure { marker, .. } => Some(get_value(marker)),
            Self::Reference { .. } => None,
            Self::OptBreak => None,
        }
    }

    pub fn marker_or_type(&self) -> &str {
        self.marker().unwrap_or_else(|| match self {
            Self::Root(_) => "USJ",
            Self::Table { .. } => "table",
            Self::Reference { .. } => "ref",
            _ => unreachable!("All other variants should be handled by marker()"),
        })
    }

    pub fn push_text_content(&mut self, text: String) -> bool {
        match self {
            UsjContent::Paragraph { content, .. }
            | UsjContent::Character { content, .. }
            | UsjContent::Note { content, .. }
            | UsjContent::TableCell { content, .. } => content.push(ParaContent::Plain(text)),

            UsjContent::Book { content, .. }
            | UsjContent::Figure { content, .. }
            | UsjContent::Reference { content, .. }
                if content.is_none() =>
            {
                *content = Some(text)
            }

            _ => return false,
        }
        true
    }

    pub fn push_usj_content(&mut self, new_content: UsjContent) -> bool {
        match self {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. } => content.push(new_content),

            UsjContent::Paragraph { content, .. }
            | UsjContent::Character { content, .. }
            | UsjContent::Note { content, .. }
            | UsjContent::TableCell { content, .. } => content.push(ParaContent::Usj(new_content)),

            _ => return false,
        }
        true
    }

    pub fn insert_usj_content(&mut self, index: usize, new_content: UsjContent) -> bool {
        match self {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. } => content.insert(index, new_content),

            UsjContent::Paragraph { content, .. }
            | UsjContent::Character { content, .. }
            | UsjContent::Note { content, .. }
            | UsjContent::TableCell { content, .. } => {
                content.insert(index, ParaContent::Usj(new_content))
            }

            _ => return false,
        }
        true
    }

    pub fn get_content(&self, index: usize) -> Option<Either<&UsjContent, &str>> {
        Some(match self {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. }
            | UsjContent::Sidebar { content, .. } => Either::Left(content.get(index)?),

            UsjContent::Paragraph { content, .. }
            | UsjContent::Character { content, .. }
            | UsjContent::Note { content, .. }
            | UsjContent::TableCell { content, .. } => match content.get(index)? {
                ParaContent::Usj(usj) => Either::Left(usj),
                ParaContent::Plain(text) => Either::Right(text),
            },

            UsjContent::Book { content, .. }
            | UsjContent::Figure { content, .. }
            | UsjContent::Reference { content, .. } => {
                if index == 0 {
                    Either::Right(content.as_ref()?)
                } else {
                    return None;
                }
            }

            _ => return None,
        })
    }

    pub fn get_content_mut(
        &mut self,
        index: usize,
    ) -> Option<Either<&mut UsjContent, &mut String>> {
        Some(match self {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. }
            | UsjContent::Sidebar { content, .. } => Either::Left(content.get_mut(index)?),

            UsjContent::Paragraph { content, .. }
            | UsjContent::Character { content, .. }
            | UsjContent::Note { content, .. }
            | UsjContent::TableCell { content, .. } => match content.get_mut(index)? {
                ParaContent::Usj(usj) => Either::Left(usj),
                ParaContent::Plain(text) => Either::Right(text),
            },

            UsjContent::Book { content, .. }
            | UsjContent::Figure { content, .. }
            | UsjContent::Reference { content, .. } => {
                if index == 0 {
                    Either::Right(content.as_mut()?)
                } else {
                    return None;
                }
            }

            _ => return None,
        })
    }

    pub fn attributes_mut(&mut self) -> Option<&mut AttributesMap> {
        match self {
            Self::Character { attributes, .. }
            | Self::Milestone { attributes, .. }
            | Self::Figure { attributes, .. }
            | Self::Reference { attributes, .. } => Some(attributes),
            _ => None,
        }
    }

    pub fn category_mut(&mut self) -> Option<&mut Option<String>> {
        match self {
            Self::Note { category, .. } | Self::Sidebar { category, .. } => Some(category),
            _ => None,
        }
    }

    fn as_para_content(&self) -> Option<&Vec<ParaContent>> {
        if let Self::Paragraph { content, .. } = &self {
            Some(content)
        } else {
            None
        }
    }

    pub fn is_title_para(&self) -> bool {
        matches!(&self, Self::Paragraph { marker, .. } if is_title_marker(marker))
    }
}

pub fn is_title_marker(marker: &str) -> bool {
    const REGEX: ere::Regex<2> =
        compile_regex!("^(mt[1-9]?|mte[1-9]?|ms[1-9]?|mr|s[1-9]?|sr|r|d|sp|sd[1-9]?)$");
    REGEX.test(marker)
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct UsjRoot {
    pub version: String,
    pub content: Vec<UsjContent>,
}

type ParaIndex = (usize, usize);

impl UsjRoot {
    pub fn book_info(&self) -> Option<UsjBookInfo> {
        self.content.iter().find_map(|content| {
            if let UsjContent::Book { code, content, .. } = &content {
                Some(UsjBookInfo {
                    book: *code,
                    description: content.clone(),
                })
            } else {
                None
            }
        })
    }

    pub fn translated_book_name(&self) -> Option<&str> {
        self.content
            .iter()
            .take_while(|x| !matches!(x, UsjContent::Chapter { .. }))
            .find_map(|x| {
                if let UsjContent::Paragraph { marker, content } = x
                    && (marker == "h" || marker == "h1")
                    && let [ParaContent::Plain(text)] = &content[..]
                {
                    Some(text.trim())
                } else {
                    None
                }
            })
    }

    pub fn find_reference(
        &self,
        chapter: NonZeroU8,
        verse_range: VerseRange,
    ) -> Option<Vec<UsjContent>> {
        let chapter_start = self.find_chapter_start(chapter)?;

        let (start, base_chapter_label) = if verse_range.first_u8() == 1 {
            (chapter_start, self.find_chapter_label())
        } else {
            let after_chapter_start = self.next_para_index(chapter_start)?;
            let (verse_start, start_range) =
                self.find_verse_start_para(verse_range.first(), after_chapter_start)?;
            if start_range.first_u8() == 1 {
                (chapter_start, self.find_chapter_label())
            } else {
                (verse_start, None)
            }
        };
        let end = self
            .find_verse_start_para(
                verse_range.last().saturating_add(1),
                self.next_para_index(start)?,
            )
            .and_then(|(index, range)| {
                Some(if range.first_u8() == verse_range.last_u8() + 1 {
                    index
                } else {
                    self.find_verse_start_para(
                        range.last().saturating_add(1),
                        self.next_para_index(index)?,
                    )?
                    .0
                })
            })
            .or_else(|| self.find_chapter_start(chapter.saturating_add(1)));

        let mut result = self.slice_para(start, end);
        if let Some(label) = base_chapter_label {
            result.insert(0, label);
        }
        Some(result)
    }

    fn find_chapter_label(&self) -> Option<UsjContent> {
        self.content
            .iter()
            .take_while(|x| !matches!(&x, UsjContent::Chapter { .. }))
            .find(|x| {
                if let UsjContent::Paragraph {
                    marker, content, ..
                } = &x
                    && marker == "cl"
                    && let &[ParaContent::Plain(_)] = &content.as_slice()
                {
                    true
                } else {
                    false
                }
            })
            .cloned()
    }

    fn find_chapter_start(&self, chapter: NonZeroU8) -> Option<ParaIndex> {
        let chapter_index = self
            .content
            .iter()
            .position(|x| matches!(&x, UsjContent::Chapter { number, .. } if *number == chapter))?;
        Some((chapter_index, 0))
    }

    fn next_para_index(&self, index: ParaIndex) -> Option<ParaIndex> {
        if let Some(para_content) = self
            .content
            .get(index.0)
            .and_then(UsjContent::as_para_content)
            && index.1 + 1 < para_content.len()
        {
            Some((index.0, index.1 + 1))
        } else {
            (index.0 + 1 < self.content.len()).then_some((index.0 + 1, 0))
        }
    }

    fn prev_para_index(&self, index: ParaIndex) -> Option<ParaIndex> {
        if index.1 > 0 {
            Some((index.0, index.1 - 1))
        } else if index.0 > 0 {
            let mut prev_index = index.0 - 1;
            loop {
                if let Some(para_content) = self
                    .content
                    .get(prev_index)
                    .and_then(UsjContent::as_para_content)
                {
                    if para_content.is_empty() {
                        if prev_index == 0 {
                            return None;
                        }
                        prev_index -= 1;
                        continue;
                    }
                    break Some((prev_index, para_content.len() - 1));
                } else {
                    break Some((prev_index, 0));
                }
            }
        } else {
            None
        }
    }

    fn find_verse_start_para(
        &self,
        verse: NonZeroU8,
        start: ParaIndex,
    ) -> Option<(ParaIndex, VerseRange)> {
        let (start_root, mut start_inner) = start;
        let (mut verse_start, verse_range) = self
            .content
            .iter()
            .enumerate()
            .skip(start_root)
            .take_while(|(_, element)| !matches!(&element, UsjContent::Chapter { .. }))
            .find_map(|(root_index, element)| {
                let content = element.as_para_content()?;
                let skip = std::mem::take(&mut start_inner);
                content
                    .iter()
                    .enumerate()
                    .skip(skip)
                    .find_map(|(index, content)| {
                        if let ParaContent::Usj(UsjContent::Verse { number: range, .. }) = content
                            && range.contains(verse)
                        {
                            Some(((root_index, index), *range))
                        } else {
                            None
                        }
                    })
            })?;
        loop {
            let Some(prev_index) = self.prev_para_index(verse_start) else {
                break;
            };
            if prev_index.1 > 0 || !self.content[prev_index.0].is_title_para() {
                break;
            }
            verse_start = prev_index;
        }
        Some((verse_start, verse_range))
    }

    fn slice_para(&self, start: ParaIndex, end: Option<ParaIndex>) -> Vec<UsjContent> {
        if let Some(end) = end
            && start.0 == end.0
        {
            return vec![self.slice_single_para(start.0, start.1..end.1)];
        }

        let mut result = Vec::new();
        result.push(self.slice_single_para(start.0, start.1..));
        if let Some(end) = end {
            result.extend_from_slice(&self.content[start.0 + 1..end.0]);
            if end.1 > 0 {
                result.push(self.slice_single_para(end.0, ..end.1));
            }
        } else {
            result.extend_from_slice(&self.content[start.0 + 1..]);
        }
        result
    }

    fn slice_single_para(
        &self,
        index: usize,
        sub_index: impl SliceIndex<[ParaContent], Output = [ParaContent]>,
    ) -> UsjContent {
        match &self.content[index] {
            UsjContent::Paragraph { marker, content } => UsjContent::Paragraph {
                content: Vec::from(&content[sub_index]),
                marker: marker.to_string(),
            },
            element => element.clone(),
        }
    }
}

// impl TryFrom<Document> for UsjContent {
//     type Error = MietteDiagnostic;
//
//     fn try_from(value: Document) -> Result<Self, Self::Error> {
//         UsjContent::Root(UsjRoot {
//             version: "3.1".to_string(),
//             content: value.content.into_iter().map(Into::into).collect(),
//         })
//     }
// }
//
// impl TryFrom<Node> for UsjContent {
//     fn from(value: Node) -> Self {
//         match value {
//             Node::Book { marker, code, content, span } => UsjContent::Book {
//                 marker: MustBeStr,
//                 code: code.into(),
//             }
//         }
//     }
// }

pub fn load_usj(reader: impl BufRead) -> Result<UsjContent, BibleDataError> {
    Ok(serde_json::from_reader(reader)?)
}

#[derive(Debug)]
pub struct LoadedUsjFromUsfm {
    pub usj: UsjContent,
    pub source: String,
    pub diagnostics: Vec<MietteDiagnostic>,
}

pub fn load_usj_from_usfm(content: String) -> Result<LoadedUsjFromUsfm, BibleDataError> {
    let parser = UsfmParser::new(content)?;

    let (usj, conversion_diags) = parser.to_usj();
    let mut all_diags = parser.diagnostics;
    all_diags.extend(conversion_diags);

    Ok(LoadedUsjFromUsfm {
        usj,
        source: parser.usfm,
        diagnostics: all_diags,
    })
}

fn usj_from_usfm(node: Node, diags: &mut Vec<MietteDiagnostic>) -> (UsjContent, Option<Span>) {
    match para_from_usfm(node, diags) {
        (ParaContent::Usj(usj), span) => (usj, span),
        (ParaContent::Plain(text), span) => {
            diags.push(MietteDiagnostic::new("Unexpected plain-text"));
            (
                UsjContent::Paragraph {
                    marker: "p".to_string(),
                    content: vec![ParaContent::Plain(text)],
                },
                span,
            )
        }
    }
}

fn para_from_usfm(node: Node, diags: &mut Vec<MietteDiagnostic>) -> (ParaContent, Option<Span>) {
    match node {
        Node::Book {
            marker,
            code,
            content,
            span,
        } => {
            #[allow(clippy::question_mark)]
            const PROPER_BOOK_REGEX: ere::Regex = compile_regex!("^[A-Z0-9][A-Z][A-Z]$");
            if !marker.is_ascii() {
                diags.push(
                    MietteDiagnostic::new("Non-standard USFM book code")
                        .with_severity(Severity::Warning)
                        .with_label(LabeledSpan::at(span.clone(), "Should be ASCII")),
                );
            } else if !PROPER_BOOK_REGEX.test(&marker) {
                diags.push(
                    MietteDiagnostic::new("Non-standard USFM book code")
                        .with_severity(Severity::Warning)
                        .with_label(LabeledSpan::at(
                            span.clone(),
                            format!(
                                "Should be 3-characters uppercase ({})",
                                &marker.to_ascii_uppercase()[..3]
                            ),
                        )),
                );
            }
            (
                ParaContent::Usj(UsjContent::Book {
                    marker: MustBeStr,
                    code: parse_string(code, span.clone(), "book code", diags),
                    content: option_string_from_usfm(content, diags),
                }),
                Some(span),
            )
        }
        Node::Chapter {
            marker: _,
            number,
            sid,
            altnumber,
            pubnumber,
            span,
        } => (
            ParaContent::Usj(UsjContent::Chapter {
                marker: MustBeStr,
                number: parse_string_with_default(
                    number,
                    nz_u8!(1),
                    span.clone(),
                    "chapter number",
                    diags,
                ),
                alt_number: altnumber.map(|n| {
                    parse_string_with_default(
                        n,
                        nz_u8!(1),
                        span.clone(),
                        "alternate chapter number",
                        diags,
                    )
                }),
                pub_number: pubnumber,
                sid: sid.expect("Chapter sid not specified"),
            }),
            Some(span),
        ),
        Node::Verse {
            marker: _,
            number,
            sid,
            altnumber,
            pubnumber,
            span,
        } => (
            ParaContent::Usj(UsjContent::Verse {
                marker: MustBeStr,
                number: parse_string(number, span.clone(), "verse number", diags),
                alt_number: altnumber
                    .map(|n| parse_string(n, span.clone(), "alternate verse number", diags)),
                pub_number: pubnumber,
                sid: sid.expect("Verse sid not specified"),
            }),
            Some(span),
        ),
        Node::Para {
            marker,
            content,
            span,
        } => (
            ParaContent::Usj(UsjContent::Paragraph {
                marker,
                content: paras_from_usfm(content, diags),
            }),
            Some(span),
        ),
        Node::Char {
            marker,
            content,
            attributes,
            span,
        } => (
            ParaContent::Usj(UsjContent::Character {
                marker,
                content: paras_from_usfm(content, diags),
                attributes: parse_attributes(attributes),
            }),
            Some(span),
        ),
        Node::Note {
            marker,
            caller,
            category,
            content,
            span,
        } => (
            ParaContent::Usj(UsjContent::Note {
                marker,
                content: paras_from_usfm(content, diags),
                caller: parse_string(caller, span.clone(), "note caller", diags),
                category,
            }),
            Some(span),
        ),
        Node::Milestone {
            marker,
            attributes,
            span,
        } => (
            ParaContent::Usj(UsjContent::Milestone {
                marker,
                attributes: parse_attributes(attributes),
            }),
            Some(span),
        ),
        Node::Figure {
            marker: _,
            content,
            attributes,
            span,
        } => (
            ParaContent::Usj(UsjContent::Figure {
                marker: MustBeStr,
                content: option_string_from_usfm(content, diags),
                attributes: parse_attributes(attributes),
            }),
            Some(span),
        ),
        Node::Sidebar {
            marker: _,
            category,
            content,
            span,
        } => (
            ParaContent::Usj(UsjContent::Sidebar {
                marker: MustBeStr,
                content: usjs_from_usfm(content, diags),
                category,
            }),
            Some(span),
        ),
        Node::Periph { content, span, .. } => {
            // TODO: \periph
            diags.push(
                MietteDiagnostic::new("\\periph not yet implemented, treating as \\p")
                    .with_severity(Severity::Advice)
                    .with_label(LabeledSpan::new_with_span(None, span.clone())),
            );
            (
                ParaContent::Usj(UsjContent::Paragraph {
                    marker: "p".to_string(),
                    content: paras_from_usfm(content, diags),
                }),
                Some(span),
            )
        }
        Node::Table { content, span } => (
            ParaContent::Usj(UsjContent::Table {
                content: usjs_from_usfm(content, diags),
            }),
            Some(span),
        ),
        Node::TableRow {
            marker: _,
            content,
            span,
        } => (
            ParaContent::Usj(UsjContent::TableRow {
                marker: MustBeStr,
                content: usjs_from_usfm(content, diags),
            }),
            Some(span),
        ),
        Node::TableCell {
            marker,
            align,
            content,
            span,
        } => (
            ParaContent::Usj(UsjContent::TableCell {
                marker,
                content: paras_from_usfm(content, diags),
                align: parse_string(align, span.clone(), "table cell alignment", diags),
            }),
            Some(span),
        ),
        Node::Ref {
            content,
            attributes,
            span,
        } => (
            ParaContent::Usj(UsjContent::Reference {
                content: option_string_from_usfm(content, diags),
                attributes: parse_attributes(attributes),
            }),
            Some(span),
        ),
        Node::Unknown { content, span, .. } => {
            diags.push(
                MietteDiagnostic::new("Custom markers are not yet supported, treating as \\p")
                    .with_severity(Severity::Advice)
                    .with_label(LabeledSpan::new_with_span(None, span.clone())),
            );
            (
                ParaContent::Usj(UsjContent::Paragraph {
                    marker: "p".to_string(),
                    content: paras_from_usfm(content, diags),
                }),
                Some(span),
            )
        }
        Node::OptBreak => (ParaContent::Usj(UsjContent::OptBreak), None),
        Node::Text(text) => (ParaContent::Plain(text), None),
    }
}

fn paras_from_usfm(nodes: Vec<Node>, diags: &mut Vec<MietteDiagnostic>) -> Vec<ParaContent> {
    nodes
        .into_iter()
        .map(|node| para_from_usfm(node, diags).0)
        .collect()
}

fn usjs_from_usfm(nodes: Vec<Node>, diags: &mut Vec<MietteDiagnostic>) -> Vec<UsjContent> {
    nodes
        .into_iter()
        .map(|node| usj_from_usfm(node, diags).0)
        .collect()
}

fn option_string_from_usfm(nodes: Vec<Node>, diags: &mut Vec<MietteDiagnostic>) -> Option<String> {
    let mut paras = nodes
        .into_iter()
        .map(|node| para_from_usfm(node, diags))
        .collect_vec()
        .into_iter();
    let (para, span) = paras.next()?;
    let result = match para {
        ParaContent::Usj(_) if span.is_some() => {
            diags.push(
                MietteDiagnostic::new("Unexpected non-string content")
                    .with_label(LabeledSpan::new_with_span(None, span.unwrap())),
            );
            None
        }
        ParaContent::Usj(_) => {
            diags.push(MietteDiagnostic::new("Unexpected non-string content"));
            None
        }
        ParaContent::Plain(text) => Some(text),
    };
    let mut spans = paras.peekable();
    if spans.peek().is_some() {
        diags.push(
            MietteDiagnostic::new("Unexpected trailing data")
                .with_severity(Severity::Warning)
                .and_labels(
                    spans.filter_map(|(_, span)| span.map(|s| LabeledSpan::new_with_span(None, s))),
                ),
        )
    }
    result
}

fn parse_string<T>(str: String, span: Span, what: &str, diags: &mut Vec<MietteDiagnostic>) -> T
where
    T: FromStr + Default,
    T::Err: ToString,
{
    parse_string_with_default(str, T::default(), span, what, diags)
}

fn parse_string_with_default<T>(
    str: String,
    fallback: T,
    span: Span,
    what: &str,
    diags: &mut Vec<MietteDiagnostic>,
) -> T
where
    T: FromStr,
    T::Err: ToString,
{
    match str.parse() {
        Ok(value) => value,
        Err(err) => {
            diags.push(
                MietteDiagnostic::new(format!("Invalid {what}"))
                    .with_label(LabeledSpan::at(span, err.to_string())),
            );
            fallback
        }
    }
}

fn parse_attributes(attributes: Vec<Attribute>) -> AttributesMap {
    attributes
        .into_iter()
        .map(|attr| (attr.key, attr.value))
        .collect()
}

pub fn load_footnote_from_usfm(footnote: &str) -> Result<LoadedUsjFromUsfm, BibleDataError> {
    let mut base = load_usj_from_usfm(format!("\\id GEN\n\\c 1\n{footnote}"))?;
    base.usj = match base.usj {
        UsjContent::Root(root) => {
            if root.content.len() > 3 {
                return Err(BibleDataError::InjectedFootnoteLength(
                    root.content.len() - 2,
                ));
            }
            let element = root.content.into_iter().nth(2).unwrap();
            if !matches!(element, UsjContent::Note { .. }) {
                return Err(BibleDataError::InjectedFootnoteNotNote(
                    element.marker_or_type().to_string(),
                ));
            }
            element
        }
        _ => unreachable!(),
    };
    Ok(base)
}

#[cfg(test)]
mod test {
    use crate::bible_data::BibleDataError;
    use crate::usj::{AttributesMap, NoteCaller, ParaContent, UsjContent, load_footnote_from_usfm};
    use std::error::Error;

    #[test]
    fn test_load_footnote() -> Result<(), Box<dyn Error>> {
        let usfm = "\\f +\\ft Test footnote \\nd Lord\\nd*\\f*";
        let usj = UsjContent::Note {
            marker: "f".to_string(),
            caller: NoteCaller::Generated,
            category: None,
            content: vec![ParaContent::Usj(UsjContent::Character {
                marker: "ft".to_string(),
                content: vec![
                    ParaContent::Plain("Test footnote ".to_string()),
                    ParaContent::Usj(UsjContent::Character {
                        marker: "nd".to_string(),
                        content: vec![ParaContent::Plain("Lord".to_string())],
                        attributes: AttributesMap::default(),
                    }),
                ],
                attributes: AttributesMap::default(),
            })],
        };

        let converted_usj = load_footnote_from_usfm(usfm)?;
        assert!(
            converted_usj.diagnostics.is_empty(),
            "{:#?}",
            converted_usj.diagnostics
        );
        assert_eq!(converted_usj.usj, usj);

        Ok(())
    }

    #[test]
    fn test_load_footnote_extra_data() {
        let usfm = "\\f +\\ft Test footnote\\f*\n\\b\n\\p Hello";
        let usj = load_footnote_from_usfm(usfm);
        assert!(
            matches!(&usj, Err(BibleDataError::InjectedFootnoteLength(3))),
            "{usj:#?}",
        );
    }

    #[test]
    fn test_load_footnote_not_note() {
        let usfm = "\\p Hello, world!";
        let usj = load_footnote_from_usfm(usfm);
        assert!(
            matches!(&usj, Err(BibleDataError::InjectedFootnoteNotNote(marker)) if marker == "p"),
            "{usj:#?}",
        );
    }
}
