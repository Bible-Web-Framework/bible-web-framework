use crate::api::{ApiError, ApiResult};
use crate::book_data::{Book, BookParseOptions};
use crate::index::{BibleIndex, ReindexType};
use crate::usj::{UsjBookInfo, UsjContent, UsjLoadError, UsjRoot, load_usj, load_usj_from_usfm};
use crate::utils::ExclusiveMutex;
use bimap::{BiMap, Overwritten};
use charabia::{Language, Tokenizer, TokenizerBuilder};
use dashmap::mapref::one::Ref;
use dashmap::{DashMap, Entry};
use miette::{GraphicalReportHandler, NamedSource, Severity};
use notify_debouncer_full::notify;
use notify_debouncer_full::notify::EventKind;
use notify_debouncer_full::notify::event::{
    CreateKind, EventAttributes, ModifyKind, RemoveKind, RenameMode,
};
use parking_lot::RwLock;
use rayon::prelude::{IntoParallelIterator, ParallelBridge, ParallelIterator};
use smallvec::smallvec;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, LinkedList};
use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
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
    pub bibles: DashMap<Cow<'static, str>, BibleData>,
    file_change_active: ExclusiveMutex,
}

#[derive(Default)]
pub struct BibleData {
    pub source: PathBuf,
    pub source_is_zip: bool,
    pub id: String,
    pub config: BibleConfig,
    pub files: DashMap<Book, UsjContent>,
    pub index: RwLock<BibleIndex>,
    sources: RwLock<BiMap<Book, Cow<'static, str>>>,
    has_ignored_files: AtomicBool,
    full_reload_active: ExclusiveMutex,
}

#[derive(Debug, Default)]
pub struct BibleConfig {
    pub book_aliases: HashMap<UniCase<Cow<'static, str>>, Book>,
    pub search: SearchConfig,
}

#[derive(Debug, Default)]
pub struct SearchConfig {
    pub languages: Option<Vec<Language>>,
    pub ignored_words: Option<fst::Set<Vec<u8>>>,
}

