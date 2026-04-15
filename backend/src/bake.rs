use crate::bible_data::{BibleConfig, BibleData};
use crate::book_data::Book;
use crate::index::ArchivedIndexedWord;
use crate::usj::content::UsjContent;
use crate::utils::serde_as::UniCaseAs;
use enum_map::{Enum, EnumMap};
use memmap2::{Mmap, MmapAsRawDesc};
use oxicode::config::{legacy, standard};
use rkyv::api::serialize_using;
use rkyv::ser::sharing::Share;
use rkyv::ser::writer::IoWriter;
use rkyv::util::with_arena;
use rkyv::{rancor, Portable};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::borrow::Cow;
use std::collections::HashSet;
use std::io;
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroU8;
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
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Cbor(#[from] serde_cbor::Error),
    #[error(transparent)]
    Oxicode(#[from] oxicode::Error),
    #[error(transparent)]
    Rancor(#[from] rancor::Error),
    #[error("Baked file exceeds maximum size of 4 GB.")]
    FileTooBig,
    #[error(
        "Incorrect baked file version. {} is required, but {} was found.",
        const_hex::encode(BAKE_VERSION),
        const_hex::encode(.0),
    )]
    VersionMismatch(BakeVersion),
}

pub fn bake_bible<W>(bible: &BibleData, mut output: W) -> Result<(), BakeError>
where
    W: Write + Seek,
{
    output.write_all(&BAKE_VERSION)?;

    bible.config.read().serialize(
        &mut serde_cbor::Serializer::new(serde_cbor::ser::IoWrite::new(&mut output)),
    )?;

    write_with_addresses(&mut output, Book::LENGTH, |mut output, book_addresses| {
        for (idx, book) in Book::VARIANTS.iter().enumerate() {
            let Some(data) = bible.books.get(book) else {
                continue;
            };
            let usj = data.usj.unwrap_root();
            let start_address = output.stream_position()?;
            book_addresses[idx] = start_address as u32;

            output.seek_relative(4)?;
            write_with_addresses(
                &mut output,
                chapter_count(book),
                |mut output, chapter_addresses| {
                    for element in &usj.content {
                        if let UsjContent::Chapter { number, .. } = element
                            && number.value.get() as usize <= chapter_count(book)
                        {
                            chapter_addresses[number.value.get() as usize - 1] =
                                output.stream_position()? as u32;
                        }
                        // Encode is implemented for Cow<'_, T> but not for &T
                        oxicode::encode_into_std_write(
                            Cow::Borrowed(element),
                            &mut output,
                            standard(),
                        )?;
                    }
                    Ok(())
                },
            )?;

            let names_address = output.stream_position()?;
            output.seek(SeekFrom::Start(start_address))?;
            output.write_all(&(names_address as u32).to_le_bytes())?;
            output.seek(SeekFrom::End(0))?;

            #[serde_as]
            #[derive(Serialize)]
            struct NamesSet<'a>(
                #[serde_as(as = "HashSet<UniCaseAs<_>>")] &'a HashSet<UniCase<Cow<'static, str>>>,
            );
            oxicode::serde::encode_into_std_write(&NamesSet(&data.names), &mut output, standard())?;
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
        <Result<(), BakeError>>::Ok(())
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

fn write_with_addresses<W, F>(mut output: W, count: usize, action: F) -> Result<(), BakeError>
where
    W: Write + Seek,
    F: FnOnce(&mut W, &mut Vec<u32>) -> Result<(), BakeError>,
{
    let mut addresses = vec![0; count];
    let addresses_address = output.stream_position()?;
    output.seek_relative(count as i64 * 4)?;
    action(&mut output, &mut addresses)?;
    output.seek(SeekFrom::Start(addresses_address))?;
    for address in addresses {
        output.write_all(&address.to_le_bytes())?;
    }
    output.seek(SeekFrom::End(0))?;
    Ok(())
}

#[derive(Debug)]
pub struct BakedBibleData {
    memory: Mmap,
    config: BibleConfig,
    books: EnumMap<Book, Option<BakedBookData>>,
    index_trie: Trie<u8, u32>,
}

#[derive(Debug)]
struct BakedBookData {
    usj_start_address: u32,
    chapter_addresses: Vec<u32>,
    names: HashSet<UniCase<Cow<'static, str>>>,
}

pub fn load_baked_bible<S: MmapAsRawDesc>(source: S) -> Result<BakedBibleData, BakeError> {
    let memory = unsafe { Mmap::map(source) }?;
    if memory.len() > u32::MAX as usize {
        return Err(BakeError::FileTooBig);
    }

    let version = &memory[..20];
    if version != BAKE_VERSION {
        return Err(BakeError::VersionMismatch(*version.as_array().unwrap()));
    }

    let mut config_deserializer = serde_cbor::Deserializer::from_slice(&memory[20..]);
    let config = BibleConfig::deserialize(&mut config_deserializer)?;
    let config_end_address = 20 + config_deserializer.byte_offset() as u32;

    let (book_addresses, mut end_address) =
        read_addresses(&memory, config_end_address, Book::LENGTH)?;
    let mut books = EnumMap::default();
    for (idx, book) in Book::VARIANTS.iter().enumerate() {
        let base_address = book_addresses[idx];
        if base_address == 0 {
            continue;
        }

        let names_address = read_address(&memory, base_address)?;
        let (chapter_addresses, _) =
            read_addresses(&memory, base_address + 4, chapter_count(book))?;

        #[serde_as]
        #[derive(Deserialize)]
        struct NamesSet(
            #[serde_as(as = "HashSet<UniCaseAs<_>>")] HashSet<UniCase<Cow<'static, str>>>,
        );
        let (NamesSet(names), names_len) = oxicode::serde::decode_owned_from_slice::<NamesSet, _>(
            &memory[names_address as usize..],
            standard(),
        )?;

        books[*book] = Some(BakedBookData {
            usj_start_address: base_address + 4,
            chapter_addresses,
            names,
        });
        end_address = names_address + names_len as u32;
    }

    let index_trie =
        oxicode::serde::decode_owned_from_slice(&memory[end_address as usize..], legacy())?.0;

    Ok(BakedBibleData {
        memory,
        config,
        books,
        index_trie,
    })
}

fn read_addresses(input: &[u8], offset: u32, count: usize) -> Result<(Vec<u32>, u32), io::Error> {
    let mut addresses = Vec::with_capacity(count);
    for i in 0..count {
        addresses.push(read_address(input, offset + (4 * i) as u32)?);
    }
    Ok((addresses, offset + (4 * count) as u32))
}

fn read_address(input: &[u8], offset: u32) -> Result<u32, io::Error> {
    let offset = offset as usize;
    let subslice = input
        .get(offset..offset + 4)
        .ok_or(io::ErrorKind::UnexpectedEof)?;
    Ok(u32::from_le_bytes(*subslice.as_array().unwrap()))
}

#[inline]
fn chapter_count(book: &Book) -> usize {
    book.chapter_count().map_or(0, NonZeroU8::get) as usize
}
