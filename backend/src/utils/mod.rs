pub mod ordered_enum;
pub mod prefix_tree;
pub mod serde_as;

use std::borrow::Cow;
use std::sync::atomic;
use std::sync::atomic::AtomicBool;
use unicase::UniCase;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

/// Returns a normalized version of `s`, or `None` if normalization was not needed. Normalized
/// means no whitespace and NFKC.
pub fn normalize_str(s: &str) -> Cow<'_, str> {
    if s.chars().any(char::is_whitespace) || is_nfkc_quick(s.chars()) != IsNormalized::Yes {
        Cow::Owned(s.nfkc().filter(|x| !x.is_whitespace()).collect::<String>())
    } else {
        Cow::Borrowed(s)
    }
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

pub trait ToUnicaseCow<'a> {
    fn to_cow(self) -> UniCase<Cow<'a, str>>;
}

impl<'a> ToUnicaseCow<'a> for UniCase<&'a str> {
    fn to_cow(self) -> UniCase<Cow<'a, str>> {
        if self.is_ascii() {
            UniCase::ascii(Cow::Borrowed(self.into_inner()))
        } else {
            UniCase::unicode(Cow::Borrowed(self.into_inner()))
        }
    }
}

pub trait CloneOptionCow<T: ToOwned + ?Sized> {
    fn clone_to_owned(&self) -> Option<Cow<'static, T>>;
}

impl<'a, T> CloneOptionCow<T> for Option<Cow<'a, T>>
where
    T: ToOwned + ?Sized,
{
    fn clone_to_owned(&self) -> Option<Cow<'static, T>> {
        self.as_ref().cloned().map(Cow::into_owned).map(Cow::Owned)
    }
}
