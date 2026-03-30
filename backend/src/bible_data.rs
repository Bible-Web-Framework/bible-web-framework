use crate::api::{ApiError, ApiResult};
use crate::book_data::{Book, BookParseOptions};
use crate::index::{BibleIndex, ReindexType};
use crate::reference::BibleReference;
use crate::usj::UsjBookInfo;
use crate::usj::content::UsjContent;
use crate::usj::loader::load_usj;
use crate::usj::loader::load_usj_from_usfm;
use crate::usj::root::UsjRoot;
use crate::utils::normalize::normalize_str;
use crate::utils::ordered_enum::EnumOrderMap;
use crate::utils::prefix_tree::PrefixTree;
use crate::utils::{ExclusiveMutex, ToUnicaseCow};
use bimap::{BiMap, Overwritten};
use charabia::{Language, Tokenizer, TokenizerBuilder};
use dashmap::mapref::one::{MappedRef, Ref};
use dashmap::{DashMap, Entry};
use enum_map::Enum;
use ere::{Regex, compile_regex};
use fst::Streamer;
use miette::{GraphicalReportHandler, NamedSource, Severity};
use notify_debouncer_full::notify;
use notify_debouncer_full::notify::EventKind;
use notify_debouncer_full::notify::event::{
    CreateKind, EventAttributes, ModifyKind, RemoveKind, RenameMode,
};
use parking_lot::RwLock;
use rangemap::RangeInclusiveMap;
use rayon::prelude::{IntoParallelIterator, ParallelBridge, ParallelIterator};
use serde::{Deserialize, Serialize};
use smallvec::smallvec;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, LinkedList};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::{io, mem, path};
use sync_file::SyncFile;
use thiserror::Error;
use unicase::UniCase;
use zip::ZipArchive;
use zip::result::ZipError;

pub struct MultiBibleData {
    pub root_dir: PathBuf,
    pub default_bible: String,
    pub disabled_bibles: HashSet<Cow<'static, str>>,
    pub bibles: DashMap<Cow<'static, str>, BibleData>,
    file_change_active: ExclusiveMutex,
}

#[derive(Default)]
pub struct BibleData {
    pub source: PathBuf,
    pub source_is_zip: bool,
    pub id: String,
    pub config: RwLock<Arc<BibleConfig>>,
    pub books: DashMap<Book, BookData>,
    pub index: RwLock<BibleIndex>,
    sources: RwLock<BiMap<Book, Cow<'static, str>>>,
    has_ignored_files: AtomicBool,
    full_reload_active: ExclusiveMutex,
}

#[derive(Debug, Default)]
pub struct BibleConfig {
    pub display_name: Option<String>,
    pub text_direction: TextDirection,
    pub book_order: EnumOrderMap<Book>,
    pub book_aliases: HashMap<UniCase<Cow<'static, str>>, Book>,
    pub search: SearchConfig,
    pub footnotes: FootnotesTree,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
pub enum TextDirection {
    #[serde(rename = "auto")]
    #[default]
    Auto,
    #[serde(rename = "ltr")]
    LeftToRight,
    #[serde(rename = "rtl")]
    RightToLeft,
}

#[derive(Debug, Default)]
pub struct SearchConfig {
    pub languages: Option<Box<[Language]>>,
    pub ignored_words: Option<fst::Set<Box<[u8]>>>,
}

pub type FootnotesTree = PrefixTree<String, RangeInclusiveMap<BibleReference, FootnotesConfig>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FootnotesConfig {
    pub footnote: UsjContent,
}

pub struct BookData {
    usj: UsjContent,
    names: HashSet<UniCase<Cow<'static, str>>>,
}

impl MultiBibleData {
    pub fn load(
        bibles_dir: PathBuf,
        default_bible: String,
        disabled_bibles: HashSet<Cow<'static, str>>,
    ) -> ConfigResult<Self> {
        let result = Self {
            root_dir: bibles_dir,
            default_bible,
            disabled_bibles,
            bibles: DashMap::new(),
            file_change_active: ExclusiveMutex::default(),
        };
        result.reload_everything()?;
        Ok(result)
    }

    pub fn get_or_api_error(
        &self,
        bible: String,
    ) -> ApiResult<Ref<'_, Cow<'static, str>, BibleData>> {
        let bible = Cow::Owned(bible);
        self.bibles
            .get(&bible)
            .ok_or_else(|| ApiError::UnknownBible(bible.into_owned()))
    }

