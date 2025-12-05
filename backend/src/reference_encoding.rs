use crate::book_data::Book;
use crate::reference::{BibleReference, BookReference};
use crate::verse_range::VerseRange;
use itertools::Itertools;
use lehmer::Lehmer;
use std::cmp::{max, min};
use std::num::NonZeroU8;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReferenceEncodingError {
    #[error("Invalid base58 character '{0}'")]
    InvalidChar(char),
    #[error("Reference too big to encode/decode")]
    TooBig,
    #[error("Can't encode no references")]
    NoReferences,
    #[error("Invalid book to encode {0}")]
    InvalidBook(Book),
    #[error("Invalid chapter to encode {0} {1}")]
    InvalidChapter(Book, NonZeroU8),
}

const BASE58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
type Carrier = u64;

pub fn base58_encode(mut value: Carrier) -> String {
    if value == 0 {
        return "1".to_string();
    }
    const MAX_LENGTH: usize = Carrier::MAX.ilog(58) as usize + 1;
    let mut result = [0u8; MAX_LENGTH];
    let mut out_index = MAX_LENGTH;
    while value > 0 {
        out_index -= 1;
        result[out_index] = BASE58_ALPHABET[(value % 58) as usize];
        value /= 58;
    }
    // SAFETY: Every character above out_index is filled in, or else we'll have panicked by now
    unsafe { str::from_utf8_unchecked(&result[out_index..]) }.to_string()
}

pub fn base58_decode(x: &str) -> Result<Carrier, ReferenceEncodingError> {
    let mut result = 0;
    for c in x.as_bytes() {
        result = mul_add(
            result,
            58,
            BASE58_ALPHABET
                .iter()
                .position(|x| x == c)
                .ok_or(ReferenceEncodingError::InvalidChar(*c as char))? as Carrier,
        )?;
    }
    Ok(result)
}

#[inline]
fn mul_add(
    accum: Carrier,
    base: Carrier,
    value: Carrier,
) -> Result<Carrier, ReferenceEncodingError> {
    debug_assert!(value < base, "Value {value} is invalid for base {base}");
    accum
        .checked_mul(base)
        .ok_or(ReferenceEncodingError::TooBig)?
        .checked_add(value)
        .ok_or(ReferenceEncodingError::TooBig)
}

#[inline]
fn mul_add_with_offset(
    accum: Carrier,
    base: Carrier,
    value: Carrier,
    offset: Carrier,
) -> Result<Carrier, ReferenceEncodingError> {
    mul_add(accum, base - offset, value - offset)
}

macro_rules! make_book_types {
    (
        OLD_TESTAMENT = [$($old_testament:expr,)+],
        NEW_TESTAMENT = [$($new_testament:expr,)+],
        APOCRYPHA = [$($apocrypha:expr,)+],
    ) => {
        const OLD_TESTAMENT: &[Book] = &[$($old_testament,)+];
        const NEW_TESTAMENT: &[Book] = &[$($new_testament,)+];
        const APOCRYPHA: &[Book] = &[$($apocrypha,)+];

        const BOOK_TYPES: &[&[Book]] = &[
            &[$($old_testament,)+], // 0
            &[$($new_testament,)+], // 1
            &[$($old_testament,)+ $($new_testament,)+], // 2
            &[$($apocrypha,)+], // 3
            &[$($old_testament,)+ $($apocrypha,)+], // 4
            &[$($old_testament,)+ $($new_testament,)+ $($apocrypha,)+], // 5
        ];
    };
}

