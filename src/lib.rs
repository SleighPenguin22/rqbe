internkey!(StringID);
pub mod il;
pub mod parse;
pub mod ssaify;
pub mod target;
mod util;

use keyvec::internkey;

pub mod intern {
    type Table = KeySet<StringID, Box<str>>;
    use std::sync::{LazyLock, RwLock};

    use keyvec::KeySet;

    use crate::StringID;

    static STRINTERN: LazyLock<RwLock<Table>> = LazyLock::new(|| RwLock::new(KeySet::new()));

    pub fn intern_str(s: &str) -> StringID {
        let mut w = interner_write();
        w.get_key_of(s)
            .unwrap_or_else(|| w.get_key(s.to_owned().into_boxed_str()))
    }
    pub fn intern_string(s: String) -> StringID {
        interner_write().get_key(s.into_boxed_str())
    }
    pub fn intern_boxstr(s: Box<str>) -> StringID {
        interner_write().get_key(s)
    }

    pub fn unintern_boxstr(id: StringID) -> Box<str> {
        interner_read()[id].clone()
    }
    pub fn unintern_string(id: StringID) -> String {
        unintern_boxstr(id).into_string()
    }
    fn interner_read() -> std::sync::RwLockReadGuard<'static, Table> {
        STRINTERN.read().expect("strinterner.read fail")
    }
    fn interner_write() -> std::sync::RwLockWriteGuard<'static, Table> {
        STRINTERN.write().expect("strinterner.write fail")
    }
}