    pub fn handle_file_change(&self, mut event: notify::Event) -> ConfigResult<()> {
        if event.need_rescan() {
            tracing::info!("File watcher requested full rescan. Reloading everything.");
            return self.reload_everything();
        }
        let _lock = self
            .file_change_active
            .lock()
            .expect("MultiBibleData::handle_file_change called while already reloading");

        let get_path = |index: usize| {
            let path = event.paths[index].as_path();
            (
                path,
                path.strip_prefix(&self.root_dir)
                    .expect("MultiBibleData::handle_file_change called with unrelated filename"),
            )
        };
        fn get_root_bible_id(trimmed_path: &Path) -> Cow<'_, str> {
            BibleData::get_file_bible_id(Path::new(
                trimmed_path.components().next().unwrap().as_os_str(),
            ))
        }
        fn get_inside_bible_path(trimmed_path: &Path) -> Cow<'_, str> {
            let mut iter = trimmed_path.components();
            iter.next();
            iter.as_path().to_string_lossy()
        }

        match event.kind {
            EventKind::Create(CreateKind::Any | CreateKind::File | CreateKind::Folder)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                let (path, trimmed_path) = get_path(0);
                let is_file = match event.kind {
                    EventKind::Create(CreateKind::File) => true,
                    EventKind::Create(CreateKind::Folder) => false,
                    _ => {
                        let file_type = path.metadata()?.file_type();
                        if !file_type.is_file() && !file_type.is_dir() {
                            return Ok(());
                        }
                        file_type.is_file()
                    }
                };
                if is_file {
                    if trimmed_path.components().nth(1).is_none() {
                        if !self.check_for_disabled_load(path) {
                            tracing::info!("Loading new bible from {}", path.display());
                            let data = BibleData::load_from_zip(path.to_owned())?;
                            self.bibles.insert(Cow::Owned(data.id.clone()), data);
                        }
                    } else {
                        let bible_id = get_root_bible_id(trimmed_path);
                        if let Some(bible) = self.bibles.get(&*bible_id)
                            && let Some(load) = bible
                                .insert_from_file_or_warn(path, get_inside_bible_path(trimmed_path))
                        {
                            match load {
                                LoadComplete::Config { needs_reindex } => {
                                    tracing::info!("Loaded new bible.toml from {}", path.display());
                                    if needs_reindex {
                                        bible.update_index(ReindexType::FullReindex);
                                    }
                                }
                                LoadComplete::Book(book) => {
                                    tracing::info!(
                                        "Loaded new book {book} from {}",
                                        path.display()
                                    );
                                    bible.update_index(ReindexType::PartialReindex(smallvec![
                                        book.book
                                    ]));
                                }
                            }
                            return Ok(());
                        }
                    }
                } else if trimmed_path.components().nth(1).is_none()
                    && !self.check_for_disabled_load(path)
                {
                    tracing::info!("Loading new bible from {}", path.display());
                    let data = BibleData::load_from_dir(path.to_owned())?;
                    self.bibles.insert(Cow::Owned(data.id.clone()), data);
                }
            }
            EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_)) => {
                // Any is only used in places that could be substituted for Data
                // It sometimes fires on directories though, at least on Windows.
                let (path, trimmed_path) = get_path(0);
                if !path.is_file() {
                    return Ok(());
                }
                let bible_id = get_root_bible_id(trimmed_path);
                let Some(bible) = self.bibles.get(&*bible_id) else {
                    return Ok(());
                };
                if trimmed_path.components().nth(1).is_some() {
                    let inside_path = get_inside_bible_path(trimmed_path);
                    let old_book = bible
                        .sources
                        .write()
                        .remove_by_right(&*inside_path)
                        .and_then(|(b, _)| bible.books.remove(&b))
                        .and_then(|(_, b)| b.usj.unwrap_root().book_info());
                    if let Some(load) = bible.insert_from_file_or_warn(path, inside_path) {
                        match load {
                            LoadComplete::Config { needs_reindex } => {
                                tracing::info!("Loaded new bible.toml from {}", path.display());
                                if needs_reindex {
                                    bible.update_index(ReindexType::FullReindex);
                                }
                            }
                            LoadComplete::Book(new_book) => {
                                let reindex_type = if let Some(old_book) = old_book
                                    && new_book != old_book
                                {
                                    tracing::info!(
                                        "Loaded book {new_book} in {bible_id} from {} (was {old_book})",
                                        path.display()
                                    );
                                    ReindexType::PartialReindex(smallvec![
                                        old_book.book,
                                        new_book.book
                                    ])
                                } else {
                                    tracing::info!(
                                        "Loaded book {new_book} in {bible_id} from {}",
                                        path.display()
                                    );
                                    ReindexType::PartialReindex(smallvec![new_book.book])
                                };
                                bible.update_index(reindex_type);
                            }
                        }
                    }
                } else {
                    tracing::info!("Reloading bible from {}", path.display());
                    bible.reload_all()?;
                    bible.update_index(ReindexType::FullReindex);
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let (old_path, old_trimmed_path) = get_path(0);
                let (new_path, new_trimmed_path) = get_path(1);
                let old_is_book = old_trimmed_path.components().nth(1).is_some();
                let new_is_book = new_trimmed_path.components().nth(1).is_some();
                match (old_is_book, new_is_book) {
                    (false, false) => {
                        let old_id = get_root_bible_id(old_trimmed_path);
                        let new_id = get_root_bible_id(new_trimmed_path);
                        if let Some((_, bible)) = self.bibles.remove(&*old_id) {
                            tracing::info!("Detected rename of bible {old_id} to {new_id}");
                            if self.disabled_bibles.contains(&new_id) {
                                tracing::info!(
                                    "Removing renamed bible {new_id} because it's disabled.",
                                );
                            } else {
                                self.bibles.insert(Cow::Owned(new_id.into_owned()), bible);
                            }
                        } else {
                            let mut paths = mem::take(&mut event.paths);
                            paths.remove(0);
                            return self.handle_rename_partial(RenameMode::To, paths);
                        }
                    }
                    (true, true) => {
                        let old_bible = get_root_bible_id(old_trimmed_path);
                        let new_bible = get_root_bible_id(new_trimmed_path);
                        let old_inside_path = get_inside_bible_path(old_trimmed_path);
                        let new_inside_path = get_inside_bible_path(new_trimmed_path);
                        let Some(old_bible_data) = self.bibles.get(&*old_bible) else {
                            let mut paths = mem::take(&mut event.paths);
                            paths.remove(0);
                            return self.handle_rename_partial(RenameMode::To, paths);
                        };
                        if old_bible == new_bible {
                            let mut sources = old_bible_data.sources.write();
                            if let Some((book, _)) = sources.remove_by_right(&*old_inside_path) {
                                tracing::info!(
                                    "Detected rename of book {book} in {old_bible} from {} to {}",
                                    old_path.display(),
                                    new_path.display(),
                                );
                                sources.insert(book, Cow::Owned(new_inside_path.into_owned()));
                            }
                        } else {
                            let Some(new_bible_data) = self.bibles.get(&*new_bible) else {
                                return Ok(());
                            };
                            let Some((book, _)) = old_bible_data
                                .sources
                                .write()
                                .remove_by_right(&*old_inside_path)
                            else {
                                return Ok(());
                            };
                            tracing::info!(
                                "Detected rename of book {book} in {old_bible} from {} to new bible {new_bible} at {}",
                                old_path.display(),
                                new_path.display(),
                            );
                            new_bible_data.insert_or_warn(
                                LoadAction::Book(old_bible_data.books.remove(&book).unwrap().1),
                                new_inside_path.into_owned(),
                            );
                            old_bible_data.update_index(ReindexType::Unindex(book));
                        }
                    }
                    _ => {
                        let mut paths = mem::take(&mut event.paths);
                        self.handle_rename_partial(RenameMode::From, vec![paths.remove(0)])?;
                        return self.handle_rename_partial(RenameMode::To, paths);
                    }
                }
            }
            EventKind::Remove(RemoveKind::Any | RemoveKind::File | RemoveKind::Folder)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                let (path, trimmed_path) = get_path(0);
                let bible_id = get_root_bible_id(trimmed_path);
                if trimmed_path.components().nth(1).is_none() {
                    if self.bibles.remove(&*bible_id).is_some() {
                        tracing::info!("Removed bible {bible_id}");
                    }
                } else {
                    let Some(bible) = self.bibles.get(&*bible_id) else {
                        return Ok(());
                    };
                    let inside_path = get_inside_bible_path(trimmed_path);
                    if let Some((book, _)) = bible.sources.write().remove_by_right(&*inside_path) {
                        bible.books.remove(&book);
                        tracing::info!(
                            "Removed book {book} from {bible_id} sourced from {}",
                            path.display()
                        );
                        bible.update_index(ReindexType::Unindex(book));
                    }
                }
            }
            unknown => tracing::debug!("Received unknown file watch event {unknown:?}: {event:?}"),
        }
        Ok(())
    }

    fn handle_rename_partial(&self, mode: RenameMode, paths: Vec<PathBuf>) -> ConfigResult<()> {
        self.handle_file_change(notify::Event {
            kind: EventKind::Modify(ModifyKind::Name(mode)),
            paths,
            attrs: EventAttributes::default(),
        })
    }

    pub fn reload_everything(&self) -> ConfigResult<()> {
        let _lock = self
            .file_change_active
            .lock()
            .expect("MultiBibleData::reload_everything called while already reloading");

        let mut keys_to_keep = (!self.bibles.is_empty()).then(HashSet::<Cow<str>>::new);
        for entry in self.root_dir.read_dir()? {
            let entry = entry?;
            let path = entry.path();
            if self.check_for_disabled_load(&path) {
                continue;
            }
            let is_file = {
                let base = entry.file_type()?;
                if !base.is_symlink() {
                    base.is_file()
                } else {
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::FileTypeExt;
                        base.is_symlink_file()
                    }

                    #[cfg(not(windows))]
                    path.metadata()?.is_file()
                }
            };
            let data = if is_file {
                BibleData::load_from_zip(path)?
            } else {
                BibleData::load_from_dir(path)?
            };
            if let Some(keys) = &mut keys_to_keep {
                keys.insert(Cow::Owned(data.id.clone()));
            }
            self.bibles.insert(Cow::Owned(data.id.clone()), data);
        }
        if let Some(keys) = keys_to_keep {
            self.bibles.retain(|k, _| keys.contains(k));
        }
        Ok(())
    }

    fn check_for_disabled_load(&self, path: &Path) -> bool {
        if self.disabled_bibles.is_empty() {
            return false;
        }
        let bible_id = BibleData::get_file_bible_id(path);
        if self.disabled_bibles.contains(&bible_id) {
            tracing::info!("Skipping loading disabled bible {bible_id}");
            true
        } else {
            false
        }
    }
}

