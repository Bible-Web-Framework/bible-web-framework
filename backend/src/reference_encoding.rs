use crate::book_data::Book;
use crate::reference::BibleReference;
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
type Carrier = u128;

pub fn base58_encode(mut value: Carrier) -> String {
    if value == 0 {
        return "1".to_string();
    }
    let mut result = [0u8; 22];
    let mut out_index = 22;
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
fn mul_add(a: Carrier, b: Carrier, x: Carrier) -> Result<Carrier, ReferenceEncodingError> {
    debug_assert!(x < b, "Value X cannot exceed Base");
    a.checked_mul(b)
        .ok_or(ReferenceEncodingError::TooBig)?
        .checked_add(x)
        .ok_or(ReferenceEncodingError::TooBig)
}

#[inline]
fn mul_add_with_offset(
    a: Carrier,
    b: Carrier,
    x: Carrier,
    o: Carrier,
) -> Result<Carrier, ReferenceEncodingError> {
    mul_add(a, b - o, x - o)
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

    result = mul_add(result, 6, book_type_id as Carrier)?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::reference::parse_references;
    use crate::reference_encoding::{ReferenceEncodingError, encode_references};
    use itertools::Itertools;

    #[test]
    pub fn test_encode() -> Result<(), ReferenceEncodingError> {
        // println!("{}", encode_references(&[reference_value!(Genesis 1:1-1)])?);
        // println!("{}", encode_references(&[reference_value!(John 3:16-16)])?);

        // println!("{}", encode_references(&[reference_value!(John 1:1-14)])?);
        // println!(
        //     "{}",
        //     encode_references(&[reference_value!(Revelation 22:12-12)])?
        // );
        // println!(
        //     "{}",
        //     encode_references(&[
        //         reference_value!(John 1:1-14),
        //         reference_value!(Revelation 22:12-12)
        //     ])?
        // );

        let ref_sets = [
            "Mark4:41;Matthew3:1-2;Matthew3:5-6;Mark1:7-8;Mark1:9-11",
            "Luke3:23;Mark1:14-45;Mark2:1-17;Mark3:1-15",
            "Luke4:16-21;Luke8:1-3;Mark4:1-20;Mark4:30-41;Mark5:21-43",
            "Matthew9:27-34;Mark6:2-11;Mark6:30-56;Luke7:11-17;Matthew16:13-20;Mark9:2-37",
            "Mark10:1;Mark10:13-34;Luke9:57-62;Matthew10:37-38;Luke10:25-42;Luke11:1-4;Luke13:10-17;Luke13:22;Luke13:31;Luke14:25-27",
            "Luke15;Luke18:9-14;Mark10:46-52;Mark11:1-10;Luke19:39-44",
            "Mark11:15-19;Mark11:27-33;Luke20:45-;21:-19;Luke21:37-;22:-20;Luke22:39-48;Luke22:54",
            "Mark14:55-65;Luke22:66-;23:-25;Mark15:16-20;Mark15:22-33;Luke23:39-56",
            "Luke24:1-11;John20:19-20;John20:24-29;Matthew28:16-20;Acts1:8-11;John3:16-17;Romans10:9-10",
        ];
        for refs in ref_sets {
            println!("{refs}");
            let parsed = parse_references(refs, None)
                .into_iter()
                .map(Result::unwrap)
                .collect_vec();
            println!("    {:?}", encode_references(&parsed));
        }

        Ok(())
    }
}
