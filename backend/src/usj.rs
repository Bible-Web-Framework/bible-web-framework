use crate::book_data::Book;
use crate::reference::VerseRange;
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

        let start = if verse_range.0 == 1 {
            chapter_start
        } else {
            let after_chapter_start = self.next_para_index(chapter_start)?;
            let this_verse = self.find_verse_para(verse_range.0, after_chapter_start)?;
            if this_verse.1 == 0 {
                let prev_verse = self.find_verse_para(verse_range.0 - 1, after_chapter_start)?;
                (prev_verse.0 + 1, 0)
            } else {
                this_verse
            }
        };
        let end = self
            .find_verse_para(verse_range.1 + 1, self.next_para_index(start)?)
            .or_else(|| self.find_chapter_start(chapter + 1));

        Some(self.slice_para(start, end))
    }

    fn find_chapter_start(&self, chapter: u8) -> Option<ParaIndex> {
        let chapter_index = self
            .content
            .iter()
            .position(|x| matches!(x, UsjContent::Chapter { number, .. } if *number == chapter))?;
        Some((chapter_index, 0))
    }

    fn next_para_index(&self, index: ParaIndex) -> Option<ParaIndex> {
        if let Some(para_content) = self.content.get(index.0).and_then(UsjContent::as_p_para)
            && index.1 + 1 < para_content.len()
        {
            Some((index.0, index.1 + 1))
        } else {
            (index.0 + 1 < self.content.len()).then_some((index.0 + 1, 0))
        }
    }

    fn find_verse_para(&self, verse: u8, start: ParaIndex) -> Option<ParaIndex> {
        let (start_root, mut start_inner) = start;
        self.content
            .iter()
            .enumerate()
            .skip(start_root)
            .take_while(|(_, element)| !matches!(element, UsjContent::Chapter { .. }))
            .find_map(|(root_index, element)| {
                let content = element.as_p_para()?;
                let skip = std::mem::take(&mut start_inner);
                content
                    .iter()
                    .skip(skip)
                    .position(|x| matches!(x, UsjContent::Verse { number, .. } if *number == verse))
                    .map(|inner_index| (root_index, inner_index - skip))
            })
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
            } if marker == "p" => UsjContent::Para {
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
    fn as_p_para(&self) -> Option<&Vec<UsjContent>> {
        if let UsjContent::Para {
            content, marker, ..
        } = self
            && marker == "p"
        {
            Some(content)
        } else {
            None
        }
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
    let reader = std::fs::File::open(path)?;
    let usj: UsjRoot = serde_json::from_reader(reader)?;
    Ok((usj.book().ok_or(UsjLoadError::NoBook)?, usj))
}
