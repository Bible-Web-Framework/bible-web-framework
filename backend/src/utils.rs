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
