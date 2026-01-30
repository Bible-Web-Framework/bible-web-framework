use charabia::Language;
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{DeserializeAs, SerializeAs};
use std::borrow::Cow;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

/// Returns a normalized version of `s`, or `None` if normalization was not needed. Normalized
/// means no whitespace and NFKC.
pub fn normalize_str(s: &str) -> Option<String> {
    if s.chars().any(char::is_whitespace) || is_nfkc_quick(s.chars()) != IsNormalized::Yes {
        Some(s.nfkc().filter(|x| !x.is_whitespace()).collect::<String>())
    } else {
        None
    }
}

/// Executes `operation` with a normalized version of `s`. Normalized means no whitespace and NFKC.
pub fn with_normalized_str<T>(s: &str, operation: impl FnOnce(&str) -> T) -> T {
    let normalized = normalize_str(s);
    operation(normalized.as_deref().unwrap_or(s))
}

#[macro_export]
macro_rules! nz_u8 {
    ($e:expr) => {
        const {
            ::std::assert!($e != 0);
            unsafe { ::std::num::NonZeroU8::new_unchecked($e) }
        }
    };
}

// TODO: Migrate to serde_with
pub mod option_as_vec {
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::fmt::Formatter;
    use std::marker::PhantomData;

    pub fn serialize<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        let iter = value.iter();
        let mut seq = serializer.serialize_seq(Some(iter.len()))?;
        for element in iter {
            seq.serialize_element(element)?;
        }
        seq.end()
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        T: Deserialize<'de>,
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

#[macro_export]
macro_rules! serde_display_and_parse {
    ($ty:ty) => {
        impl ::std::fmt::Display for $ty {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::serde::Serialize::serialize(self, f)
            }
        }

        impl ::std::str::FromStr for $ty {
            type Err = ::serde::de::value::Error;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                ::serde::Deserialize::deserialize(::serde::de::IntoDeserializer::into_deserializer(
                    s,
                ))
            }
        }
    };
}

pub struct LanguageAsCode;

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
        let code = <Cow<'de, str>>::deserialize(deserializer)?;
        Language::from_code(&code).ok_or_else(|| {
            Error::invalid_value(
                Unexpected::Str(&code),
                &"an ISO 639-9 3-letter language code",
            )
        })
    }
}
