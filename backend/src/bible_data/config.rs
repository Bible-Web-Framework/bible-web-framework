use crate::book_data::Book;
use crate::reference::BibleReference;
use crate::usj::content::UsjContent;
use crate::utils::ordered_enum::EnumOrderMap;
use crate::utils::prefix_tree::PrefixTree;
use crate::utils::serde_as::{FstSetAs, UniCaseAs};
use charabia::{Language, Tokenizer, TokenizerBuilder};
use rangemap::RangeInclusiveMap;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::borrow::Cow;
use std::collections::HashMap;
use unicase::UniCase;

#[serde_as]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BibleConfig {
    pub display_name: Option<String>,
    pub text_direction: TextDirection,
    pub book_order: EnumOrderMap<Book>,
    #[serde_as(as = "HashMap<UniCaseAs<_>, _>")]
    pub book_aliases: HashMap<UniCase<Cow<'static, str>>, Book>,
    pub search: SearchConfig,
    pub footnotes: FootnotesTree,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
pub enum TextDirection {
    #[serde(rename = "auto")]
    #[default]
    Auto,
    #[serde(rename = "ltr")]
    LeftToRight,
    #[serde(rename = "rtl")]
    RightToLeft,
}

#[serde_as]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SearchConfig {
    pub index: bool,
    pub languages: Option<Box<[Language]>>,
    #[serde_as(as = "Option<FstSetAs<_>>")]
    pub ignored_words: Option<fst::Set<Box<[u8]>>>,
}

pub type FootnotesTree = PrefixTree<String, RangeInclusiveMap<BibleReference, FootnotesConfig>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct FootnotesConfig {
    pub footnote: UsjContent,
}

impl SearchConfig {
    pub fn create_tokenizer(&self) -> Tokenizer<'_> {
        let mut builder = TokenizerBuilder::new();
        if let Some(languages) = &self.languages {
            builder.allow_list(languages);
        }
        if let Some(words) = &self.ignored_words {
            builder.stop_words(words);
        }
        builder.into_tokenizer()
    }
}
