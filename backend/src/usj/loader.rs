use crate::bible_data::BibleDataError;
use crate::nz_u8;
use crate::usj::content::{AttributesMap, ParaContent, UsjContent};
use crate::usj::marker::{ContentMarker, MilestoneMarker, MilestoneSide, NoteMarker};
use crate::usj::root::UsjRoot;
use crate::utils::parsed_string_value::ParsedStringValue;
use crate::verse_range::VerseRange;
use itertools::Itertools;
use miette::{LabeledSpan, MietteDiagnostic, Severity};
use monostate::MustBeStr;
use smallvec::SmallVec;
use std::io::BufRead;
use std::str::FromStr;
use usfm3::ast::{Attribute, Node};
use usfm3::builder::parse;
use usfm3::diagnostics::Span;

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
    let parse_results = parse(&content);

    let mut diags = parse_results
        .diagnostics
        .into_inner()
        .into_iter()
        .map(|diag| {
            MietteDiagnostic::new(diag.message)
                .with_severity(match diag.severity {
                    usfm3::diagnostics::Severity::Info => Severity::Advice,
                    usfm3::diagnostics::Severity::Warning => Severity::Warning,
                    usfm3::diagnostics::Severity::Error => Severity::Error,
                })
                .with_label(LabeledSpan::new_with_span(None, diag.span))
                .with_code(format!("DiagnosticCode::{:?}", diag.code))
        })
        .collect();

    Ok(LoadedUsjFromUsfm {
        usj: UsjContent::Root(UsjRoot {
            version: "3.1".to_string(),
            content: usjs_from_usfm(parse_results.document.content, &mut diags),
        }),
        source: content,
        diagnostics: diags,
    })
}

