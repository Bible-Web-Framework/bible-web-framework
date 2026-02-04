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
use rayon::prelude::{IntoParallelIterator, ParallelBridge, ParallelIterator};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use std::{fs, io, path};
use sync_file::SyncFile;
use thiserror::Error;
use unicase::UniCase;
use zip::ZipArchive;
use zip::result::ZipError;

pub struct MultiBibleData {
    pub root_dir: PathBuf,
    pub bibles: DashMap<String, BibleData>,
}

#[derive(Default)]
pub struct BibleData {
    pub source: PathBuf,
    pub source_is_zip: bool,
    pub id: String,
    pub config: BibleConfig,
    pub files: DashMap<Book, UsjContent>,
    pub index: RwLock<BibleIndex>,
    sources: RwLock<BiMap<Book, String>>,
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
    pub languages: Vec<Language>,
    pub ignored_words: fst::Set<Vec<u8>>,
}

impl MultiBibleData {
    pub fn load(bibles_dir: PathBuf) -> ConfigResult<Self> {
        let bibles = DashMap::new();
        for entry in fs::read_dir(&bibles_dir)? {
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
                    fs::metadata(&path)?.is_file()
                }
            };
            let data = if is_file {
                BibleData::load_from_zip(path)?
            } else {
                BibleData::load_from_dir(path)?
            };
            data.update_index(ReindexType::FullReindex);
            bibles.insert(data.id.clone(), data);
        }
        Ok(Self {
            root_dir: bibles_dir,
            bibles,
        })
    }

    pub fn get_or_api_error(&self, bible: String) -> ApiResult<Ref<'_, String, BibleData>> {
        self.bibles.get(&bible).ok_or(ApiError::UnknownBible(bible))
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
            config: BibleConfig::from_reader(File::open(&config_path).map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    ConfigError::MissingFile(config_path)
                } else {
                    e.into()
                }
            })?)?,
            source: path,
            source_is_zip: false,
            ..Default::default()
        };
        data.reload_all(None)?;
        Ok(data)
    }

    pub fn load_from_zip(path: PathBuf) -> ConfigResult<Self> {
        let mut zip_file = ZipArchive::new(SyncFile::open(&path)?)?;
        let data = BibleData {
            id: path
                .file_stem()
                .unwrap_or_else(|| {
                    path.file_name()
                        .expect("BibleData::load_from_zip called with non-file path")
                })
                .to_string_lossy()
                .to_string(),
            config: BibleConfig::from_reader(zip_file.by_name(Self::CONFIG_PATH).map_err(
                |e| {
                    if matches!(e, ZipError::FileNotFound) {
                        ConfigError::MissingInZip(path.clone())
                    } else {
                        e.into()
                    }
                },
            )?)?,
            source: path,
            source_is_zip: true,
            ..Default::default()
        };
        data.reload_all(Some(zip_file))?;
        Ok(data)
    }

    pub fn book_parse_options(&self) -> BookParseOptions<'_, impl Fn(Book) -> bool> {
        BookParseOptions {
            additional_aliases: Some(&self.config.book_aliases),
            book_allowed: |book| self.files.contains_key(&book),
        }
    }

    fn update_index(&self, reindex_type: ReindexType) {
        self.index.write().unwrap().update_index(
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
                    self.sources.write().unwrap().insert(book.book, source)
                {
                    self.files.remove(&book);
                    self.update_index(ReindexType::Unindex(book));
                }
            }
            Entry::Occupied(mut e) => {
                let sources = self.sources.read().unwrap();
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

    fn insert_from_file_or_warn(
        &mut self,
        filename: String,
        reader: Result<impl Read, impl Into<UsjLoadError>>,
    ) -> Option<UsjBookInfo> {
        self.load_us_or_warn(&filename, reader)
            .and_then(|usj| self.insert_or_warn(usj, filename))
    }

    fn reload_all(&self, source_zip: Option<ZipArchive<SyncFile>>) -> ConfigResult<()> {
        let _lock = self
            .full_reload_active
            .lock()
            .expect("BibleData::reload_all called while reload active");
        self.files.clear();
        self.sources.write().unwrap().clear();
        self.has_ignored_files.store(false, Ordering::Relaxed);
        let start = Instant::now();
        let files = if !self.source_is_zip {
            walkdir::WalkDir::new(&self.source)
                .follow_links(true) // Eh, sure why not
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
                .collect::<walkdir::Result<Vec<_>>>()?
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
        builder.allow_list(&self.languages);
        builder.stop_words(&self.ignored_words);
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
        book_aliases: AliasesConfig,
        search: SearchConfig,
    }

    #[serde_as]
    #[derive(Debug, Deserialize)]
    pub struct SearchConfig {
        #[serde_as(as = "Vec<LanguageAsCode>")]
        pub languages: Vec<Language>,
        #[serde(default)]
        pub ignored_words: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
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
                ignored_words: fst::Set::from_iter(
                    val.ignored_words
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
                .unwrap(),
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
