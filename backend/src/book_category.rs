use crate::book_data::Book;
use enumset::{EnumSet, enum_set};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookCategory {
    #[serde(rename = "OT")]
    OldTestament,
    #[serde(rename = "NT")]
    NewTestament,
    #[serde(rename = "AP")]
    Apocrypha,
}

impl BookCategory {
    pub const fn books(self) -> EnumSet<Book> {
        match self {
            Self::OldTestament => OLD_TESTAMENT_BOOKS,
            Self::NewTestament => NEW_TESTAMENT_BOOKS,
            Self::Apocrypha => APOCRYPHA_BOOKS,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BooksOrBookCategory {
    Category(BookCategory),
    Books(EnumSet<Book>),
}

impl From<BooksOrBookCategory> for EnumSet<Book> {
    fn from(val: BooksOrBookCategory) -> Self {
        match val {
            BooksOrBookCategory::Category(category) => category.books(),
            BooksOrBookCategory::Books(books) => books,
        }
    }
}

pub const OLD_TESTAMENT_BOOKS: EnumSet<Book> = enum_set!(
    Book::Genesis |
    Book::Exodus |
    Book::Leviticus |
    Book::Numbers |
    Book::Deuteronomy |
    Book::Joshua |
    Book::Judges |
    Book::Ruth |
    Book::FirstSamuel |
    Book::SecondSamuel |
    Book::FirstKings |
    Book::SecondKings |
    Book::FirstChronicles |
    Book::SecondChronicles |
    Book::Ezra |
    Book::Nehemiah |
    Book::Esther |
    Book::Job |
    Book::Psalms |
    Book::Proverbs |
    Book::Ecclesiastes |
    Book::SongOfSolomon |
    Book::Isaiah |
    Book::Jeremiah |
    Book::Lamentations |
    Book::Ezekiel |
    Book::Daniel |
    Book::Hosea |
    Book::Joel |
    Book::Amos |
    Book::Obadiah |
    Book::Jonah |
    Book::Micah |
    Book::Nahum |
    Book::Habakkuk |
    Book::Zephaniah |
    Book::Haggai |
    Book::Zechariah |
    Book::Malachi |
);

pub const NEW_TESTAMENT_BOOKS: EnumSet<Book> = enum_set!(
    Book::Matthew |
    Book::Mark |
    Book::Luke |
    Book::John |
    Book::Acts |
    Book::Romans |
    Book::FirstCorinthians |
    Book::SecondCorinthians |
    Book::Galatians |
    Book::Ephesians |
    Book::Philippians |
    Book::Colossians |
    Book::FirstThessalonians |
    Book::SecondThessalonians |
    Book::FirstTimothy |
    Book::SecondTimothy |
    Book::Titus |
    Book::Philemon |
    Book::Hebrews |
    Book::James |
    Book::FirstPeter |
    Book::SecondPeter |
    Book::FirstJohn |
    Book::SecondJohn |
    Book::ThirdJohn |
    Book::Jude |
    Book::Revelation |
);

pub const APOCRYPHA_BOOKS: EnumSet<Book> = enum_set!(
    Book::Tobit |
    Book::Judith |
    Book::EstherGreek |
    Book::WisdomOfSolomon |
    Book::Sirach |
    Book::Baruch |
    Book::LetterOfJeremiah |
    Book::SongOfTheThreeYoungMen |
    Book::Susanna |
    Book::BelAndTheDragon |
    Book::FirstMaccabees |
    Book::SecondMaccabees |
    Book::ThirdMaccabees |
    Book::FourthMaccabees |
    Book::FirstEsdras |
    Book::SecondEsdras |
    Book::PrayerOfManasseh |
    Book::PsalmOneFiftyOne |
    Book::Odes |
    Book::PsalmsOfSolomon |
    Book::EzraApocalypse |
    Book::FifthEzra |
    Book::SixthEzra |
    Book::DanielGreek |
    Book::PsalmOneFiftyTwoThroughOneFiftyFive |
    Book::SecondBaruch |
    Book::LetterOfBaruch |
    Book::Jubilees |
    Book::Enoch |
    Book::Reproof |
    Book::FourthBaruch |
    Book::LetterToTheLaodiceans |
);
