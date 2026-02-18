use crate::usj::{UsjContent, load_footnote_from_usfm};
use charabia::Language;
use itertools::Itertools;
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_cow::CowStr;
use serde_with::{DeserializeAs, SerializeAs};
use std::borrow::Borrow;
use std::fmt::Write;
use std::sync::atomic;
use std::sync::atomic::AtomicBool;
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
        let code = <CowStr>::deserialize(deserializer)?.0;
        Language::from_code(&code).ok_or_else(|| {
            Error::invalid_value(
                Unexpected::Str(&code),
                &"an ISO 639-9 3-letter language code",
            )
        })
    }
}

#[derive(Default)]
pub struct ExclusiveMutex {
    active: AtomicBool,
}

impl ExclusiveMutex {
    pub fn lock(&self) -> Option<PanicBarrierLock<'_>> {
        self.active
            .compare_exchange(
                false,
                true,
                atomic::Ordering::AcqRel,
                atomic::Ordering::Relaxed,
            )
            .ok()?;
        Some(PanicBarrierLock(&self.active))
    }
}

pub struct PanicBarrierLock<'a>(&'a AtomicBool);

impl Drop for PanicBarrierLock<'_> {
    fn drop(&mut self) {
        self.0.store(false, atomic::Ordering::Release);
    }
}

// TODO: Move the SerializeAs and DeserializeAs implementations into their own module
pub struct FootnoteAsUsfm;

impl<'de> DeserializeAs<'de, UsjContent> for FootnoteAsUsfm {
    fn deserialize_as<D>(deserializer: D) -> Result<UsjContent, D::Error>
    where
        D: Deserializer<'de>,
    {
        let footnote = <CowStr>::deserialize(deserializer)?.0;
        let loaded = load_footnote_from_usfm(&footnote).map_err(Error::custom)?;
        if !loaded.diagnostics.is_empty() {
            let mut error = String::from("Invalid footnote:");
            for diag in loaded.diagnostics {
                let _ = write!(error, "\n  - {}", diag.message);
            }
            return Err(Error::custom(error));
        }
        Ok(loaded.usj)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefixTree<K, V> {
    children: Vec<(K, Self)>,
    value: Option<V>,
}

impl<K, V> Default for PrefixTree<K, V> {
    fn default() -> Self {
        Self {
            children: vec![],
            value: None,
        }
    }
}

impl<K, V> PrefixTree<K, V> {
    pub fn value(&self) -> Option<&V> {
        self.value.as_ref()
    }
}

impl<K, V> PrefixTree<K, V>
where
    K: Ord,
{
    pub fn child<KB>(&self, k: KB) -> Option<&Self>
    where
        KB: Borrow<K>,
    {
        self.children
            .binary_search_by_key(&k.borrow(), |(key, _)| key)
            .ok()
            .map(|idx| &self.children[idx].1)
    }

    #[cfg(test)]
    pub fn indirect_child<KB, I>(&self, k: I) -> Option<&Self>
    where
        KB: Borrow<K>,
        I: IntoIterator<Item = KB>,
    {
        let mut tree = self;
        for part in k {
            tree = tree.child(part)?;
        }
        Some(tree)
    }

    #[cfg(test)]
    pub fn get<KB, I>(&self, k: I) -> Option<&V>
    where
        KB: Borrow<K>,
        I: IntoIterator<Item = KB>,
    {
        self.indirect_child(k)?.value.as_ref()
    }
}

impl<K, KI, V> FromIterator<(KI, V)> for PrefixTree<K, V>
where
    K: Ord,
    KI: IntoIterator<Item = K>,
{
    fn from_iter<T: IntoIterator<Item = (KI, V)>>(iter: T) -> Self {
        let iter = iter
            .into_iter()
            .map(|(sub, value)| (sub.into_iter().collect_vec(), value))
            .sorted_unstable_by(|(k1, _), (k2, _)| k1.cmp(k2));
        let mut stack = vec![(None, PrefixTree::default())];

        let finish_one = |stack: &mut Vec<(Option<K>, Self)>| {
            let (key, mut finished) = stack.pop().unwrap();
            finished.children.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
            finished.children.shrink_to_fit();
            stack
                .last_mut()
                .unwrap()
                .1
                .children
                .push((key.unwrap(), finished));
        };

        for (key, value) in iter {
            while stack.len() > key.len() + 1 {
                finish_one(&mut stack);
            }
            while stack.len() > 1
                && stack.last().unwrap().0.as_ref().unwrap() != &key[stack.len() - 2]
            {
                finish_one(&mut stack);
            }
            if stack.len() < key.len() + 1 {
                stack.extend(
                    key.into_iter()
                        .skip(stack.len() - 1)
                        .map(|k| (Some(k), PrefixTree::default())),
                );
            }
            stack.last_mut().unwrap().1.value = Some(value);
        }

        while stack.len() > 1 {
            finish_one(&mut stack);
        }
        let mut result = stack.into_iter().next().unwrap().1;
        result.children.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        result.children.shrink_to_fit();
        result
    }
}

impl<I, K, KI, V> From<I> for PrefixTree<K, V>
where
    I: IntoIterator<Item = (KI, V)>,
    K: Ord,
    KI: IntoIterator<Item = K>,
{
    fn from(value: I) -> Self {
        Self::from_iter(value)
    }
}

#[cfg(test)]
mod test {
    use crate::utils::PrefixTree;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_construct_tree() {
        let tree = PrefixTree::from([
            (vec!["hello", "world"], 1),
            (vec!["hello", "echo", "apple"], 2),
            (vec!["hello", "echo"], 3),
            (vec!["hello"], 4),
            (vec!["hello", "deeper", "key"], 5),
            (vec!["other"], 6),
            (vec!["some", "really", "long"], 7),
            (vec![], 8),
        ]);
        assert_eq!(tree.get(["hello", "world"]), Some(&1));
        assert_eq!(tree.get(["hello", "echo", "apple"]), Some(&2));
        assert_eq!(tree.get(["hello", "echo"]), Some(&3));
        assert_eq!(tree.get(["hello"]), Some(&4));
        assert_eq!(tree.get(["hello", "deeper", "key"]), Some(&5));
        assert_eq!(tree.get(["other"]), Some(&6));
        assert_eq!(tree.get(["some", "really", "long"]), Some(&7));
        assert_eq!(tree.get::<&str, _>([]), Some(&8));
        assert_eq!(tree.get(["none"]), None);
        assert_eq!(tree.get(["hello", "none"]), None);
    }
}