pub fn load_footnote_from_usfm(footnote: String) -> Result<LoadedUsjFromUsfm, BibleDataError> {
    let mut base = load_usj_from_usfm(footnote)?;
    base.usj = match base.usj {
        UsjContent::Root(root) => {
            if root.content.len() > 1 {
                return Err(BibleDataError::InjectedFootnoteLength(root.content.len()));
            }
            let element = root.content.into_iter().next().unwrap();
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

fn usj_from_usfm(node: Node, diags: &mut Vec<MietteDiagnostic>) -> (UsjContent, Option<Span>) {
    match para_from_usfm(node, diags) {
        (ParaContent::Usj(usj), span) => (usj, span),
        (ParaContent::Plain(text), span) => {
            diags.push(MietteDiagnostic::new("Unexpected plain-text"));
            (
                UsjContent::Paragraph {
                    marker: ContentMarker::P(()),
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
            marker: _,
            code,
            content,
            span,
        } => {
            #[expect(clippy::question_mark)]
            const PROPER_BOOK_REGEX: ere::Regex = ere::compile_regex!("^[A-Z0-9][A-Z][A-Z]$");
            if !code.is_ascii() {
                diags.push(
                    MietteDiagnostic::new("Non-standard USFM book code")
                        .with_severity(Severity::Warning)
                        .with_label(LabeledSpan::at(span.clone(), "Should be ASCII")),
                );
            } else if !PROPER_BOOK_REGEX.test(&code) {
                diags.push(
                    MietteDiagnostic::new("Non-standard USFM book code")
                        .with_severity(Severity::Warning)
                        .with_label(LabeledSpan::at(
                            span.clone(),
                            format!(
                                "Should be 3-characters uppercase ({})",
                                &code.to_ascii_uppercase()[..3]
                            ),
                        )),
                );
            }
            (
                ParaContent::Usj(UsjContent::Book {
                    marker: MustBeStr,
                    code: parse_string(&code, span.clone(), "book code", "Genesis", diags),
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
                number: try_parse_string(&number, span.clone(), "chapter number", "1", diags)
                    .unwrap_or(ParsedStringValue {
                        value: nz_u8!(1),
                        string: number,
                    }),
                alt_number: altnumber,
                pub_number: pubnumber,
                sid: sid.unwrap_or_default(),
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
                number: try_parse_string(&number, span.clone(), "verse number", "1", diags)
                    .unwrap_or(ParsedStringValue {
                        value: const { VerseRange::new_single_verse(nz_u8!(1)) },
                        string: number,
                    }),
                alt_number: altnumber,
                pub_number: pubnumber,
                sid: sid.unwrap_or_default(),
            }),
            Some(span),
        ),
        Node::Para {
            marker,
            content,
            span,
        } => (
            ParaContent::Usj(UsjContent::Paragraph {
                marker: try_parse_string(&marker, span.clone(), "paragraph marker", "\\p", diags)
                    .unwrap_or(ContentMarker::P(())),
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
                marker: try_parse_string(&marker, span.clone(), "character marker", "\\no", diags)
                    .unwrap_or(ContentMarker::No(())),
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
                marker: try_parse_string(&marker, span.clone(), "note marker", "\\f", diags)
                    .unwrap_or(NoteMarker::F(())),
                content: paras_from_usfm(content, diags),
                caller: parse_string(&caller, span.clone(), "note caller", "+", diags),
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
                marker: try_parse_string(
                    &marker,
                    span.clone(),
                    "milestone marker",
                    "\\qt1-s",
                    diags,
                )
                .unwrap_or(MilestoneMarker::Qt((MilestoneSide::Start, 1))),
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
        Node::Periph {
            alt,
            content,
            attributes,
            span,
        } => (
            ParaContent::Usj(UsjContent::Periph {
                alt: alt.unwrap_or_else(|| {
                    diags.push(
                        MietteDiagnostic::new("Missing periph title")
                            .with_label(LabeledSpan::new_with_span(None, span.clone())),
                    );
                    "".to_string()
                }),
                content: usjs_from_usfm(content, diags),
                attributes: parse_attributes(attributes),
            }),
            Some(span),
        ),
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
                marker: try_parse_string(
                    &marker,
                    span.clone(),
                    "table cell marker",
                    "\\tc1",
                    diags,
                )
                .unwrap_or(ContentMarker::Tc((1, 1))),
                content: paras_from_usfm(content, diags),
                align: parse_string(&align, span.clone(), "table cell alignment", "start", diags),
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
        Node::Unknown { marker, span, .. } => {
            if marker.starts_with('z') {
                diags.push(
                    MietteDiagnostic::new("Custom markers are not yet supported, and are removed")
                        .with_severity(Severity::Error)
                        .with_label(LabeledSpan::new_with_span(None, span.clone())),
                );
            }
            (ParaContent::Plain("".to_string()), Some(span))
        }
        Node::OptBreak => (ParaContent::Usj(UsjContent::OptBreak), None),
        Node::Text(text) => (ParaContent::Plain(text), None),
    }
}

fn paras_from_usfm(nodes: Vec<Node>, diags: &mut Vec<MietteDiagnostic>) -> Vec<ParaContent> {
    let mut result = nodes
        .into_iter()
        .map(|node| para_from_usfm(node, diags).0)
        .collect_vec();
    result.shrink_to_fit();
    result
}

fn usjs_from_usfm(nodes: Vec<Node>, diags: &mut Vec<MietteDiagnostic>) -> Vec<UsjContent> {
    let mut result = nodes
        .into_iter()
        .map(|node| usj_from_usfm(node, diags).0)
        .collect_vec();
    result.shrink_to_fit();
    result
}

fn option_string_from_usfm(nodes: Vec<Node>, diags: &mut Vec<MietteDiagnostic>) -> Option<String> {
    let mut paras = nodes
        .into_iter()
        .map(|node| para_from_usfm(node, diags))
        .collect::<SmallVec<[_; 1]>>()
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

fn parse_string<T>(
    str: &str,
    span: Span,
    what: &str,
    fallback_str: &str,
    diags: &mut Vec<MietteDiagnostic>,
) -> T
where
    T: FromStr + Default,
    T::Err: ToString,
{
    try_parse_string(str, span, what, fallback_str, diags).unwrap_or_else(T::default)
}

fn try_parse_string<T>(
    str: &str,
    span: Span,
    what: &str,
    fallback_str: &str,
    diags: &mut Vec<MietteDiagnostic>,
) -> Option<T>
where
    T: FromStr,
    T::Err: ToString,
{
    match str.parse() {
        Ok(value) => Some(value),
        Err(err) => {
            diags.push(
                MietteDiagnostic::new(format!(
                    "Invalid or unsupported {what}, falling back to {fallback_str}"
                ))
                .with_label(LabeledSpan::at(span, err.to_string())),
            );
            None
        }
    }
}

fn parse_attributes(attributes: Vec<Attribute>) -> AttributesMap {
    attributes
        .into_iter()
        .map(|attr| (attr.key, attr.value))
        .collect()
}

#[cfg(test)]
mod test {
    use crate::bible_data::BibleDataError;
    use crate::usj::content::UsjContent;
    use crate::usj::content::{AttributesMap, NoteCaller, ParaContent};
    use crate::usj::loader::load_footnote_from_usfm;
    use crate::usj::marker::{ContentMarker, NoteMarker};
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_load_footnote() -> Result<(), Box<dyn Error>> {
        let usfm = "\\f +\\ft Test footnote \\nd Lord\\nd*\\f*";
        let usj = UsjContent::Note {
            marker: NoteMarker::F(()),
            caller: NoteCaller::Generated,
            category: None,
            content: vec![ParaContent::Usj(UsjContent::Character {
                marker: ContentMarker::Ft(()),
                content: vec![
                    ParaContent::Plain("Test footnote ".to_string()),
                    ParaContent::Usj(UsjContent::Character {
                        marker: ContentMarker::Nd(()),
                        content: vec![ParaContent::Plain("Lord".to_string())],
                        attributes: AttributesMap::default(),
                    }),
                ],
                attributes: AttributesMap::default(),
            })],
        };

        let converted_usj = load_footnote_from_usfm(usfm.to_string())?;
        assert_eq!(converted_usj.usj, usj);
        assert!(
            converted_usj.diagnostics.is_empty(),
            "{:#?}",
            converted_usj.diagnostics,
        );

        Ok(())
    }

    #[test]
    fn test_load_footnote_extra_data() {
        let usfm = "\\f +\\ft Test footnote\\f*\n\\b\n\\p Hello";
        let usj = load_footnote_from_usfm(usfm.to_string());
        assert!(
            matches!(&usj, Err(BibleDataError::InjectedFootnoteLength(3))),
            "{usj:#?}",
        );
    }

    #[test]
    fn test_load_footnote_not_note() {
        let usfm = "\\p Hello, world!";
        let usj = load_footnote_from_usfm(usfm.to_string());
        assert!(
            matches!(&usj, Err(BibleDataError::InjectedFootnoteNotNote(marker)) if marker == "p"),
            "{usj:#?}",
        );
    }
}
