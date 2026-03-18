pub mod normalize;
pub mod ordered_enum;
pub mod parsed_string_value;
pub mod prefix_tree;
pub mod serde_as;

use std::borrow::Cow;
use std::sync::atomic;
use std::sync::atomic::AtomicBool;
use unicase::UniCase;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkd_quick};

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

pub trait CloneToOwned {
    type Output;

    fn clone_to_owned(&self) -> Self::Output;
}

impl<'a, T> CloneToOwned for Cow<'a, T>
where
    T: ToOwned + ?Sized + 'static,
{
    type Output = Cow<'static, T>;

    fn clone_to_owned(&self) -> Self::Output {
        Cow::Owned(self.clone().into_owned())
    }
}

impl<'a, T> CloneToOwned for Option<Cow<'a, T>>
where
    T: ToOwned + ?Sized + 'static,
{
    type Output = Option<Cow<'static, T>>;

    fn clone_to_owned(&self) -> Self::Output {
        self.as_ref().map(Cow::clone_to_owned)
    }
}

pub fn nfkd_str<'a, const N: usize>(s: &str, arr: &'a mut [u8; N]) -> Option<&'a mut str> {
    let index = if is_nfkd_quick(s.chars()) == IsNormalized::Yes {
        if s.len() > N {
            return None;
        }
        arr[..s.len()].copy_from_slice(s.as_bytes());
        s.len()
    } else {
        let mut remaining = arr as &mut [u8];
        for normalized in s.nfkd() {
            let len = normalized.len_utf8();
            if len > remaining.len() {
                return None;
            }
            normalized.encode_utf8(remaining);
            remaining = &mut remaining[len..];
        }
        N - remaining.len()
    };
    Some(unsafe { str::from_utf8_unchecked_mut(&mut arr[..index]) })
}
