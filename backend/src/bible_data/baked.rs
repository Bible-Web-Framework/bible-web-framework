use crate::bible_data::config::BibleConfig;
use crate::bible_data::expanded::{ConfigResult, ExpandedBibleData};
use crate::bible_data::{BibleData, MultiBibleData};
use crate::book_data::Book;
use crate::index::ArchivedIndexedWord;
use crate::usj::content::UsjContent;
use crate::usj::loader::USJ_VERSION;
use crate::usj::root::UsjRoot;
use crate::usj::{ParaIndex, TranslatedBookInfo};
use crate::utils::print_memory_stats;
use crate::utils::serde_as::UniCaseAs;
use crate::verse_range::VerseRange;
use enum_map::{Enum, EnumMap};
use memmap2::{Mmap, MmapAsRawDesc};
use oxicode::config::{legacy, standard};
use rkyv::api::serialize_using;
use rkyv::ser::sharing::Share;
use rkyv::ser::writer::IoWriter;
use rkyv::util::with_arena;
use rkyv::{Portable, rancor};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::io::{Seek, SeekFrom, Write};
use std::num::{NonZeroU8, NonZeroUsize};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;
use strum::VariantArray;
use thiserror::Error;
use trie_rs::map::{Trie, TrieBuilder};
use unicase::UniCase;

type BakeVersion = [u8; 20];
const BAKE_VERSION: BakeVersion =
    match const_hex::const_decode_to_array(env!("GIT_COMMIT_HASH").as_bytes()) {
        Ok(version) => version,
        Err(_) => panic!("Invalid GIT_COMMIT_HASH"),
    };

#[derive(Debug, Error)]
pub enum BakeError {
    #[error("General I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Config serialization error: {0}")]
    Cbor(#[from] serde_cbor::Error),
    #[error("General serialization error: {0}")]
    Oxicode(#[from] oxicode::Error),
    #[error("Index serialization error: {0}")]
    Rancor(#[from] rancor::Error),
    #[error("Baked file exceeds maximum size of 4 GB.")]
    FileTooBig,
    #[error(
        "Incorrect baked file version. {} is required, but {} was found.",
        const_hex::encode(BAKE_VERSION),
        const_hex::encode(.0),
    )]
    VersionMismatch(BakeVersion),
    #[error("Out of bounds chapter {1} for {0}. Only up through {0} {2} is supported.")]
    OutOfBoundsChapter(Book, NonZeroU8, usize),
}

pub type BakeResult<T> = Result<T, BakeError>;

pub fn bake_bible<W: Write + Seek>(bible: &ExpandedBibleData, mut output: W) -> BakeResult<()> {
    output.write_all(&BAKE_VERSION)?;

    bible
        .config
        .read()
        .serialize(&mut serde_cbor::Serializer::new(
            serde_cbor::ser::IoWrite::new(&mut output),
        ))?;

    #[serde_as]
    #[derive(Serialize)]
    struct NamesMap(
        #[serde_as(as = "HashMap<UniCaseAs<_>, _>")] HashMap<UniCase<Cow<'static, str>>, Book>,
    );
    oxicode::serde::encode_into_std_write(
        &NamesMap(
            bible
                .books
                .iter()
                .flat_map(|book_data| {
                    let book = *book_data.key();
                    book_data
                        .names
                        .clone()
                        .into_iter()
                        .map(move |name| (name, book))
                })
                .collect(),
        ),
        &mut output,
        standard(),
    )?;

    write_with_addresses(&mut output, Book::LENGTH, |mut output, book_addresses| {
        for (idx, book) in Book::VARIANTS.iter().enumerate() {
            let Some(data) = bible.books.get(book) else {
                continue;
            };
            let usj = data.usj.unwrap_root();
            let start_address = output.stream_position()?;
            book_addresses[idx] = start_address as u32;

            oxicode::serde::encode_into_std_write(
                &data.usj.unwrap_root().translated_book_info(),
                &mut output,
                standard(),
            )?;

            write_address(&mut output, usj.content.len() as u32)?;
            for element in &usj.content {
                oxicode::encode_into_std_write(Cow::Borrowed(element), &mut output, standard())?;
            }
        }
        Ok(())
    })?;

    static_assertions::assert_impl_all!(ArchivedIndexedWord: Portable);
    let index = bible.index.read();
    let mut symbols = vec![];
    let mut interner_trie = {
        let mut builder = TrieBuilder::new();
        for (symbol, lemma) in index.iter_lemmas_and_ids() {
            symbols.push((symbol, lemma));
            builder.push(lemma, 0u32);
        }
        builder.build()
    };
    let trie_start = output.stream_position()?;
    // legacy() because we need fixed-size integers because we'll be changing the values later
    let original_trie_size =
        oxicode::serde::encode_into_std_write(&interner_trie, &mut output, legacy())?;

    with_arena(|arena| {
        let data_start = output.stream_position()?;
        let mut serializer = rkyv::ser::Serializer::new(
            IoWriter::with_pos(&mut output, data_start as usize),
            arena.acquire(),
            Share::new(),
        );
        for (symbol, lemma) in symbols {
            let word = index.word_from_symbol(symbol).unwrap();
            let address = serialize_using::<_, rancor::Error>(word, &mut serializer)?;
            *interner_trie.exact_match_mut(lemma).unwrap() = address as u32;
        }
        BakeResult::Ok(())
    })?;

    output.seek(SeekFrom::Start(trie_start))?;
    let new_trie_size =
        oxicode::serde::encode_into_std_write(&interner_trie, &mut output, legacy())?;
    let final_size = output.seek(SeekFrom::End(0))?;
    assert_eq!(original_trie_size, new_trie_size);

    if final_size > u32::MAX as u64 {
        return Err(BakeError::FileTooBig);
    }

    Ok(())
}

fn write_with_addresses<W, F>(mut output: W, count: usize, action: F) -> BakeResult<()>
where
    W: Write + Seek,
    F: FnOnce(&mut W, &mut Vec<u32>) -> BakeResult<()>,
{
    let mut addresses = vec![0; count];
    let addresses_address = output.stream_position()?;
    output.seek_relative(count as i64 * 4)?;
    action(&mut output, &mut addresses)?;
    output.seek(SeekFrom::Start(addresses_address))?;
    for address in addresses {
        write_address(&mut output, address)?
    }
    output.seek(SeekFrom::End(0))?;
    Ok(())
}

fn write_address(mut output: impl Write, address: u32) -> BakeResult<()> {
    output.write_all(&address.to_le_bytes())?;
    Ok(())
}

pub struct MultiBakedBibleData {
    pub default_bible: String,
    pub bibles: HashMap<String, BakedBibleData>,
}

impl MultiBakedBibleData {
    pub fn load(
        bibles_dir: PathBuf,
        default_bible: String,
        disabled_bibles: HashSet<String>,
    ) -> BakeResult<Self> {
        let mut bibles = HashMap::new();
        let start = Instant::now();
        for entry in bibles_dir.read_dir()? {
            let entry = entry?;
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            let Some(bible_id) = file_name.strip_suffix(".dat") else {
                tracing::info!("Skipping non-baked bible file {file_name}");
                continue;
            };
            if disabled_bibles.contains(bible_id) {
                tracing::info!("Skipping loading disabled bible {bible_id}");
                continue;
            }
            tracing::info!("Loading baked bible {bible_id}");
            let bible = File::open(entry.path())
                .map_err(BakeError::Io)
                .and_then(|f| load_baked_bible(&f))
                .inspect_err(|_| {
                    tracing::error!("Error while loading bible {bible_id}");
                })?;
            bibles.insert(bible_id.to_string(), bible);
        }
        tracing::info!(
            "Loaded {} baked bibles in {:?}",
            bibles.len(),
            start.elapsed(),
        );
        print_memory_stats();
        Ok(Self {
            default_bible,
            bibles,
        })
    }
}

impl MultiBibleData for MultiBakedBibleData {
    fn default_bible(&self) -> &str {
        &self.default_bible
    }

