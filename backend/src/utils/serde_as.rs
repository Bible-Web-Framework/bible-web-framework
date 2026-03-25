use crate::usj::content::UsjContent;
use crate::usj::loader::load_footnote_from_usfm;
use crate::verse_range::VerseRange;
use charabia::Language;
use miette::{Diagnostic, Severity};
use serde::de::{Error, SeqAccess, Unexpected, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_cow::CowStr;
use serde_with::{DeserializeAs, SerializeAs};
use std::fmt::{Formatter, Write};
use std::marker::PhantomData;
use std::num::NonZeroU8;

pub struct OptionAsVec;

pub struct LanguageAsCode;

pub struct FootnoteUsfmAsUsj;

pub struct VerseRangeAsTuple;

impl<T> SerializeAs<Option<T>> for OptionAsVec
where
    T: Serialize,
{
    fn serialize_as<S>(source: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let iter = source.iter();
        let mut seq = serializer.serialize_seq(Some(iter.len()))?;
        for element in iter {
            seq.serialize_element(element)?;
        }
        seq.end()
    }
}

impl<'de, T> DeserializeAs<'de, Option<T>> for OptionAsVec
where
    T: Deserialize<'de>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VecVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for VecVisitor<T> {
            type Value = Option<T>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence with zero or one values")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let value = seq.next_element()?;
                if seq.next_element::<T>()?.is_some() {
                    return Err(Error::invalid_length(
                        2 + seq.size_hint().unwrap_or_default(),
                        &self,
                    ));
                }
                Ok(value)
            }
        }
        deserializer.deserialize_seq(VecVisitor(PhantomData))
    }
}

impl SerializeAs<Language> for LanguageAsCode {
    fn serialize_as<S>(source: &Language, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        source.code().serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, Language> for LanguageAsCode {
    fn deserialize_as<D>(deserializer: D) -> Result<Language, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = <CowStr>::deserialize(deserializer)?.0;
        Language::from_code(&code).ok_or_else(|| {
            Error::invalid_value(
                Unexpected::Str(&code),
                &"an ISO 639-9 3-letter language code (see docs)",
            )
        })
    }
}

impl<'de> DeserializeAs<'de, UsjContent> for FootnoteUsfmAsUsj {
    fn deserialize_as<D>(deserializer: D) -> Result<UsjContent, D::Error>
    where
        D: Deserializer<'de>,
    {
        let footnote = String::deserialize(deserializer)?;
        let loaded = load_footnote_from_usfm(footnote).map_err(Error::custom)?;
        if loaded
            .diagnostics
            .iter()
            .any(|x| x.severity() == Some(Severity::Error))
        {
            let mut error = String::from("Invalid footnote:");
            for diag in loaded.diagnostics {
                let _ = write!(error, "\n  - {}", diag.message);
            }
            return Err(Error::custom(error));
        }
        Ok(loaded.usj)
    }
}

impl SerializeAs<VerseRange> for VerseRangeAsTuple {
    fn serialize_as<S>(source: &VerseRange, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (source.first(), source.last()).serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, VerseRange> for VerseRangeAsTuple {
    fn deserialize_as<D>(deserializer: D) -> Result<VerseRange, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (first, last) = <(NonZeroU8, NonZeroU8)>::deserialize(deserializer)?;
        VerseRange::new(first, last).map_err(|_| {
            D::Error::invalid_value(
                Unexpected::Other(&format!("({first}, {last})")),
                &"values to be in order",
            )
        })
    }
}
