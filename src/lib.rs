internkey!(StringKey);
impl StringKey {
    fn unintern(self) -> Box<str> {
        crate::intern::unintern_boxstr(self)
    }
}
pub mod il;
pub mod parse;
pub mod ssaify;
pub mod target;
pub mod typecheck;
mod util;
use keyvec::internkey;

pub mod intern {
    type Table = KeySet<StringKey, Box<str>>;
    use std::sync::{LazyLock, RwLock};

    use keyvec::KeySet;

    use crate::StringKey;

    pub(crate) static STRINTERN: LazyLock<RwLock<Table>> =
        LazyLock::new(|| RwLock::new(KeySet::new()));

    pub fn intern_str(s: &str) -> StringKey {
        let mut w = interner_write();
        w.get_key_of(s)
            .unwrap_or_else(|| w.get_key(Box::<str>::from(s)))
    }
    pub fn intern_string(s: String) -> StringKey {
        interner_write().get_key(s.into_boxed_str())
    }
    pub fn intern_boxstr(s: Box<str>) -> StringKey {
        interner_write().get_key(s)
    }

    pub fn unintern_boxstr(id: StringKey) -> Box<str> {
        interner_read()[id].clone()
    }
    pub fn unintern_string(id: StringKey) -> String {
        unintern_boxstr(id).into_string()
    }
    pub(crate) fn interner_read() -> std::sync::RwLockReadGuard<'static, Table> {
        STRINTERN.read().expect("strinterner.read fail")
    }
    pub(crate) fn interner_write() -> std::sync::RwLockWriteGuard<'static, Table> {
        STRINTERN.write().expect("strinterner.write fail")
    }
}
