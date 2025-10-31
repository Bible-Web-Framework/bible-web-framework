use crate::book_data::Book;
use crate::reference::VerseRange;
use ere::compile_regex;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use std::path::Path;
use std::slice::SliceIndex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsjRoot {
    pub content: Vec<UsjContent>,
    #[serde(flatten)]
    remainder: serde_json::Value,
}

type ParaIndex = (usize, usize);

impl UsjRoot {
    pub fn book(&self) -> Option<Book> {
        self.content.iter().find_map(|content| {
            if let UsjContent::Book { code, .. } = content {
                Some(*code)
            } else {
                None
            }
        })
    }

    pub fn find_reference(&self, chapter: u8, verse_range: VerseRange) -> Option<Vec<UsjContent>> {
        let chapter_start = self.find_chapter_start(chapter)?;

        let (start, base_chapter_label) = if verse_range.0 == 1 {
            (chapter_start, self.find_chapter_label())
        } else {
            let after_chapter_start = self.next_para_index(chapter_start)?;
            (
                self.find_verse_start_para(verse_range.0, after_chapter_start)?,
                None,
            )
        };
        let end = self
            .find_verse_start_para(verse_range.1 + 1, self.next_para_index(start)?)
            .or_else(|| self.find_chapter_start(chapter + 1));

        let mut result = self.slice_para(start, end);
        if let Some(label) = base_chapter_label {
            result.insert(0, label);
        }
        Some(result)
    }

    fn find_chapter_label(&self) -> Option<UsjContent> {
        self.content
            .iter()
            .take_while(|x| !matches!(x, UsjContent::Chapter { .. }))
            .find(|x| {
                if let UsjContent::Para {
                    marker, content, ..
                } = x
                    && marker == "cl"
                    && let &[UsjContent::Plain(_)] = &content.as_slice()
                {
                    true
                } else {
                    false
                }
            })
            .cloned()
    }

    fn find_chapter_start(&self, chapter: u8) -> Option<ParaIndex> {
        let chapter_index = self
            .content
            .iter()
            .position(|x| matches!(x, UsjContent::Chapter { number, .. } if *number == chapter))?;
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
            let prev_index = index.0 - 1;
            if let Some(para_content) = self
                .content
                .get(prev_index)
                .and_then(UsjContent::as_para_content)
            {
                Some((prev_index, para_content.len() - 1))
            } else {
                Some((prev_index, 0))
            }
        } else {
            None
        }
    }

    fn find_verse_start_para(&self, verse: u8, start: ParaIndex) -> Option<ParaIndex> {
        let (start_root, mut start_inner) = start;
        let mut verse_start = self
            .content
            .iter()
            .enumerate()
            .skip(start_root)
            .take_while(|(_, element)| !matches!(element, UsjContent::Chapter { .. }))
            .find_map(|(root_index, element)| {
                let content = element.as_para_content()?;
                let skip = std::mem::take(&mut start_inner);
                content
                    .iter()
                    .skip(skip)
                    .position(|x| matches!(x, UsjContent::Verse { number, .. } if *number == verse))
                    .map(|inner_index| (root_index, inner_index + skip))
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
        Some(verse_start)
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
        sub_index: impl SliceIndex<[UsjContent], Output = [UsjContent]>,
    ) -> UsjContent {
        match &self.content[index] {
            UsjContent::Para {
                marker,
                content,
                remainder,
            } => UsjContent::Para {
                content: Vec::from(&content[sub_index]),
                marker: marker.to_string(),
                remainder: remainder.clone(),
            },
            element => element.clone(),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsjContent {
    Book {
        code: Book,
        #[serde(flatten)]
        remainder: serde_json::Value,
    },
    Para {
        marker: String,
        content: Vec<UsjContent>,
        #[serde(flatten)]
        remainder: serde_json::Value,
    },
    Chapter {
        #[serde_as(as = "DisplayFromStr")]
        number: u8,
        #[serde(flatten)]
        remainder: serde_json::Value,
    },
    Verse {
        #[serde_as(as = "DisplayFromStr")]
        number: u8,
        #[serde(flatten)]
        remainder: serde_json::Value,
    },

    #[serde(untagged)]
    Plain(String),
    #[serde(untagged)]
    Other(serde_json::Value),
}

impl UsjContent {
    fn as_para_content(&self) -> Option<&Vec<UsjContent>> {
        if let UsjContent::Para { content, .. } = self {
            Some(content)
        } else {
            None
        }
    }

    fn is_title_para(&self) -> bool {
        const REGEX: ere::Regex =
            compile_regex!("mt[1-4]?|mte[1-2]?|cl|cd|ms[1-3]?|mr|s[1-4]?|sr|r|d|sp|sd[1-4]?");
        matches!(self, UsjContent::Para { marker, .. } if REGEX.test(marker))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UsjLoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No book ID tag")]
    NoBook,
}

pub fn load_usj(path: impl AsRef<Path>) -> Result<(Book, UsjRoot), UsjLoadError> {
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let usj: UsjRoot = serde_json::from_reader(reader)?;
    Ok((usj.book().ok_or(UsjLoadError::NoBook)?, usj))
}
