use crate::book_data::Book;
use crate::serde_display_and_parse;
use crate::usj::is_title_marker;
use crate::usj::root::UsjRoot;
use crate::utils::{parsed_string_value::ParsedStringValue, serde_as::OptionAsVec};
use crate::verse_range::VerseRange;
use either::Either;
use monostate::MustBe;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use std::collections::BTreeMap;
use std::num::NonZeroU8;

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
        #[serde_as(as = "OptionAsVec")]
        content: Option<String>,
        code: Book,
    },

    Chapter {
        marker: MustBe!("c"),
        number: ParsedStringValue<NonZeroU8>,
        #[serde(rename = "altnumber", skip_serializing_if = "Option::is_none")]
        alt_number: Option<String>,
        #[serde(rename = "pubnumber", skip_serializing_if = "Option::is_none")]
        pub_number: Option<String>,
        sid: String,
    },

    Verse {
        marker: MustBe!("v"),
        number: ParsedStringValue<VerseRange>,
        #[serde(rename = "altnumber", skip_serializing_if = "Option::is_none")]
        alt_number: Option<String>,
        #[serde(rename = "pubnumber", skip_serializing_if = "Option::is_none")]
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
        #[serde_as(as = "OptionAsVec")]
        content: Option<String>,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    #[serde(rename = "ref")]
    Reference {
        #[serde_as(as = "OptionAsVec")]
        content: Option<String>,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    Periph {
        alt: String,
        content: Vec<UsjContent>,
        #[serde(flatten)]
        attributes: AttributesMap,
    },

    OptBreak,
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
            Self::Periph { .. } => None,
            Self::OptBreak => None,
        }
    }

    pub fn marker_or_type(&self) -> &str {
        self.marker().unwrap_or_else(|| match self {
            Self::Root(_) => "USJ",
            Self::Table { .. } => "table",
            Self::Reference { .. } => "ref",
            Self::Periph { .. } => "periph",
            Self::OptBreak => "optbreak",
            _ => unreachable!("All other variants should be handled by marker()"),
        })
    }

    pub fn insert_usj_content(&mut self, index: usize, new_content: UsjContent) -> bool {
        match self {
            UsjContent::Root(UsjRoot { content, .. })
            | UsjContent::Table { content, .. }
            | UsjContent::TableRow { content, .. }
            | UsjContent::Sidebar { content, .. }
            | UsjContent::Periph { content, .. } => content.insert(index, new_content),

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
            | UsjContent::Sidebar { content, .. }
            | UsjContent::Periph { content, .. } => Either::Left(content.get(index)?),

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
            | UsjContent::Sidebar { content, .. }
            | UsjContent::Periph { content, .. } => Either::Left(content.get_mut(index)?),

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

    pub(super) fn as_para_content(&self) -> Option<&Vec<ParaContent>> {
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