    fn bibles(&self) -> Vec<String> {
        self.bibles.keys().cloned().collect()
    }

    fn get_bible(&self, bible: &str) -> Option<BibleData<'_>> {
        self.bibles.get(bible).map(BibleData::Baked)
    }
}

pub struct BakedBibleData {
    memory: Mmap,
    pub config: BibleConfig,
    pub full_book_names: HashMap<UniCase<Cow<'static, str>>, Book>,
    pub books: EnumMap<Book, Option<BakedBookData>>,
    index_trie: Trie<u8, u32>,
}

#[derive(Debug)]
pub struct BakedBookData {
    usj_address_range: Range<usize>,
    usj_len: usize,
    pub translated_book_info: TranslatedBookInfo<'static>,
    chapter_address_indices: Vec<Option<(NonZeroUsize, usize)>>,
}

pub fn load_baked_bible<S: MmapAsRawDesc>(source: S) -> BakeResult<BakedBibleData> {
    let memory = unsafe { Mmap::map(source) }?;
    if memory.len() > u32::MAX as usize {
        return Err(BakeError::FileTooBig);
    }

    let version = &memory[..20];
    if version != BAKE_VERSION {
        return Err(BakeError::VersionMismatch(*version.as_array().unwrap()));
    }
    let mut address = 20;

    let mut config_deserializer = serde_cbor::Deserializer::from_slice(&memory[20..]);
    let config = BibleConfig::deserialize(&mut config_deserializer)?;
    address += config_deserializer.byte_offset();

    #[serde_as]
    #[derive(Deserialize)]
    struct NamesMap(
        #[serde_as(as = "HashMap<UniCaseAs<_>, _>")] HashMap<UniCase<Cow<'static, str>>, Book>,
    );
    let (NamesMap(full_book_names), names_len) =
        oxicode::serde::decode_owned_from_slice(&memory[address..], standard())?;
    address += names_len;

    let (book_addresses, addresses_len) = read_addresses(&memory, address, Book::LENGTH)?;
    address += addresses_len;

    let mut end_address = address;
    let mut books = EnumMap::default();
    for (idx, &book) in Book::VARIANTS.iter().enumerate() {
        let mut address = book_addresses[idx];
        if address == 0 {
            continue;
        }

        let (translated_book_info, i18n_len) =
            oxicode::serde::decode_owned_from_slice(&memory[address..], standard())?;
        address += i18n_len;

        let usj_len = read_address(&memory, address)?;
        address += 4;

        let usj_start_address = address;
        let mut chapter_address_indices = vec![None; chapter_count(book)];
        let mut last_encountered_chapter = 0;
        for i in 0..usj_len {
            let (element, element_len) = oxicode::decode_from_slice(&memory[address..])?;
            if let UsjContent::Chapter { number, .. } = element
                && number.value.get() > last_encountered_chapter
            {
                if let Some(address_index) =
                    chapter_address_indices.get_mut(number.value.get() as usize - 1)
                {
                    *address_index = Some((NonZeroUsize::new(address).unwrap(), i));
                    last_encountered_chapter = number.value.get();
                } else {
                    return Err(BakeError::OutOfBoundsChapter(
                        book,
                        number.value,
                        chapter_count(book),
                    ));
                }
            }
            address += element_len;
        }

        books[book] = Some(BakedBookData {
            usj_address_range: usj_start_address..address,
            usj_len,
            translated_book_info,
            chapter_address_indices,
        });
        end_address = address;
    }

    let index_trie = oxicode::serde::decode_owned_from_slice(&memory[end_address..], legacy())?.0;

    // TODO: Validate index trie and index data

    Ok(BakedBibleData {
        memory,
        config,
        full_book_names,
        books,
        index_trie,
    })
}

