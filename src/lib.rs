internkey!(StringID);
pub mod il;
pub mod parse;
pub mod ssaify;
pub mod target;
mod util;

use idset::internkey;

pub mod intern {
    use std::sync::{LazyLock, RwLock};

    use idset::KeySet;

    use crate::StringID;

    static STRINTERN: LazyLock<RwLock<KeySet<StringID, String>>> =
        LazyLock::new(|| RwLock::new(KeySet::<StringID, String>::new()));

    pub fn intern_str(s: &str) -> StringID {
        let mut w = interner_write();
        match w.get_id_of(s) {
            Some(id) => id,
            None => w.get_or_intern(s.to_string()),
        }
    }
    pub fn intern_string(s: String) -> StringID {
        interner_write().get_or_intern(s)
    }

    pub fn unintern_str(id: StringID) -> Option<String> {
        interner_read().get_by_id(id).cloned()
    }
    fn interner_read() -> std::sync::RwLockReadGuard<'static, KeySet<StringID, String>> {
        STRINTERN.read().expect("strinterner.read fail")
    }
    fn interner_write() -> std::sync::RwLockWriteGuard<'static, KeySet<StringID, String>> {
        STRINTERN.write().expect("strinterner.write fail")
    }
}