macro_rules! format_source {
    ($self:ident) => {
        format_args!("{}{}", $self.source.display(), path::MAIN_SEPARATOR)
    };
}

impl BibleData {
    pub fn load_from_dir(path: PathBuf) -> ConfigResult<Self> {
        Self::load(path, false)
    }

    pub fn load_from_zip(path: PathBuf) -> ConfigResult<Self> {
        Self::load(path, true)
    }

    fn load(path: PathBuf, from_zip: bool) -> ConfigResult<Self> {
        let data = BibleData {
            id: Self::get_file_bible_id(&path).into_owned(),
            source: path,
            source_is_zip: from_zip,
            books: DashMap::with_capacity(Book::LENGTH),
            ..Default::default()
        };
        data.reload_all()?;

        let mut index = data.index.write();
        index.log_marker = Some(data.id.clone());
        index.update_index(
            ReindexType::FullReindex,
            &data.books,
            &data.config.read().search.create_tokenizer(),
        );
        drop(index);

        Ok(data)
    }

    fn get_file_bible_id(path: &Path) -> Cow<'_, str> {
        path.file_stem()
            .unwrap_or_else(|| {
                path.file_name()
                    .expect("BibleData::load_from_zip called with non-file path")
            })
            .to_string_lossy()
    }

    pub fn book_parse_options(&self) -> impl BookParseOptions {
        struct Options<'a> {
            config: Arc<BibleConfig>,
            books: &'a DashMap<Book, BookData>,
        }

        impl BookParseOptions for Options<'_> {
            fn languages(&self) -> Option<&[Language]> {
                self.config.search.languages.as_deref()
            }

            fn lookup_book(&self, str: UniCase<&str>) -> Option<Book> {
                self.config
                    .book_aliases
                    .get(&str.to_cow())
                    .copied()
                    .or_else(|| {
                        self.books.iter().find_map(|book| {
                            book.names.contains(&str.to_cow()).then_some(*book.key())
                        })
                    })
            }

            fn book_allowed(&self, book: Book) -> bool {
                self.books.contains_key(&book)
            }
        }

        Options {
            config: self.config.read().clone(),
            books: &self.books,
        }
    }

    pub fn usj(&self, book: Book) -> Option<MappedRef<'_, Book, BookData, UsjContent>> {
        self.books.get(&book).map(|data| data.map(BookData::usj))
    }

    fn update_index(&self, reindex_type: ReindexType) {
        if matches!(reindex_type, ReindexType::Unindex(_))
            && self.has_ignored_files.load(Ordering::Acquire)
        {
            tracing::info!(
                "Reloading all books in {} due to previously ignored files",
                self.id
            );
            return self.update_index(ReindexType::FullReindex);
        }
        self.index.write().update_index(
            reindex_type,
            &self.books,
            &self.config.read().search.create_tokenizer(),
        );
    }

    fn insert_or_warn(&self, load: LoadAction, source: String) -> Option<LoadComplete> {
        match load {
            LoadAction::Config(new_config) => {
                let old_config = self.config.read();
                let languages_changed = new_config.search.languages != old_config.search.languages;
                let mut tokenizer_changed = languages_changed;
                if !tokenizer_changed
                    && let Some(new_words) = &new_config.search.ignored_words
                    && let Some(old_words) = &old_config.search.ignored_words
                {
                    tokenizer_changed = new_words
                        .op()
                        .add(old_words)
                        .symmetric_difference()
                        .next()
                        .is_some();
                }
                drop(old_config);

                *self.config.write() = new_config.clone();
                if languages_changed {
                    for mut book in self.books.iter_mut() {
                        book.value_mut()
                            .regenerate_names(new_config.search.languages.as_deref());
                    }
                }
                Some(LoadComplete::Config {
                    needs_reindex: tokenizer_changed,
                })
            }
            LoadAction::Book(mut data) => {
                let Some(book) = data.usj.as_root().and_then(UsjRoot::book_info) else {
                    tracing::error!(
                        "Book at {}{source} missing root element or book identifier",
                        format_source!(self),
                    );
                    return None;
                };
                data.regenerate_names(self.config.read().search.languages.as_deref());
                match self.books.entry(book.book) {
                    Entry::Vacant(e) => {
                        e.insert(data);
                        if let Overwritten::Right(book, _) =
                            self.sources.write().insert(book.book, Cow::Owned(source))
                        {
                            self.books.remove(&book);
                            self.update_index(ReindexType::Unindex(book));
                        }
                    }
                    Entry::Occupied(mut e) => {
                        let sources = self.sources.read();
                        let old_path = sources.get_by_left(&book.book).unwrap();
                        if &source == old_path {
                            e.insert(data);
                        } else {
                            self.has_ignored_files.store(true, Ordering::Release);
                            let new_description = book
                                .description
                                .map_or("".to_string(), |x| format!(" ({x})"));
                            tracing::warn!(
                                "Duplicate USJ files in {} for book {}: {old_path}{} and {source}{new_description}. The latter{new_description} will be ignored.",
                                self.source.display(),
                                book.book,
                                e.get()
                                    .usj
                                    .unwrap_root()
                                    .book_info()
                                    .unwrap()
                                    .description
                                    .map_or("".to_string(), |x| format!(" ({x})")),
                            );
                        }
                        return None;
                    }
                }
                Some(LoadComplete::Book(book))
            }
        }
    }

    fn load_or_warn(
        &self,
        filename: &str,
        reader: Result<impl Read, impl Into<BibleDataError>>,
    ) -> Option<LoadAction> {
        self.load_or_err(filename, reader)
            .inspect_err(|err| {
                tracing::error!("Failed to load {}{filename}: {err}", format_source!(self));
            })
            .ok()
            .flatten()
    }

    fn load_or_err(
        &self,
        filename: &str,
        reader: Result<impl Read, impl Into<BibleDataError>>,
    ) -> Result<Option<LoadAction>, BibleDataError> {
        const IGNORED_EXTENSIONS: Regex<3> =
            compile_regex!("^(a?png|avif|gif|jpe?g|jfif|pjp(eg)?|svg|webp)$");

        if filename == "bible.toml" {
            return Ok(Some(LoadAction::Config(Arc::new(
                reader
                    .map_err(Into::into)
                    .and_then(BibleConfig::from_reader)?,
            ))));
        }

        Ok(
            match filename
                .split('.')
                .next_back()
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("usj") => Some(LoadAction::Book(BookData::new(
                    reader
                        .map_err(Into::into)
                        .map(BufReader::new)
                        .and_then(load_usj)?,
                ))),
                Some("usfm" | "sfm") => {
                    let mut usfm = String::new();
                    let usj = reader
                        .map_err(Into::into)
                        .and_then(|mut reader| {
                            reader.read_to_string(&mut usfm).map_err(BibleDataError::Io)
                        })
                        .and_then(|_| load_usj_from_usfm(usfm))?;
                    if !usj.diagnostics.is_empty() {
                        let mut diag_message = String::new();
                        let source_code = Arc::new(NamedSource::new(
                            format!("{}{filename}", format_source!(self)),
                            usj.source,
                        ));
                        let reporter = GraphicalReportHandler::new();
                        let is_any_error = usj
                            .diagnostics
                            .iter()
                            .any(|x| x.severity.unwrap_or_default() == Severity::Error);
                        for diag in usj.diagnostics {
                            let report =
                                miette::Report::new(diag).with_source_code(source_code.clone());
                            diag_message.push('\n');
                            let _ = reporter.render_report(&mut diag_message, &*report);
                        }
                        if is_any_error {
                            tracing::error!(
                                "Errors in {}{filename}. The file will be attempted to be loaded, but errors may occur.{diag_message}",
                                format_source!(self),
                            );
                        } else {
                            tracing::warn!(
                                "Warnings in {}{filename}.{diag_message}",
                                format_source!(self),
                            );
                        }
                    }
                    Some(LoadAction::Book(BookData::new(usj.usj)))
                }
                Some(ext) if IGNORED_EXTENSIONS.test(ext) => None,
                Some(_) | None => {
                    tracing::warn!("Found unknown file {}{filename}", format_source!(self));
                    None
                }
            },
        )
    }

    fn insert_from_file_or_warn(&self, path: &Path, source: Cow<'_, str>) -> Option<LoadComplete> {
        self.load_or_warn(&source, File::open(path))
            .and_then(|usj| self.insert_or_warn(usj, source.into_owned()))
    }

    fn reload_all(&self) -> ConfigResult<()> {
        let _lock = self
            .full_reload_active
            .lock()
            .expect("BibleData::reload_all called while reload active");
        self.books.clear();
        self.sources.write().clear();
        self.has_ignored_files.store(false, Ordering::Relaxed);
        let start = Instant::now();
        let files = if !self.source_is_zip {
            walkdir::WalkDir::new(&self.source)
                .follow_links(true)
                .into_iter()
                .par_bridge()
                .filter_map(|entry| {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(err) => return Some(Err(err)),
                    };
                    if !entry.file_type().is_file() {
                        return None;
                    }
                    let path = entry.path();
                    let rel_path = path.strip_prefix(&self.source).unwrap().to_string_lossy();
                    self.load_or_warn(&rel_path, File::open(entry.path()))
                        .map(|usj| Ok((usj, rel_path.into_owned())))
                })
                .collect::<walkdir::Result<LinkedList<_>>>()?
        } else {
            let zip = ZipArchive::new(SyncFile::open(&self.source)?)?;
            (0..zip.len())
                .into_par_iter()
                .filter_map(|index| {
                    let mut zip = zip.clone();
                    let filename = zip.name_for_index(index).unwrap();
                    if filename
                        .chars()
                        .next_back()
                        .is_some_and(|c| c == '/' || c == '\\')
                    {
                        // Is a directory
                        return None;
                    }
                    let filename = filename.to_owned();
                    self.load_or_warn(&filename, zip.by_index(index))
                        .map(|usj| (usj, filename))
                })
                .collect()
        };
        for (usj, source) in files {
            self.insert_or_warn(usj, source);
        }
        tracing::info!(
            "Loaded {} USFM/USJ files from {} in {:?}",
            self.books.len(),
            self.source.display(),
            start.elapsed(),
        );
        Ok(())
    }
}