make_book_types!(
    OLD_TESTAMENT = [
        Book::Genesis,
        Book::Exodus,
        Book::Leviticus,
        Book::Numbers,
        Book::Deuteronomy,
        Book::Joshua,
        Book::Judges,
        Book::Ruth,
        Book::FirstSamuel,
        Book::SecondSamuel,
        Book::FirstKings,
        Book::SecondKings,
        Book::FirstChronicles,
        Book::SecondChronicles,
        Book::Ezra,
        Book::Nehemiah,
        Book::Esther,
        Book::Job,
        Book::Psalms,
        Book::Proverbs,
        Book::Ecclesiastes,
        Book::SongOfSolomon,
        Book::Isaiah,
        Book::Jeremiah,
        Book::Lamentations,
        Book::Ezekiel,
        Book::Daniel,
        Book::Hosea,
        Book::Joel,
        Book::Amos,
        Book::Obadiah,
        Book::Jonah,
        Book::Micah,
        Book::Nahum,
        Book::Habakkuk,
        Book::Zephaniah,
        Book::Haggai,
        Book::Zechariah,
        Book::Malachi,
    ],
    NEW_TESTAMENT = [
        Book::Matthew,
        Book::Mark,
        Book::Luke,
        Book::John,
        Book::Acts,
        Book::Romans,
        Book::FirstCorinthians,
        Book::SecondCorinthians,
        Book::Galatians,
        Book::Ephesians,
        Book::Philippians,
        Book::Colossians,
        Book::FirstThessalonians,
        Book::SecondThessalonians,
        Book::FirstTimothy,
        Book::SecondTimothy,
        Book::Titus,
        Book::Philemon,
        Book::Hebrews,
        Book::James,
        Book::FirstPeter,
        Book::SecondPeter,
        Book::FirstJohn,
        Book::SecondJohn,
        Book::ThirdJohn,
        Book::Jude,
        Book::Revelation,
    ],
    APOCRYPHA = [
        Book::Tobit,
        Book::Judith,
        Book::EstherGreek,
        Book::WisdomOfSolomon,
        Book::Sirach,
        Book::Baruch,
        Book::LetterOfJeremiah,
        Book::SongOfTheThreeYoungMen,
        Book::Susanna,
        Book::BelAndTheDragon,
        Book::FirstMaccabees,
        Book::SecondMaccabees,
        Book::ThirdMaccabees,
        Book::FourthMaccabees,
        Book::FirstEsdras,
        Book::SecondEsdras,
        Book::PrayerOfManasseh,
        Book::PsalmOneFiftyOne,
        Book::Odes,
        Book::PsalmsOfSolomon,
        Book::EzraApocalypse,
        Book::FifthEzra,
        Book::SixthEzra,
        Book::DanielGreek,
        Book::PsalmOneFiftyTwoThroughOneFiftyFive,
        Book::SecondBaruch,
        Book::LetterOfBaruch,
        Book::Jubilees,
        Book::Enoch,
        Book::Reproof,
        Book::FourthBaruch,
        Book::LetterToTheLaodiceans,
    ],
);

pub fn encode_references(references: &[BibleReference]) -> Result<String, ReferenceEncodingError> {
    Ok(base58_encode(encode_references_to_num(references)?))
}

