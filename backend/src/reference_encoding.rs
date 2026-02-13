use crate::book_category::{APOCRYPHA_BOOKS, NEW_TESTAMENT_BOOKS, OLD_TESTAMENT_BOOKS};
use crate::book_data::Book;
use crate::reference::{BibleReference, BookReference};
use crate::verse_range::VerseRange;
use itertools::Itertools;
use lehmer::Lehmer;
use rustrict::{Censor, Type};
use std::num::NonZeroU8;
use strum::VariantArray;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReferenceEncodingError {
    #[error("Invalid base58 character '{0}'")]
    InvalidChar(char),
    #[error("Reference too big to encode/decode")]
    TooBig,
    #[error("Can't encode 0 references")]
    NoReferences,
    #[error("Invalid book to encode {0}")]
    InvalidBook(Book),
    #[error("Invalid chapter to encode {0} {1}")]
    InvalidChapter(Book, NonZeroU8),
    #[error("No verses remaining in chapter {0} {1}")]
    NoVersesRemaining(Book, NonZeroU8),
    #[error("Final reference signal was indicated, but more references remain")]
    NonExhaustedReference,
}

const BASE58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

pub type Carrier = u64;
const MAX_BASE58_LENGTH: usize = Carrier::MAX.ilog(58) as usize + 1;

pub fn base58_encode(value: Carrier) -> String {
    if value == 0 {
        return "1".to_string();
    }
    let mut result = [0u8; MAX_BASE58_LENGTH];
    let result = base58_encode_internal(value, &mut result);
    // SAFETY: Every character above out_index is filled in, or else we'll have panicked by now
    unsafe { str::from_utf8_unchecked(result) }.to_string()
}

pub fn is_base58_swear(value: Carrier) -> bool {
    if value < 58 {
        return false;
    }
    let mut text = [0u8; MAX_BASE58_LENGTH];
    let text = base58_encode_internal(value, &mut text);
    Censor::new(
        text.iter()
            .map(|x| unsafe { char::from_u32_unchecked(*x as u32) }),
    )
    .analyze()
    .is(Type::ANY & !Type::SPAM)
}