enum LoadAction {
    Config(Arc<BibleConfig>),
    Book(BookData),
}

enum LoadComplete {
    Config { needs_reindex: bool },
    Book(UsjBookInfo),
}

impl BibleConfig {
    fn from_reader(mut reader: impl Read) -> ConfigResult<Self> {
        let mut data = vec![];
        reader.read_to_end(&mut data)?;
        let unresolved: unresolved::BibleConfig = toml::from_slice(&data)?;
        Ok(unresolved.into())
    }
}

impl SearchConfig {
    pub fn create_tokenizer(&self) -> Tokenizer<'_> {
        let mut builder = TokenizerBuilder::new();
        if let Some(languages) = &self.languages {
            builder.allow_list(languages);
        }
        if let Some(words) = &self.ignored_words {
            builder.stop_words(words);
        }
        builder.into_tokenizer()
    }
}

impl BookData {
    pub fn new(usj: UsjContent) -> Self {
        Self {
            usj,
            names: HashSet::new(),
        }
    }

    fn regenerate_names(&mut self, languages: Option<&[Language]>) {
        self.names = self
            .usj
            .unwrap_root()
            .translated_book_info()
            .names()
            .map(|s| {
                UniCase::new(Cow::Owned(
                    normalize_str(Cow::Borrowed(s), languages).0.into_owned(),
                ))
            })
            .collect();
    }

