internkey!(StringID);

static STRINTERN: LazyLock<RwLock<KeySet<StringID, String>>> =
    LazyLock::new(|| RwLock::new(KeySet::<StringID, String>::new()));

fn intern_str(s: &str) -> StringID {
    let mut w = STRINTERN.write().expect("strinterner borrow fail");
    match w.get_id_of(s) {
        Some(id) => id,
        None => w.get_or_intern(s.to_string()),
    }
}
fn intern_string(s: String) -> StringID {
    STRINTERN
        .write()
        .expect("strinterner borrow fail")
        .get_or_intern(s)
}

fn unintern_str(id: StringID) -> Option<String> {
    STRINTERN
        .read()
        .expect("strintern borrow fail")
        .get_by_id(id)
        .cloned()
}

pub mod il;
pub mod parse;
pub mod ssaify;
pub mod target;
mod util;
use std::sync::{LazyLock, RwLock};

use idset::{KeySet, internkey};
