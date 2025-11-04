use std::collections::HashMap;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

pub struct UsfmQuery(pub Query);

#[macro_export]
macro_rules! usfm_queries {
    { $($vis:vis static $name:ident = $query:literal;)+ } => {
        $(
            $vis static $name: ::std::sync::LazyLock<$crate::usfm_converter::query::UsfmQuery> =
                ::std::sync::LazyLock::new(||
                    $crate::usfm_converter::query::UsfmQuery(
                        ::tree_sitter::Query::new(
                            &$crate::usfm_converter::usfm_parser::LANGUAGE,
                            $query,
                        )
                        .unwrap()
                    )
                );
        )+
    };
}

impl UsfmQuery {
    pub fn captures<'query, 'tree, 'source>(
        &'query self,
        node: Node<'tree>,
        source: &'source str,
    ) -> HashMap<&'query str, Vec<Node<'tree>>> {
        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(&self.0, node, source.as_bytes());
        let mut result: HashMap<&'query str, Vec<Node<'tree>>> =
            HashMap::with_capacity(self.0.capture_names().len());
        while let Some((query_match, capture_index)) = captures.next() {
            let capture = query_match.captures[*capture_index];
            result
                .entry(self.0.capture_names()[capture.index as usize])
                .or_default()
                .push(capture.node);
        }
        result
    }
}