fn base58_encode_internal(mut value: Carrier, output: &mut [u8; MAX_BASE58_LENGTH]) -> &[u8] {
    let mut out_index = MAX_BASE58_LENGTH;
    while value > 0 {
        out_index -= 1;
        output[out_index] = BASE58_ALPHABET[(value % 58) as usize];
        value /= 58;
    }
    &output[out_index..]
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

#[derive(Copy, Clone)]
struct BookType {
    len: Carrier,
    into_carrier: fn(Book) -> Carrier,
    from_carrier: fn(Carrier) -> Book,
}

const FIRST_NEW_TESTAMENT: Book = Book::Matthew;
const FIRST_NEW_TESTAMENT_ID: Carrier = FIRST_NEW_TESTAMENT as Carrier;
const FIRST_APOCRYPHA: Book = Book::Tobit;
const FIRST_APOCRYPHA_ID: Carrier = FIRST_APOCRYPHA as Carrier;
const FIRST_SPECIAL: Book = Book::FrontMatter;
const FIRST_SPECIAL_ID: Carrier = FIRST_SPECIAL as Carrier;
const BOOK_TYPES: &[BookType] = &[
    /* 0: OT */
    BookType {
        len: FIRST_NEW_TESTAMENT_ID,
        into_carrier: |b| b as Carrier,
        from_carrier: |c| Book::VARIANTS[c as usize],
    },
    /* 1: NT */
    BookType {
        len: FIRST_APOCRYPHA_ID - FIRST_NEW_TESTAMENT_ID,
        into_carrier: |b| b as Carrier - FIRST_NEW_TESTAMENT_ID,
        from_carrier: |c| Book::VARIANTS[(c + FIRST_NEW_TESTAMENT_ID) as usize],
    },
    /* 2: OT + NT */
    BookType {
        len: FIRST_APOCRYPHA_ID,
        into_carrier: |b| b as Carrier,
        from_carrier: |c| Book::VARIANTS[c as usize],
    },
    /* 3: AP */
    BookType {
        len: FIRST_SPECIAL_ID - FIRST_APOCRYPHA_ID,
        into_carrier: |b| b as Carrier - FIRST_APOCRYPHA_ID,
        from_carrier: |c| Book::VARIANTS[(c + FIRST_APOCRYPHA_ID) as usize],
    },
    /* 4: OT + AP */
    BookType {
        len: FIRST_NEW_TESTAMENT_ID + (FIRST_SPECIAL_ID - FIRST_APOCRYPHA_ID),
        into_carrier: |b| {
            if b >= FIRST_APOCRYPHA {
                b as Carrier - FIRST_APOCRYPHA_ID + FIRST_NEW_TESTAMENT_ID
            } else {
                b as Carrier
            }
        },
        from_carrier: |c| {
            if c >= FIRST_NEW_TESTAMENT_ID {
                Book::VARIANTS[(c - FIRST_NEW_TESTAMENT_ID + FIRST_APOCRYPHA_ID) as usize]
            } else {
                Book::VARIANTS[c as usize]
            }
        },
    },
    /* 5: OT + NT + AP */
    BookType {
        len: FIRST_SPECIAL_ID,
        into_carrier: |b| b as Carrier,
        from_carrier: |c| Book::VARIANTS[c as usize],
    },
];

pub fn encode_references_to_num(
    references: &[BibleReference],
) -> Result<Carrier, ReferenceEncodingError> {
    let has_ot = references
        .iter()
        .any(|x| OLD_TESTAMENT_BOOKS.contains(x.book));
    let has_nt = references
        .iter()
        .any(|x| NEW_TESTAMENT_BOOKS.contains(x.book));
    let has_ap = references.iter().any(|x| APOCRYPHA_BOOKS.contains(x.book));
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

    let book_base = book_type.len + 1;

    let mut references_ordered = references
        .iter()
        .copied()
        .enumerate()
        .map(|(i, r)| (r, i))
        .collect_vec();
    references_ordered.sort_unstable();

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
            book_offset = (book_type.into_carrier)(previous_reference.book);
            if reference.book == previous_reference.book {
                chapter_offset = previous_reference.chapter.get() as Carrier;
                if reference.chapter == previous_reference.chapter {
                    verse_offset = previous_reference.verses.last_u8() as Carrier + 1;
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
            (book_type.into_carrier)(reference.book) + 1,
            //    Have to add 1 here, otherwise books like ^^^
            //             Genesis are 0 and non-decodable
            book_offset,
        )?;
    }

    result = mul_add(result, BOOK_TYPES.len() as Carrier, book_type_id as Carrier)?;

    Ok(result)
}

pub fn decode_references_from_num(
    mut refs: Carrier,
) -> Result<Vec<BibleReference>, ReferenceEncodingError> {
    let mut result: Vec<BibleReference> = vec![];

    let book_type_id;
    (refs, book_type_id) = div_mod(refs, BOOK_TYPES.len() as Carrier);
    let book_type = BOOK_TYPES[book_type_id as usize];

    let book_base = book_type.len + 1;

    let mut lehmer_product = 1;
    let mut book_offset = 0;
    let mut chapter_offset = 1;
    let mut verse_offset = 1;
    while refs >= lehmer_product {
        let book_id;
        (refs, book_id) = div_mod_with_offset(refs, book_base, book_offset);
        if book_id == 0 {
            return Err(ReferenceEncodingError::NonExhaustedReference);
        }
        let book = (book_type.from_carrier)(book_id - 1);

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
        if verse_offset > verse_count.get() as Carrier {
            return Err(ReferenceEncodingError::NoVersesRemaining(book, chapter_num));
        }
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

        verse_offset = last_verse_num.get() as Carrier + 1;
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
    use crate::reference::BibleReference;
    use crate::reference_encoding::{
        MAX_BASE58_LENGTH, ReferenceEncodingError, base58_decode, base58_encode,
        decode_references_from_num, encode_references_to_num,
    };
    use crate::reference_value;
    use itertools::Itertools;
    use unicase::UniCase;

    fn encode_references(references: &[BibleReference]) -> Result<String, ReferenceEncodingError> {
        Ok(base58_encode(encode_references_to_num(references)?))
    }

    fn decode_references(references: &str) -> Result<Vec<BibleReference>, ReferenceEncodingError> {
        decode_references_from_num(base58_decode(references)?)
    }

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
        roundtrip_test!(Genesis 1:1-1);
        roundtrip_test!(Genesis 1:31-31);
        roundtrip_test!(Luke 1:22-48, Matthew 28:18-20, Luke 1:16-21);
        roundtrip_test!(Acts 1:2-4, Acts 1:5-10);
        Ok(())
    }

    #[test]
    #[ignore]
    fn check_swears() -> Result<(), ReferenceEncodingError> {
        let mut count = 0usize;
        let mut failed_count = 0usize;
        let mut error_count = 0usize;
        for swear in (censor::Standard + censor::Zealous + censor::Sex)
            .list()
            .sorted_unstable()
        {
            if swear.len() > MAX_BASE58_LENGTH {
                continue;
            }
            let swear_unicase = UniCase::new(swear);
            for variation in swear
                .bytes()
                .interleave(swear.bytes().map(|x| x.to_ascii_uppercase()))
                .combinations(swear.len())
                .map(|s| String::from_utf8(s).unwrap())
                .filter(|x| UniCase::new(x) == swear_unicase)
                .unique()
            {
                let Ok(decoded_num) = base58_decode(&variation) else {
                    continue;
                };
                count += 1;
                let Ok(decoded) = decode_references_from_num(decoded_num) else {
                    error_count += 1;
                    continue;
                };
                let Ok(re_encoded) = encode_references(&decoded) else {
                    error_count += 1;
                    continue;
                };
                if variation == re_encoded {
                    println!("Swear {variation} failed check: {decoded:?}");
                    failed_count += 1;
                }
            }
        }
        println!(
            "Checked {count} swears. {failed_count} are real. {error_count} caused errors. {} are no issue.",
            count - failed_count - error_count
        );
        Ok(())
    }
}