    pub fn usj(&self) -> &UsjContent {
        &self.usj
    }
}

#[derive(Debug, Error)]
pub enum BibleDataError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Directory walk error: {0}")]
    WalkDir(#[from] walkdir::Error),
    #[error("Zip file error: {0}")]
    Zip(#[from] ZipError),
    #[error("Error parsing bible.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Error parsing USJ: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Injected footnote has multiple ({0}) paragraph elements")]
    InjectedFootnoteLength(usize),
    #[error("Injected footnote is not a note (was a {0})")]
    InjectedFootnoteNotNote(String),
}

pub type ConfigResult<T> = Result<T, BibleDataError>;

mod unresolved {
    use crate::bible_data::TextDirection;
    use crate::book_data::Book;
    use crate::reference::BibleReference;
    use crate::usj::content::UsjContent;
    use crate::utils::normalize::normalize_str;
    use crate::utils::ordered_enum::{EnumOrderMap, EnumOrderMapAs};
    use crate::utils::serde_as::{FootnoteUsfmAsUsj, LanguageAsCode};
    use charabia::normalizer::NormalizerOption;
    use charabia::{Language, Normalize, StrDetection, Token};
    use itertools::Itertools;
    use permutate::Permutator;
    use serde::Deserialize;
    use serde_with::{DisplayFromStr, NoneAsEmptyString, serde_as};
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::ops::RangeInclusive;
    use std::{iter, mem};
    use unicase::UniCase;

    #[serde_as]
    #[derive(Debug, Deserialize)]
    pub struct BibleConfig {
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        text_direction: TextDirection,
        #[serde_as(as = "EnumOrderMapAs<NoneAsEmptyString>")]
        #[serde(default)]
        book_order: EnumOrderMap<Book>,
        #[serde(default)]
        book_aliases: AliasesConfig,
        #[serde(default)]
        search: SearchConfig,
        #[serde(default)]
        footnotes: HashMap<String, Vec<FootnotesConfig>>,
    }

    #[derive(Debug, Default, Deserialize)]
    struct AliasesConfig {
        #[serde(default)]
        common: HashMap<String, Vec<String>>,
        books: HashMap<Book, Vec<BookAlias>>,
    }

    #[serde_as]
    #[derive(Debug, Default, Deserialize)]
    struct SearchConfig {
        #[serde_as(as = "Option<Vec<LanguageAsCode>>")]
        #[serde(default)]
        languages: Option<Vec<Language>>,
        #[serde(default)]
        ignored_words: Option<Vec<String>>,
    }

    #[serde_as]
    #[derive(Debug, Deserialize)]
    struct FootnotesConfig {
        bible_ranges: Vec<BibleRange>,
        #[serde_as(as = "FootnoteUsfmAsUsj")]
        footnote: UsjContent,
    }

    #[serde_as]
    #[derive(Copy, Clone, Debug, Deserialize)]
    #[serde(untagged)]
    enum BibleRange {
        Simple(#[serde_as(as = "DisplayFromStr")] BibleReference),
        MultiChapter(
            #[serde_as(as = "DisplayFromStr")] BibleReference,
            #[serde_as(as = "DisplayFromStr")] BibleReference,
        ),
    }

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum BookAlias {
        Simple(String),
        Permutations(Vec<BookVecOrAlias>),
    }

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum BookVecOrAlias {
        Alias(String),
        Vec(Vec<String>),
    }

    impl From<BibleConfig> for super::BibleConfig {
        fn from(val: BibleConfig) -> Self {
            let mut book_aliases = HashMap::new();
            for (book, aliases) in val.book_aliases.books {
                for alias in aliases {
                    alias.permute(&val.book_aliases.common, |alias| {
                        book_aliases.insert(
                            UniCase::new(Cow::Owned(
                                normalize_str(Cow::Owned(alias), val.search.languages.as_deref())
                                    .0
                                    .into_owned(),
                            )),
                            book,
                        );
                    });
                }
            }

            let search = super::SearchConfig::from(val.search);
            let tokenizer = search.create_tokenizer();

            super::BibleConfig {
                display_name: val.display_name,
                text_direction: val.text_direction,
                book_order: val.book_order,
                book_aliases,
                footnotes: val
                    .footnotes
                    .into_iter()
                    .map(|(key, footnotes)| {
                        (
                            tokenizer
                                .tokenize(&key)
                                .map(|t| t.lemma.into_owned())
                                .collect_vec(),
                            footnotes
                                .into_iter()
                                .flat_map(|mut footnote| {
                                    let ranges_count = footnote.bible_ranges.len();
                                    mem::take(&mut footnote.bible_ranges)
                                        .into_iter()
                                        .map(Into::into)
                                        .zip(iter::repeat_n(footnote.into(), ranges_count))
                                })
                                .collect(),
                        )
                    })
                    .filter(|(key, _)| !key.is_empty())
                    .collect(),
                search,
            }
        }
    }

    impl From<SearchConfig> for super::SearchConfig {
        fn from(val: SearchConfig) -> Self {
            super::SearchConfig {
                ignored_words: val.ignored_words.map(|words| {
                    fst::Set::from_iter(
                        words
                            .into_iter()
                            .map(|x| {
                                let mut detect = StrDetection::new(&x, val.languages.as_deref());
                                let script = detect.script();
                                let mut language = detect.language();
                                if language == Some(Language::Pes) {
                                    // Bypass PersianNormalizer, as it happens after Classifier
                                    language = None;
                                }
                                Token {
                                    lemma: Cow::Owned(x),
                                    script,
                                    language,
                                    ..Default::default()
                                }
                                .normalize(&NormalizerOption::default())
                                .lemma
                                .into_owned()
                            })
                            .sorted_unstable()
                            .dedup(),
                    )
                    .unwrap()
                    .map_data(Vec::into_boxed_slice)
                    .unwrap()
                }),
                languages: val.languages.map(Vec::into_boxed_slice),
            }
        }
    }

    impl From<FootnotesConfig> for super::FootnotesConfig {
        fn from(value: FootnotesConfig) -> Self {
            super::FootnotesConfig {
                footnote: value.footnote,
            }
        }
    }

    impl From<BibleRange> for RangeInclusive<BibleReference> {
        fn from(value: BibleRange) -> Self {
            match value {
                BibleRange::Simple(reference) => reference.split_to_range(),
                BibleRange::MultiChapter(start, end) => {
                    *start.split_to_range().start()..=*end.split_to_range().end()
                }
            }
        }
    }

    impl BookAlias {
        fn permute(
            self,
            common_aliases: &HashMap<String, Vec<String>>,
            mut handler: impl FnMut(String),
        ) {
            let get_alias = |alias| {
                common_aliases
                    .get(alias)
                    .unwrap_or_else(|| panic!("Unknown alias '{alias}'"))
            };
            match self {
                Self::Simple(alias) => handler(alias),
                Self::Permutations(groups) if groups.len() != 1 => {
                    let groups = groups
                        .iter()
                        .map(|x| {
                            match x {
                                BookVecOrAlias::Alias(alias) => get_alias(alias),
                                BookVecOrAlias::Vec(vec) => vec,
                            }
                            .iter()
                            .collect_vec()
                        })
                        .collect_vec();
                    let groups = groups.iter().map(|x| x.as_slice()).collect_vec();
                    let mut permutator = Permutator::new(&groups);
                    let empty_string = "".to_string();
                    let mut current_groups = vec![&empty_string; groups.len()];
                    while permutator.next_with_buffer(&mut current_groups) {
                        let mut new_alias = String::new();
                        for alias in &current_groups {
                            new_alias.push_str(alias);
                        }
                        handler(new_alias);
                    }
                }
                Self::Permutations(group) => match &group[0] {
                    BookVecOrAlias::Alias(alias_group) => {
                        for alias in get_alias(alias_group) {
                            handler(alias.clone());
                        }
                    }
                    BookVecOrAlias::Vec(aliases) => {
                        for alias in aliases {
                            handler(alias.clone());
                        }
                    }
                },
            }
        }
    }
}
