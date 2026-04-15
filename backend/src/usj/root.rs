use crate::usj::content::{ParaContent, UsjContent};
use crate::usj::marker::ContentMarker;
use crate::usj::{ParaIndex, TranslatedBookInfo, UsjBookInfo};
use crate::verse_range::VerseRange;
use oxicode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::num::NonZeroU8;
use std::slice::SliceIndex;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct UsjRoot {
    pub version: Cow<'static, str>,
    pub content: Vec<UsjContent>,
}

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
            match marker {
                ContentMarker::H(_) => info.running_header = Some(Cow::Borrowed(text)),
                ContentMarker::Toc(1) => info.long_book_name = Some(Cow::Borrowed(text)),
                ContentMarker::Toc(2) => info.short_book_name = Some(Cow::Borrowed(text)),
                ContentMarker::Toc(3) => info.book_abbreviation = Some(Cow::Borrowed(text)),
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
                if start == chapter_start {
                    self.next_para_index(start)?
                } else {
                    start
                },
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
                    && *marker == ContentMarker::Cl(())
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
        let chapter_index = self.content.iter().position(
            |x| matches!(&x, UsjContent::Chapter { number, .. } if number.value == chapter),
        )?;
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
                            && range.value.contains(verse)
                        {
                            Some(((root_index, index), range.value))
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
                marker: *marker,
            },
            element => element.clone(),
        }
    }
}