impl BakedBibleData {
    fn load_usj_in_range(&self, range: Range<usize>, len: usize) -> UsjRoot {
        let mut data = &self.memory[range];
        let mut content = Vec::with_capacity(len);
        while !data.is_empty() {
            let (element, length) = oxicode::decode_from_slice(data).unwrap();
            content.push(element);
            data = &data[length..];
        }
        UsjRoot {
            version: Cow::Borrowed(USJ_VERSION),
            content,
        }
    }
}

impl BakedBookData {
    pub fn load_full_usj(&self, bible: &BakedBibleData) -> UsjContent {
        UsjContent::Root(bible.load_usj_in_range(self.usj_address_range.clone(), self.usj_len))
    }

    pub fn list_chapter_usjs(&self, bible: &BakedBibleData) -> impl Iterator<Item = UsjContent> {
        self.chapter_address_indices
            .iter()
            .filter_map(|address| *address)
            .map(|(address, _)| oxicode::decode_value(&bible.memory[address.get()..]).unwrap())
    }

    pub fn find_reference(
        &self,
        bible: &BakedBibleData,
        chapter: NonZeroU8,
        verse_range: VerseRange,
    ) -> Option<(ParaIndex, Vec<UsjContent>)> {
        let (base_address, base_index) = (*self
            .chapter_address_indices
            .get(chapter.get() as usize - 1)?)?;
        let base_address = base_address.get();
        let (next_address, next_index) = {
            let mut index = chapter.get() as usize;
            loop {
                match self.chapter_address_indices.get(index) {
                    Some(Some((address, index))) => break (address.get(), *index),
                    Some(None) => index += 1,
                    None => break (self.usj_address_range.end, index),
                }
            }
        };
        let loaded_usj =
            bible.load_usj_in_range(base_address..next_address, next_index - base_index);
        let (para_index, content) = loaded_usj.find_reference(chapter, verse_range)?;
        Some(((para_index.0 + base_index, para_index.1), content))
    }

    pub fn has_chapter(&self, chapter: NonZeroU8) -> bool {
        self.chapter_address_indices
            .get(chapter.get() as usize - 1)
            .is_some_and(Option::is_some)
    }
}

fn read_addresses(
    input: &[u8],
    offset: usize,
    count: usize,
) -> Result<(Vec<usize>, usize), io::Error> {
    let mut addresses = Vec::with_capacity(count);
    for i in 0..count {
        addresses.push(read_address(input, offset + (4 * i))?);
    }
    Ok((addresses, offset + (4 * count)))
}

fn read_address(input: &[u8], offset: usize) -> Result<usize, io::Error> {
    let offset = offset;
    let subslice = input
        .get(offset..offset + 4)
        .ok_or(io::ErrorKind::UnexpectedEof)?;
    Ok(u32::from_le_bytes(*subslice.as_array().unwrap()) as usize)
}

#[inline]
fn chapter_count(book: Book) -> usize {
    book.chapter_count().map_or(0, NonZeroU8::get) as usize
}