impl MultiBibleData {
    pub fn load(bibles_dir: PathBuf) -> ConfigResult<Self> {
        let result = Self {
            root_dir: bibles_dir,
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

    // TODO: Handle config file edits too
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
                        tracing::info!("Loading new bible from {}", path.display());
                        let data = BibleData::load_from_zip(path.to_owned())?;
                        self.bibles.insert(Cow::Owned(data.id.clone()), data);
                    } else {
                        let bible_id = get_root_bible_id(trimmed_path);
                        if let Some(bible) = self.bibles.get(&*bible_id)
                            && let Some(book) = bible
                                .insert_from_file_or_warn(path, get_inside_bible_path(trimmed_path))
                        {
                            tracing::info!("Loaded new book {book} from {}", path.display());
                            bible.update_index(ReindexType::PartialReindex(smallvec![book.book]));
                            return Ok(());
                        }
                    }
                } else if trimmed_path.components().nth(1).is_none() {
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
                        .and_then(|(b, _)| bible.files.remove(&b))
                        .and_then(|(_, b)| b.unwrap_root().book_info());
                    if let Some(new_book) = bible.insert_from_file_or_warn(path, inside_path) {
                        let reindex_type = if let Some(old_book) = old_book
                            && new_book != old_book
                        {
                            tracing::info!(
                                "Loaded book {new_book} in {bible_id} from {} (was {old_book})",
                                path.display()
                            );
                            ReindexType::PartialReindex(smallvec![old_book.book, new_book.book])
                        } else {
                            tracing::info!(
                                "Loaded book {new_book} in {bible_id} from {}",
                                path.display()
                            );
                            ReindexType::PartialReindex(smallvec![new_book.book])
                        };
                        bible.update_index(reindex_type);
                    }
                } else {
                    tracing::info!("Reloading bible from {}", path.display());
                    bible.reload_all(None)?;
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
                            self.bibles.insert(Cow::Owned(new_id.into_owned()), bible);
                        }
                    }
                    (true, true) => {
                        let old_bible = get_root_bible_id(old_trimmed_path);
                        let new_bible = get_root_bible_id(new_trimmed_path);
                        let old_inside_path = get_inside_bible_path(old_trimmed_path);
                        let new_inside_path = get_inside_bible_path(new_trimmed_path);
                        let Some(old_bible_data) = self.bibles.get(&*old_bible) else {
                            return Ok(());
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
                                old_bible_data.files.remove(&book).unwrap().1,
                                new_inside_path.into_owned(),
                            );
                            old_bible_data.update_index(ReindexType::Unindex(book));
                        }
                    }
                    _ => {
                        let mut paths = mem::take(&mut event.paths);
                        self.handle_file_change(notify::Event {
                            kind: EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                            paths: vec![paths.remove(0)],
                            attrs: EventAttributes::default(),
                        })?;
                        self.handle_file_change(notify::Event {
                            kind: EventKind::Modify(ModifyKind::Name(RenameMode::To)),
                            paths,
                            attrs: EventAttributes::default(),
                        })?;
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
                        bible.files.remove(&book);
                        tracing::info!(
                            "Removed book {book} from {bible_id} source from {}",
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

    pub fn reload_everything(&self) -> ConfigResult<()> {
        let _lock = self
            .file_change_active
            .lock()
            .expect("MultiBibleData::reload_everything called while already reloading");

        let mut keys_to_keep = (!self.bibles.is_empty()).then(HashSet::<Cow<str>>::new);
        for entry in self.root_dir.read_dir()? {
            let entry = entry?;
            let path = entry.path();
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
}

macro_rules! format_source {
    ($self:ident) => {
        format_args!("{}{}", $self.source.display(), path::MAIN_SEPARATOR)
    };
}

impl BibleData {
    const CONFIG_PATH: &str = "bible.toml";

    pub fn load_from_dir(path: PathBuf) -> ConfigResult<Self> {
        let config_path = path.join(Self::CONFIG_PATH);
        let data = BibleData {
            id: path
                .file_name()
                .expect("BibleData::load_from_dir called with .. path")
                .to_string_lossy()
                .into_owned(),
            config: match File::open(&config_path) {
                Ok(file) => BibleConfig::from_reader(file)?,
                Err(e) if e.kind() == ErrorKind::NotFound => BibleConfig::default(),
                Err(e) => return Err(e.into()),
            },
            source: path,
            source_is_zip: false,
            ..Default::default()
        };
        data.reload_all(None)?;
        data.finish_load();
        Ok(data)
    }

    pub fn load_from_zip(path: PathBuf) -> ConfigResult<Self> {
        let mut zip_file = ZipArchive::new(SyncFile::open(&path)?)?;
        let data = BibleData {
            id: Self::get_file_bible_id(&path).into_owned(),
            config: match zip_file.by_name(Self::CONFIG_PATH) {
                Ok(file) => BibleConfig::from_reader(file)?,
                Err(ZipError::FileNotFound) => BibleConfig::default(),
                Err(e) => return Err(e.into()),
            },
            source: path,
            source_is_zip: true,
            ..Default::default()
        };
        data.reload_all(Some(zip_file))?;
        data.finish_load();
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

    fn finish_load(&self) {
        let mut index = self.index.write();
        index.log_marker = Some(self.id.clone());
        index.update_index(
            ReindexType::FullReindex,
            &self.files,
            &self.config.search.create_tokenizer(),
        );
    }

    pub fn book_parse_options(&self) -> BookParseOptions<'_, impl Fn(Book) -> bool> {
        BookParseOptions {
            additional_aliases: Some(&self.config.book_aliases),
            book_allowed: |book| self.files.contains_key(&book),
        }
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
            &self.files,
            &self.config.search.create_tokenizer(),
        );
    }

    fn insert_or_warn(&self, usj: UsjContent, source: String) -> Option<UsjBookInfo> {
        let Some(book) = usj.as_root().and_then(UsjRoot::book_info) else {
            tracing::error!(
                "Book at {}{source} missing root element or book identifier",
                format_source!(self),
            );
            return None;
        };
        match self.files.entry(book.book) {
            Entry::Vacant(e) => {
                e.insert(usj);
                if let Overwritten::Right(book, _) =
                    self.sources.write().insert(book.book, Cow::Owned(source))
                {
                    self.files.remove(&book);
                    self.update_index(ReindexType::Unindex(book));
                }
            }
            Entry::Occupied(mut e) => {
                let sources = self.sources.read();
                let old_path = sources.get_by_left(&book.book).unwrap();
                if &source == old_path {
                    e.insert(usj);
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
        Some(book)
    }

    fn load_us_or_warn(
        &self,
        filename: &str,
        reader: Result<impl Read, impl Into<UsjLoadError>>,
    ) -> Option<UsjContent> {
        match filename
            .split('.')
            .next_back()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("usj") => reader
                .map_err(Into::into)
                .map(BufReader::new)
                .and_then(load_usj)
                .inspect_err(|err| {
                    tracing::error!("Failed to load {}{filename}: {err}", format_source!(self));
                })
                .ok(),
            Some("usfm" | "sfm") => {
                let mut usfm = String::new();
                let usj = reader
                    .map_err(Into::into)
                    .and_then(|mut reader| {
                        reader.read_to_string(&mut usfm).map_err(UsjLoadError::Io)
                    })
                    .and_then(|_| load_usj_from_usfm(usfm))
                    .inspect_err(|err| {
                        tracing::error!("Failed to load {}{filename}: {err}", format_source!(self));
                    })
                    .ok()?;
                if !usj.diagnostics.is_empty() {
                    let mut diag_message = String::new();
                    let source_code = Arc::new(NamedSource::new(filename, usj.source));
                    let reporter = GraphicalReportHandler::new();
                    let is_all_error = usj
                        .diagnostics
                        .iter()
                        .all(|x| x.severity.unwrap_or_default() == Severity::Error);
                    for diag in usj.diagnostics {
                        let report =
                            miette::Report::new(diag).with_source_code(source_code.clone());
                        diag_message.push('\n');
                        let _ = reporter.render_report(&mut diag_message, &*report);
                    }
                    if is_all_error {
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
                Some(usj.usj)
            }
            Some(_) | None => {
                tracing::warn!("Found non-USFM/USJ file {}{filename}", format_source!(self));
                None
            }
        }
    }

    fn insert_from_file_or_warn(&self, path: &Path, filename: Cow<'_, str>) -> Option<UsjBookInfo> {
        self.load_us_or_warn(&filename, File::open(path))
            .and_then(|usj| self.insert_or_warn(usj, filename.into_owned()))
    }

    fn reload_all(&self, source_zip: Option<ZipArchive<SyncFile>>) -> ConfigResult<()> {
        let _lock = self
            .full_reload_active
            .lock()
            .expect("BibleData::reload_all called while reload active");
        self.files.clear();
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
                    self.load_us_or_warn(&rel_path, File::open(entry.path()))
                        .map(|usj| Ok((usj, rel_path.into_owned())))
                })
                .collect::<walkdir::Result<LinkedList<_>>>()?
        } else {
            #[allow(unused_qualifications)] // It is, in fact, used
            let zip = source_zip.map_or_else(
                || ConfigResult::Ok(ZipArchive::new(SyncFile::open(&self.source)?)?),
                Ok,
            )?;
            (0..zip.len())
                .into_par_iter()
                .filter_map(|index| {
                    let mut zip = zip.clone();
                    let filename = zip.name_for_index(index).unwrap();
                    if filename == Self::CONFIG_PATH {
                        return None;
                    }
                    if filename
                        .chars()
                        .next_back()
                        .is_some_and(|c| c == '/' || c == '\\')
                    {
                        // Is a directory
                        return None;
                    }
                    let filename = filename.to_owned();
                    self.load_us_or_warn(&filename, zip.by_index(index))
                        .map(|usj| (usj, filename))
                })
                .collect()
        };
        for (usj, source) in files {
            self.insert_or_warn(usj, source);
        }
        tracing::info!(
            "Loaded {} USFM/USJ files from {} in {:?}",
            self.files.len(),
            self.source.display(),
            start.elapsed()
        );
        Ok(())
    }
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
    fn create_tokenizer(&self) -> Tokenizer<'_> {
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

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Directory walk error: {0}")]
    WalkDir(#[from] walkdir::Error),
    #[error("Zip file error: {0}")]
    Zip(#[from] ZipError),
    #[error("Missing bible config file {0}")]
    MissingFile(PathBuf),
    #[error("Missing {file} in {0}", file = BibleData::CONFIG_PATH)]
    MissingInZip(PathBuf),
    #[error("Error reading {file}: {0}", file = BibleData::CONFIG_PATH)]
    Parse(#[from] toml::de::Error),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

mod unresolved {
    use crate::book_data::Book;
    use crate::utils::LanguageAsCode;
    use charabia::normalizer::NormalizerOption;
    use charabia::{Language, Normalize, Token};
    use itertools::Itertools;
    use permutate::Permutator;
    use serde::Deserialize;
    use serde_with::serde_as;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use unicase::UniCase;

    #[derive(Debug, Deserialize)]
    pub struct BibleConfig {
        #[serde(default)]
        book_aliases: AliasesConfig,
        #[serde(default)]
        search: SearchConfig,
    }

    #[serde_as]
    #[derive(Debug, Default, Deserialize)]
    pub struct SearchConfig {
        #[serde_as(as = "Option<Vec<LanguageAsCode>>")]
        #[serde(default)]
        pub languages: Option<Vec<Language>>,
        #[serde(default)]
        pub ignored_words: Option<Vec<String>>,
    }

    #[derive(Debug, Default, Deserialize)]
    struct AliasesConfig {
        #[serde(default)]
        common: HashMap<String, Vec<String>>,
        books: HashMap<Book, Vec<BookAlias>>,
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
                        book_aliases.insert(UniCase::new(Cow::Owned(alias)), book);
                    });
                }
            }

            super::BibleConfig {
                book_aliases,
                search: val.search.into(),
            }
        }
    }

    impl From<SearchConfig> for super::SearchConfig {
        fn from(val: SearchConfig) -> Self {
            super::SearchConfig {
                languages: val.languages,
                ignored_words: val.ignored_words.map(|words| {
                    fst::Set::from_iter(
                        words
                            .into_iter()
                            .sorted_unstable()
                            .map(|x| {
                                Token {
                                    lemma: Cow::Owned(x),
                                    ..Default::default()
                                }
                                .normalize(&NormalizerOption::default())
                                .lemma
                                .into_owned()
                            })
                            .dedup(),
                    )
                    .unwrap()
                }),
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
