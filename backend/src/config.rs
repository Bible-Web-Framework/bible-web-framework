use crate::ServerError;
use crate::book_data::Book;
use crate::usj::{UsjRoot, load_usj};
use rayon::iter::{ParallelBridge, ParallelIterator};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug)]
pub struct BibleConfig {
    pub usj_files: HashMap<Book, UsjRoot>,
    pub additional_aliases: HashMap<String, Book>,
}

impl BibleConfig {
    pub fn load_initial(usj_dir: impl AsRef<Path>) -> Result<BibleConfig, ServerError> {
        Ok(BibleConfig {
            usj_files: std::fs::read_dir(usj_dir)?
                .par_bridge()
                .filter_map(|file| {
                    let entry = match file {
                        Ok(f) => f,
                        Err(e) => return Some(Err(e)),
                    };
                    match load_usj(entry.path()) {
                        Ok(usj) => Some(Ok(usj)),
                        Err(err) => {
                            tracing::error!("Failed to load {}: {err}", entry.path().display());
                            None
                        }
                    }
                })
                .collect::<std::io::Result<HashMap<_, _>>>()?,
            additional_aliases: HashMap::new(), // TODO: Parse from config
        })
    }
}
