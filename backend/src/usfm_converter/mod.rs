//! Port of [py-usfm-grammar's converter](https://github.com/Bridgeconn/usfm-grammar/blob/master/py-usfm-parser/src/usfm_grammar/usfm_parser.py) to Rust

mod query;
mod usfm_parser;
mod usj_generator;

pub use usfm_parser::{FatalUsfmError, UsfmParser};
