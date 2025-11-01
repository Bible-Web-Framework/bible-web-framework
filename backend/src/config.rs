use crate::book_data::Book;
use crate::usj::{UsjBookInfo, UsjRoot, load_usj};
use bimap::BiMap;
use notify_debouncer_full::notify;
use notify_debouncer_full::notify::EventKind;
use notify_debouncer_full::notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use rayon::iter::{ParallelBridge, ParallelIterator};
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ffi::{OsStr, OsString};
use std::fs::canonicalize;
use std::io;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Instant;
use unicase::UniCase;

#[derive(Debug)]
pub struct UsjFileMap {
    pub root_dir: PathBuf,
    pub files: HashMap<Book, UsjRoot>,
    pub sources: BiMap<Book, OsString>,
    has_ignored_files: bool,
}

impl UsjFileMap {
    pub fn new(root_dir: PathBuf) -> io::Result<Self> {
        const BOOK_COUNT: usize = 66;
        Ok(UsjFileMap {
            root_dir: canonicalize(root_dir)?,
            files: HashMap::with_capacity(BOOK_COUNT),
            sources: BiMap::with_capacity(BOOK_COUNT),
            has_ignored_files: false,
        })
    }

    fn insert_or_warn(&mut self, usj: UsjRoot, source: OsString) -> Option<UsjBookInfo> {
        let Some(book) = usj.book_info() else {
            tracing::error!("Book at {} missing book identifier", source.display());
            return None;
        };
        match self.files.entry(book.book) {
            Entry::Vacant(e) => {
                e.insert(usj);
                self.sources.insert(book.book, source);
            }
            Entry::Occupied(mut e) => {
                let old_path = self.sources.get_by_left(&book.book).unwrap();
                if &source == old_path {
                    e.insert(usj);
                } else {
                    self.has_ignored_files = true;
                    let new_description = book
                        .description
                        .map_or("".to_string(), |x| format!(" ({x})"));
                    tracing::warn!(
                        "Duplicate USJ files for book {}: {}{} and {}{new_description}. The latter{new_description} will be ignored.",
                        book.book,
                        old_path.display(),
                        e.get()
                            .book_info()
                            .unwrap()
                            .description
                            .map_or("".to_string(), |x| format!(" ({x})")),
                        source.display(),
                    );
                }
                return None;
            }
        }
        Some(book)
    }

    fn load_usj_or_warn(&self, file: &OsStr) -> Option<UsjRoot> {
        load_usj(self.root_dir.join(file))
            .inspect_err(|err| tracing::error!("Failed to load {}: {err}", file.display()))
            .ok()
    }

    fn insert_from_file_or_warn(&mut self, file: OsString) -> Option<UsjBookInfo> {
        self.load_usj_or_warn(&file)
            .and_then(|usj| self.insert_or_warn(usj, file))
    }

    pub fn reload_all_from_dir(&mut self) -> io::Result<()> {
        self.files.clear();
        self.sources.clear();
        self.has_ignored_files = false;
        let start = Instant::now();
        std::fs::read_dir(&self.root_dir)?
            .par_bridge()
            .filter_map(|entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(err) => return Some(Err(err)),
                };
                let file_type = match entry.file_type() {
                    Ok(t) => t,
                    Err(err) => return Some(Err(err)),
                };
                if !file_type.is_file() {
                    return None;
                }
                let file = entry.file_name();
                self.load_usj_or_warn(&file).map(|usj| Ok((usj, file)))
            })
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .for_each(|(usj, source)| {
                self.insert_or_warn(usj, source);
            });
        tracing::info!(
            "Loaded {} USJ files in {:?}",
            self.files.len(),
            start.elapsed()
        );
        Ok(())
    }

    pub fn handle_file_change(&mut self, event: notify::Event) -> io::Result<()> {
        if event.need_rescan() {
            tracing::debug!("File watcher requested full rescan");
            return self.reload_all_from_dir();
        }
        let get_path = |index: usize| event.paths[index].file_name().unwrap().to_owned();
        match event.kind {
            EventKind::Create(CreateKind::File | CreateKind::Any)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                let path = get_path(0);
                if self.root_dir.join(&path).is_file()
                    && let Some(book) = self.insert_from_file_or_warn(path.clone())
                {
                    tracing::info!("Loaded new book {book} from {}", path.display());
                }
            }
            EventKind::Modify(ModifyKind::Data(_)) => {
                let path = get_path(0);
                let old_book = self
                    .sources
                    .remove_by_right(&path)
                    .and_then(|(b, _)| self.files.remove(&b))
                    .and_then(|b| b.book_info());
                if let Some(new_book) = self.insert_from_file_or_warn(path.clone()) {
                    if let Some(old_book) = old_book
                        && new_book != old_book
                    {
                        tracing::info!(
                            "Loaded book {new_book} from {} (was {old_book})",
                            path.display()
                        );
                    } else {
                        tracing::info!("Loaded book {new_book} from {}", path.display());
                    }
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let old_path = get_path(0);
                let new_path = get_path(1);
                if let Some((book, _)) = self.sources.remove_by_right(&old_path) {
                    tracing::info!(
                        "Detected rename of book {book} source file from {} to {}",
                        old_path.display(),
                        new_path.display()
                    );
                    self.sources.insert(book, new_path);
                }
            }
            EventKind::Remove(RemoveKind::File | RemoveKind::Any)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                let path = get_path(0);
                if let Some((book, _)) = self.sources.remove_by_right(&path) {
                    self.files.remove(&book);
                    tracing::info!("Removed book {book} sourced from {}", path.display());
                    if self.has_ignored_files {
                        tracing::info!("Reloading all books due to previously ignored files");
                        self.reload_all_from_dir()?;
                    }
                }
            }
            unknown => tracing::debug!("Received unknown file watch event {unknown:?}: {event:?}"),
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BibleConfig {
    pub usj: UsjFileMap,
    pub additional_aliases: HashMap<UniCase<Cow<'static, str>>, Book>,
}

impl BibleConfig {
    pub fn load_initial(usj_dir: PathBuf) -> io::Result<BibleConfig> {
        let mut usj = UsjFileMap::new(usj_dir)?;
        usj.reload_all_from_dir()?;
        Ok(BibleConfig {
            usj,
            additional_aliases: HashMap::new(), // TODO: Parse from config
        })
    }
}

pub type BibleConfigLock = RwLock<BibleConfig>;
