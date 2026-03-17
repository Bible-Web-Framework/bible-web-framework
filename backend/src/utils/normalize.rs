use charabia::normalizer::{
    AeOeNormalizer, ArabicNormalizer, CharNormalizer, CharOrStr, ChineseNormalizer,
    ClassifierOption, CompatibilityDecompositionNormalizer, ControlCharNormalizer,
    JapaneseNormalizer, LOSSY_NORMALIZERS, LowercaseNormalizer, NORMALIZERS as DEFAULT_NORMALIZERS,
    Normalizer, NormalizerOption, PersianNormalizer, TurkishNormalizer, VietnameseNormalizer,
};
use charabia::{Language, StrDetection, Token};
use std::borrow::Cow;
use std::sync::LazyLock;

/// Returns a normalized version of `s`, or `None` if normalization was not needed. Normalized
/// means no whitespace and NFKC.
pub fn normalize_str<'a>(
    s: Cow<'a, str>,
    languages: Option<&[Language]>,
) -> (Cow<'a, str>, Option<Vec<usize>>) {
    let mut detector = StrDetection::new(&s, languages);
    let mut token = Token {
        char_end: s.chars().count(),
        byte_end: s.len(),
        script: detector.script(),
        language: detector.language(),
        lemma: s,
        ..Default::default()
    };
    for normalizer in *NORMALIZERS {
        if normalizer.should_normalize(&token) {
            token = normalizer.normalize(
                token,
                &NormalizerOption {
                    create_char_map: true,
                    lossy: true,
                    classifier: ClassifierOption {
                        stop_words: None,
                        separators: None,
                    },
                },
            );
        }
    }
    let resulting_char_map = token.char_map.map(|map| {
        let mut result = Vec::with_capacity(token.lemma.len() + 1);
        let mut source_idx = 0;
        let mut normal_idx = 0;
        for (source_len, normal_len) in map {
            normal_idx += normal_len as usize;
            if normal_len > 0 {
                result.push(source_idx);
                while result.len() < normal_idx {
                    result.push(usize::MAX); // Invalid char boundary
                }
            }
            source_idx += source_len as usize;
        }
        result.push(source_idx);
        result
    });
    (token.lemma, resulting_char_map)
}

// https://github.com/meilisearch/charabia/issues/370
static NORMALIZERS: LazyLock<[&dyn Normalizer; 14]> = LazyLock::new(|| {
    [
        &CompatibilityDecompositionNormalizer,
        &*DEFAULT_NORMALIZERS[1], // &SwedishRecompositionNormalizer,
        &ControlCharNormalizer,
        &PersianNormalizer,
        &LowercaseNormalizer,
        &*LOSSY_NORMALIZERS[1], // &QuoteNormalizer,
        &AeOeNormalizer,
        &ChineseNormalizer,
        &JapaneseNormalizer,
        // &GreekNormalizer, // Only the last character is checked
        &ArabicNormalizer,
        &*LOSSY_NORMALIZERS[7], // &NonspacingMarkNormalizer,
        &VietnameseNormalizer,
        &TurkishNormalizer,
        &SpacesNormalizer,
    ]
});

struct SpacesNormalizer;

impl CharNormalizer for SpacesNormalizer {
    fn normalize_char(&self, c: char) -> Option<CharOrStr> {
        if !c.is_whitespace() {
            Some(c.into())
        } else {
            None
        }
    }

    fn should_normalize(&self, token: &Token) -> bool {
        token.lemma.chars().any(char::is_whitespace)
    }
}

#[cfg(test)]
mod test {
    use crate::utils::normalize::normalize_str;
    use pretty_assertions::assert_eq;
    use std::borrow::Cow;

    #[test]
    fn test_char_map() {
        const M: usize = usize::MAX;
        assert_eq!(
            normalize_str(Cow::Borrowed("hello world"), None),
            (
                Cow::Borrowed("helloworld"),
                Some(vec![0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11])
            ),
        );
        assert_eq!(
            normalize_str(Cow::Borrowed("ヨハネ！"), None),
            (
                Cow::Borrowed("よはね!"),
                Some(vec![0, M, M, 3, M, M, 6, M, M, 9, 12])
            ),
        );
    }
}
