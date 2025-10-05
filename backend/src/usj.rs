use crate::book_data::Book;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct UsjRoot {
    pub content: Vec<UsjContent>,
    #[serde(flatten)]
    remainder: serde_json::Value,
}

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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsjContent {
    Book {
        code: Book,
        #[serde(flatten)]
        remainder: serde_json::Value,
    },
    Para {
        content: Vec<UsjContent>,
        #[serde(flatten)]
        remainder: serde_json::Value,
    },

    #[serde(untagged)]
    Plain(String),
    #[serde(untagged)]
    Other(serde_json::Value),
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