fn encode_references_to_num(
    references: &[BibleReference],
) -> Result<Carrier, ReferenceEncodingError> {
    let has_ot = references.iter().any(|x| OLD_TESTAMENT.contains(&x.book));
    let has_nt = references.iter().any(|x| NEW_TESTAMENT.contains(&x.book));
    let has_ap = references.iter().any(|x| APOCRYPHA.contains(&x.book));
    let book_type_id = if has_ap {
        if has_nt {
            5
        } else if has_ot {
            4
        } else {
            3
        }
    } else if has_nt {
        if has_ot { 2 } else { 1 }
    } else if has_ot {
        0
    } else {
        return Err(ReferenceEncodingError::NoReferences);
    };
    let book_type = BOOK_TYPES[book_type_id];

    let book_base = book_type.len() as Carrier + 1;

    let mut references_ordered = references
        .iter()
        .copied()
        .enumerate()
        .map(|(i, r)| (r, i))
        .collect_vec();
    references_ordered.sort_unstable();

    {
        let mut did_simplify = false;
        let mut index = 0;
        while index < references_ordered.len() - 1 {
            let (next_reference, _) = references_ordered[index + 1];
            let (reference, _) = &mut references_ordered[index];
            if next_reference.book == reference.book
                && next_reference.chapter == reference.chapter
                && next_reference.verses.first_u8() - 1 <= reference.verses.last_u8()
            {
                reference.reference.verses = VerseRange::new(
                    min(reference.verses.first(), next_reference.verses.first()),
                    max(reference.verses.last(), next_reference.verses.last()),
                )
                .unwrap();
                references_ordered.remove(index + 1);
                did_simplify = true;
            } else {
                index += 1;
            }
        }

        if did_simplify {
            // Recompute the indices after simplification
            references_ordered.sort_unstable_by_key(|&(_, idx)| idx);
            references_ordered
                .iter_mut()
                .enumerate()
                .for_each(|(i, (_, index))| *index = i);
            references_ordered.sort_unstable();
        }
    }

    let mut result = 0;

    // Stuff down below is read back in reverse order

    let ordering_base = Lehmer::max_value(references_ordered.len()) as Carrier + 1;
    let lehmer_code = Lehmer::from_permutation(
        &references_ordered
            .iter()
            .map(|&(_, i)| i as u8)
            .collect_vec(),
    );
    result = mul_add(result, ordering_base, lehmer_code.to_decimal() as Carrier)?;

    for (ordered_index, (reference, _)) in references_ordered.iter().enumerate().rev() {
        let mut book_offset = 0;
        let mut chapter_offset = 1;
        let mut verse_offset = 1;

        if ordered_index > 0 {
            let (previous_reference, _) = references_ordered[ordered_index - 1];
            book_offset = book_type
                .iter()
                .position(|&x| x == previous_reference.book)
                .unwrap() as Carrier;
            if reference.book == previous_reference.book {
                chapter_offset = previous_reference.chapter.get() as Carrier;
                if reference.chapter == previous_reference.chapter {
                    verse_offset = previous_reference.verses.last_u8() as Carrier + 2;
                }
            }
        }

        let chapter_count = reference
            .book
            .chapter_count()
            .ok_or(ReferenceEncodingError::InvalidBook(reference.book))?;
        let verse_count = reference.book.verse_count(reference.chapter).ok_or(
            ReferenceEncodingError::InvalidChapter(reference.book, reference.chapter),
        )?;

        result = mul_add_with_offset(
            result,
            verse_count.get() as Carrier + 1,
            reference.verses.last_u8() as Carrier,
            reference.verses.first_u8() as Carrier,
        )?;
        result = mul_add_with_offset(
            result,
            verse_count.get() as Carrier + 1,
            reference.verses.first_u8() as Carrier,
            verse_offset,
        )?;
        result = mul_add_with_offset(
            result,
            chapter_count.get() as Carrier + 1,
            reference.chapter.get() as Carrier,
            chapter_offset,
        )?;
        result = mul_add_with_offset(
            result,
            book_base,
            book_type
                .iter()
                .position(|&x| x == reference.book)
                .expect("chapter_count lookup should've already bailed") as Carrier
                + 1,
            //  ^^^ Have to add 1 here, otherwise books like Genesis are 0 and non-decodable
            book_offset,
        )?;
    }

    result = mul_add(result, BOOK_TYPES.len() as Carrier, book_type_id as Carrier)?;

    Ok(result)
}

pub fn decode_references(references: &str) -> Result<Vec<BibleReference>, ReferenceEncodingError> {
    decode_references_from_num(base58_decode(references)?)
}

