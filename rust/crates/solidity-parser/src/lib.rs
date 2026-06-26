// Many parser entry points are exported through the generated CXX bridge and
// are not referenced directly from Rust code.
#![allow(dead_code)]

pub mod bridge;
mod doc_string_parser;
mod parser;
mod token;
