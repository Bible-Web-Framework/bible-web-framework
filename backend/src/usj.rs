use crate::bible_data::BibleDataError;
use crate::book_data::Book;
use crate::serde_display_and_parse;
use crate::utils::CloneToOwned;
use crate::utils::serde_as::OptionAsVec;
use crate::verse_range::VerseRange;
use either::Either;
use ere::compile_regex;
use monostate::MustBe;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io::BufRead;
use std::num::NonZeroU8;
use std::slice::SliceIndex;

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TranslatedBookInfo<'a> {
    pub running_header: Option<Cow<'a, str>>,
    pub long_book_name: Option<Cow<'a, str>>,
    pub short_book_name: Option<Cow<'a, str>>,
    pub book_abbreviation: Option<Cow<'a, str>>,
}

impl TranslatedBookInfo<'_> {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        [
            &self.running_header,
            &self.long_book_name,
            &self.short_book_name,
            &self.book_abbreviation,
        ]
        .into_iter()
        .filter_map(Option::as_deref)
    }

    pub fn as_owned(&self) -> TranslatedBookInfo<'static> {
        TranslatedBookInfo {
            running_header: self.running_header.clone_to_owned(),
            long_book_name: self.long_book_name.clone_to_owned(),
            short_book_name: self.short_book_name.clone_to_owned(),
            book_abbreviation: self.book_abbreviation.clone_to_owned(),
        }
    }

    pub fn is_full(&self) -> bool {
        matches!(
            (
                &self.running_header,
                &self.long_book_name,
                &self.short_book_name,
                &self.book_abbreviation
            ),
            (Some(_), Some(_), Some(_), Some(_)),
        )
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
        #[serde_as(as = "OptionAsVec")]
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

pub type ParaIndex = (usize, usize);

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

    pub fn translated_book_info(&self) -> TranslatedBookInfo<'_> {
        let mut info = TranslatedBookInfo::default();
        for element in self
            .content
            .iter()
            .take_while(|x| !matches!(x, UsjContent::Chapter { .. }))
        {
            let UsjContent::Paragraph { marker, content } = element else {
                continue;
            };
            let [ParaContent::Plain(text)] = &content[..] else {
                continue;
            };
            match marker.as_str() {
                "h" | "h1" => info.running_header = Some(Cow::Borrowed(text)),
                "toc1" => info.long_book_name = Some(Cow::Borrowed(text)),
                "toc2" => info.short_book_name = Some(Cow::Borrowed(text)),
                "toc3" => info.book_abbreviation = Some(Cow::Borrowed(text)),
                _ => continue,
            }
            if info.is_full() {
                break;
            }
        }
        info
    }

    pub fn find_reference(
        &self,
        chapter: NonZeroU8,
        verse_range: VerseRange,
    ) -> Option<(ParaIndex, Vec<UsjContent>)> {
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
            .or_else(|| self.find_next_chapter_start(start.0 + 1));

        let mut result = self.slice_para(start, end);
        if let Some(label) = base_chapter_label {
            result.insert(0, label);
        }
        Some((start, result))
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

    fn find_next_chapter_start(&self, start_index: usize) -> Option<ParaIndex> {
        self.content
            .iter()
            .enumerate()
            .skip(start_index)
            .find(|(_, x)| matches!(x, UsjContent::Chapter { .. }))
            .map(|(idx, _)| (idx, 0))
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

pub fn load_usj(reader: impl BufRead) -> Result<UsjContent, BibleDataError> {
    Ok(serde_json::from_reader(reader)?)
}
