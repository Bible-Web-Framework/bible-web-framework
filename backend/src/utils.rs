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
            ::core::assert!($e != 0);
            unsafe { ::std::num::NonZeroU8::new_unchecked($e) }
        }
    };
}
