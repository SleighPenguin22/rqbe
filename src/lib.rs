internkey!(StringID);
pub mod il;
pub mod parse;
pub mod ssaify;
pub mod target;
mod util;

use idset::internkey;

pub mod intern {
    type Table = KeySet<StringID, Box<str>>;
    use std::sync::{LazyLock, RwLock};

    use idset::KeySet;

    use crate::StringID;

    static STRINTERN: LazyLock<RwLock<Table>> = LazyLock::new(|| RwLock::new(KeySet::new()));

    pub fn intern_str(s: &str) -> StringID {
        let mut w = interner_write();
        match w.get_id_of(s) {
            Some(id) => id,
            None => w.get_or_intern(s.to_owned().into_boxed_str()),
        }
    }
    pub fn intern_string(s: String) -> StringID {
        interner_write().get_or_intern(s.into_boxed_str())
    }
    pub fn intern_boxstr(s: Box<str>) -> StringID {
        interner_write().get_or_intern(s)
    }

    pub fn unintern_boxstr(id: StringID) -> Option<Box<str>> {
        interner_read().get_by_id(id).cloned()
    }
    pub fn unintern_string(id: StringID) -> Option<String> {
        interner_read()
            .get_by_id(id)
            .cloned()
            .map(|s| s.into_string())
    }
    fn interner_read() -> std::sync::RwLockReadGuard<'static, Table> {
        STRINTERN.read().expect("strinterner.read fail")
    }
    fn interner_write() -> std::sync::RwLockWriteGuard<'static, Table> {
        STRINTERN.write().expect("strinterner.write fail")
    }
}
