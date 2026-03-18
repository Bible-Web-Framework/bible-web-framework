use crate::nz_u8;
use crate::utils::nfkd_str;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter};
use std::num::NonZeroU8;
use std::ops::RangeInclusive;
use std::str::FromStr;
use thiserror::Error;

#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct VerseRange {
    first: NonZeroU8,
    last: NonZeroU8,
}

impl VerseRange {
    pub fn new(first: NonZeroU8, last: NonZeroU8) -> Result<Self, (NonZeroU8, NonZeroU8)> {
        if first > last {
            return Err((first, last));
        }
        Ok(Self { first, last })
    }

    pub const fn const_new(first: NonZeroU8, last: NonZeroU8) -> Self {
        assert!(
            first.get() <= last.get(),
            "first > last in VerseRange::const_new",
        );
        Self { first, last }
    }

    pub const fn new_single_verse(verse: NonZeroU8) -> Self {
        Self {
            first: verse,
            last: verse,
        }
    }

    pub fn first(&self) -> NonZeroU8 {
        self.first
    }

    pub fn first_u8(&self) -> u8 {
        self.first.get()
    }

    pub fn last(&self) -> NonZeroU8 {
        self.last
    }

    pub fn last_u8(&self) -> u8 {
        self.last.get()
    }

    pub fn range(&self) -> RangeInclusive<NonZeroU8> {
        self.first()..=self.last()
    }

    pub fn contains(&self, verse: NonZeroU8) -> bool {
        self.range().contains(&verse)
    }

    pub const fn split_to_range(self) -> RangeInclusive<Self> {
        Self::new_single_verse(self.first)..=Self::new_single_verse(self.last)
    }

    pub fn is_single_verse(&self) -> bool {
        self.first == self.last
    }
}

impl Default for VerseRange {
    fn default() -> Self {
        Self::new_single_verse(nz_u8!(1))
    }
}

impl FromStr for VerseRange {
    type Err = VerseRangeParseError;

    /// Simple parsing function for verse range. Does not support unbounded ranges like `-5` or `5-`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_verse(verse: &str) -> Result<NonZeroU8, VerseRangeParseError> {
            nfkd_str(verse.trim_matches(|c: char| !c.is_numeric()), &mut [0; 3])
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| VerseRangeParseError::InvalidVerse {
                    verse: verse.to_string(),
                })
        }
        if let Some((first, last)) = s.split_once('-') {
            let first = parse_verse(first)?;
            let last = parse_verse(last)?;
            if first > last {
                return Err(VerseRangeParseError::OutOfOrderVerses {
                    verse_range: (first, last),
                });
            }
            Ok(VerseRange { first, last })
        } else {
            let verse = parse_verse(s)?;
            Ok(VerseRange {
                first: verse,
                last: verse,
            })
        }
    }
}

impl Debug for VerseRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}-{}", self.first, self.last))
    }
}

impl Display for VerseRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_single_verse() {
            f.write_fmt(format_args!("{}", self.first))
        } else {
            f.write_fmt(format_args!("{}-{}", self.first, self.last))
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VerseRangeParseError {
    #[error("Invalid verse number '{verse}'")]
    InvalidVerse { verse: String },
    #[error("Verse {} is larger than verse {}", verse_range.0, verse_range.1)]
    OutOfOrderVerses { verse_range: (NonZeroU8, NonZeroU8) },
}

impl Serialize for VerseRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for VerseRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{Error, Unexpected, Visitor};
        struct Deserializer;
        impl<'de> Visitor<'de> for Deserializer {
            type Value = VerseRange;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a verse range string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                v.parse().map_err(|e| {
                    Error::custom(format_args!("invalid value: {}: {}", Unexpected::Str(v), e))
                })
            }
        }
        deserializer.deserialize_str(Deserializer)
    }
}