fn decode_references_from_num(
    mut refs: Carrier,
) -> Result<Vec<BibleReference>, ReferenceEncodingError> {
    let mut result: Vec<BibleReference> = vec![];

    let book_type_id;
    (refs, book_type_id) = div_mod(refs, BOOK_TYPES.len() as Carrier);
    let book_type = BOOK_TYPES[book_type_id as usize];

    let book_base = book_type.len() as Carrier + 1;

    let mut lehmer_product = 1;
    let mut book_offset = 0;
    let mut chapter_offset = 1;
    let mut verse_offset = 1;
    while refs >= lehmer_product {
        let book_id;
        (refs, book_id) = div_mod_with_offset(refs, book_base, book_offset);
        let book = book_type[(book_id - 1) as usize];

        if book_id - 1 != book_offset {
            book_offset = book_id - 1;
            chapter_offset = 1;
            verse_offset = 1;
        }

        let chapter_count = book
            .chapter_count()
            .ok_or(ReferenceEncodingError::InvalidBook(book))?;
        let chapter_num;
        (refs, chapter_num) =
            div_mod_with_offset(refs, chapter_count.get() as Carrier + 1, chapter_offset);
        let chapter_num = NonZeroU8::new(chapter_num as u8).unwrap();

        if chapter_num.get() as Carrier != chapter_offset {
            chapter_offset = chapter_num.get() as Carrier;
            verse_offset = 1;
        }

        let verse_count = book
            .verse_count(chapter_num)
            .ok_or(ReferenceEncodingError::InvalidChapter(book, chapter_num))?;
        let verse_num;
        (refs, verse_num) =
            div_mod_with_offset(refs, verse_count.get() as Carrier + 1, verse_offset);
        let first_verse_num = NonZeroU8::new(verse_num as u8).unwrap();

        let last_verse_num;
        (refs, last_verse_num) = div_mod_with_offset(
            refs,
            verse_count.get() as Carrier + 1,
            first_verse_num.get() as Carrier,
        );
        let last_verse_num = NonZeroU8::new(last_verse_num as u8).unwrap();

        result.push(BibleReference {
            book,
            reference: BookReference {
                chapter: chapter_num,
                verses: VerseRange::new(first_verse_num, last_verse_num).unwrap(),
            },
        });

        verse_offset = last_verse_num.get() as Carrier + 2;
        lehmer_product *= result.len() as Carrier;
    }

    let mut permutation = Lehmer::from_decimal(refs as usize, result.len()).to_permutation();
    // https://github.com/tiby312/reorder
    for i in 0..result.len() {
        let mut target = permutation[i] as usize;
        while i != target {
            permutation.swap(i, target);
            result.swap(i, target);
            target = permutation[i] as usize;
        }
    }

    Ok(result)
}

/// Returns `(accum, value)`
#[inline]
fn div_mod(accum: Carrier, base: Carrier) -> (Carrier, Carrier) {
    (accum / base, accum % base)
}

/// Returns `(accum, value)`
#[inline]
fn div_mod_with_offset(accum: Carrier, base: Carrier, offset: Carrier) -> (Carrier, Carrier) {
    (accum / (base - offset), accum % (base - offset) + offset)
}

#[cfg(test)]
mod tests {

    use crate::reference_encoding::{ReferenceEncodingError, decode_references, encode_references};
    use crate::reference_value;

    macro_rules! roundtrip_test {
        ($($book:ident $chapter:literal:$verse_start:literal-$verse_end:literal),+ $(,)?) => {{
            let references = &[$(reference_value!($book $chapter:$verse_start-$verse_end)),+];
            let encoded = encode_references(references)?;
            println!("{references:?} is encoded as {encoded}");
            let decoded = decode_references(&encoded)?;
            assert_eq!(references, &decoded[..], "Encoding was {encoded}");
        }};
    }

    #[test]
    fn test_roundtrip() -> Result<(), ReferenceEncodingError> {
        roundtrip_test!(Acts 1:2-4, Acts 1:6-6);
        roundtrip_test!(Genesis 1:1-1, Genesis 1:3-4);
        roundtrip_test!(Matthew 1:1-1, Matthew 1:3-4);
        roundtrip_test!(Luke 1:22-48, Matthew 28:18-20);
        roundtrip_test!(Psalms 119:1-100);
        Ok(())
    }
}
