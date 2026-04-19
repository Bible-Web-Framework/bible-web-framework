pub mod normalize;
pub mod ordered_enum;
pub mod parsed_string_value;
pub mod prefix_tree;
pub mod serde_as;

use memory_stats::memory_stats;
use std::borrow::Cow;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, atomic};
use unicase::UniCase;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkd_quick};

#[macro_export]
macro_rules! nz_u8 {
    ($e:expr) => {
        const {
            ::std::assert!($e != 0);
            // SAFETY: Just asserted $e is non-zero
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

pub fn print_memory_stats() {
    if let Some(memory) = memory_stats() {
        const MIB: usize = 1024 * 1024;
        tracing::info!(
            "Process memory usage: physical: {} MiB | virtual: {} MiB",
            memory.physical_mem / MIB,
            memory.virtual_mem / MIB,
        );
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

pub trait AsBorrowed<'a> {
    type Output;

    fn as_borrowed(&'a self) -> Self::Output;
}

impl<'a, 'b: 'a, T> AsBorrowed<'a> for Cow<'b, T>
where
    T: ToOwned + ?Sized + 'static,
{
    type Output = Cow<'a, T>;

    fn as_borrowed(&'a self) -> Self::Output {
        Cow::Borrowed(self)
    }
}

impl<'a, 'b: 'a, T> AsBorrowed<'a> for Option<Cow<'b, T>>
where
    T: ToOwned + ?Sized + 'static,
{
    type Output = Option<Cow<'a, T>>;

    fn as_borrowed(&'a self) -> Self::Output {
        self.as_ref().map(Cow::as_borrowed)
    }
}

pub trait ToOwnedStatic {
    type Output;

    fn to_owned_static(self) -> Self::Output;
}

impl<'a, T> ToOwnedStatic for Cow<'a, T>
where
    T: ToOwned + ?Sized + 'static,
{
    type Output = Cow<'static, T>;

    fn to_owned_static(self) -> Self::Output {
        Cow::Owned(self.into_owned())
    }
}

impl<'a, T> ToOwnedStatic for Option<Cow<'a, T>>
where
    T: ToOwned + ?Sized + 'static,
{
    type Output = Option<Cow<'static, T>>;

    fn to_owned_static(self) -> Self::Output {
        self.map(Cow::to_owned_static)
    }
}

pub enum ArcOrRef<'a, T: ?Sized> {
    Arc(Arc<T>),
    Ref(&'a T),
}

impl<T: ?Sized> Deref for ArcOrRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Arc(value) => value,
            Self::Ref(value) => value,
        }
    }
}

impl<T: ?Sized> Clone for ArcOrRef<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Arc(arc) => Self::Arc(arc.clone()),
            Self::Ref(value) => Self::Ref(value),
        }
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
        let mut remaining = &mut arr[..];
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
    // SAFETY: index bytes have been filled based on the above code
    Some(unsafe { str::from_utf8_unchecked_mut(&mut arr[..index]) })
}
