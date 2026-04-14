use crate::bible_data::BibleData;
use crate::book_data::Book;
use crate::index::ArchivedIndexedWord;
use crate::usj::content::UsjContent;
use crate::utils::serde_as::UniCaseAs;
use oxicode::config::{legacy, standard};
use rkyv::api::serialize_using;
use rkyv::ser::sharing::Share;
use rkyv::ser::writer::IoWriter;
use rkyv::util::with_arena;
use rkyv::{Portable, rancor};
use serde::Serialize;
use serde_with::serde_as;
use std::borrow::Cow;
use std::collections::HashSet;
use std::io;
use std::io::{Seek, SeekFrom, Write};
use std::num::NonZeroU8;
use strum::VariantArray;
use thiserror::Error;
use trie_rs::map::TrieBuilder;
use unicase::UniCase;

const BAKE_VERSION: [u8; 20] =
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
}

pub fn bake_bible<W>(bible: &BibleData, mut output: W) -> Result<(), BakeError>
where
    W: Write + Seek,
{
    output.write_all(&BAKE_VERSION)?;

    bible.config.read().serialize(
        &mut serde_cbor::Serializer::new(serde_cbor::ser::IoWrite::new(&mut output))
            .packed_format(),
    )?;

    write_with_addresses(
        &mut output,
        Book::VARIANTS.len(),
        |mut output, book_addresses| {
            for (idx, book) in Book::VARIANTS.iter().enumerate() {
                let Some(data) = bible.books.get(book) else {
                    continue;
                };
                let usj = data.usj.unwrap_root();
                let start_address = output.stream_position()?;
                book_addresses[idx] = start_address as u32;

                output.seek_relative(4)?;
                let chapter_count = book.chapter_count().map_or(0, NonZeroU8::get) as usize;
                write_with_addresses(
                    &mut output,
                    book.chapter_count().map_or(0, NonZeroU8::get) as usize,
                    |mut output, chapter_addresses| {
                        output.write_all(&(usj.content.len() as u32).to_le_bytes())?;
                        for element in &usj.content {
                            if let UsjContent::Chapter { number, .. } = element
                                && number.value.get() as usize <= chapter_count
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

                let names_offset = output.stream_position()?;
                output.seek(SeekFrom::Start(start_address))?;
                output.write_all(&(names_offset as u32).to_le_bytes())?;
                output.seek(SeekFrom::End(0))?;

                #[serde_as]
                #[derive(Serialize)]
                struct NamesSet<'a>(
                    #[serde_as(as = "HashSet<UniCaseAs<_>>")]
                    &'a HashSet<UniCase<Cow<'static, str>>>,
                );
                oxicode::serde::encode_into_std_write(
                    &NamesSet(&data.names),
                    &mut output,
                    standard(),
                )?;
            }
            Ok(())
        },
    )?;

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
            let offset = serialize_using::<_, rancor::Error>(word, &mut serializer)?;
            *interner_trie.exact_match_mut(lemma).unwrap() = offset as u32;
        }
        <Result<(), BakeError>>::Ok(())
    })?;

    output.seek(SeekFrom::Start(trie_start))?;
    let new_trie_size =
        oxicode::serde::encode_into_std_write(&interner_trie, &mut output, legacy())?;
    output.seek(SeekFrom::End(0))?;
    assert_eq!(original_trie_size, new_trie_size);

    Ok(())
}

fn write_with_addresses<W, F>(mut output: W, count: usize, action: F) -> Result<(), BakeError>
where
    W: Write + Seek,
    F: FnOnce(&mut W, &mut Vec<u32>) -> Result<(), BakeError>,
{
    let mut addresses = vec![0; count];
    let addresses_offset = output.stream_position()?;
    output.seek_relative(count as i64 * 4)?;
    action(&mut output, &mut addresses)?;
    output.seek(SeekFrom::Start(addresses_offset))?;
    for address in addresses {
        output.write_all(&address.to_le_bytes())?;
    }
    output.seek(SeekFrom::End(0))?;
    Ok(())
}
