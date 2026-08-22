use indexmap::IndexSet;
use indexmap::set::MutableValues;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use crate::{il::ILModule, util::pretty_print::DisplayModuleItem};

pub trait InternKey: Copy + Eq {
    fn construct(n: u32) -> Self;
    fn destruct(self) -> u32;
}

#[macro_export]
macro_rules! internkey {
    ($typ:ident) => {
        #[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
        #[repr(transparent)]
        pub struct $typ(u32);
        impl_internkey!($typ);
    };
}
#[macro_export]
macro_rules! impl_internkey {
    ($typ:path) => {
        impl InternKey for $typ {
            fn construct(n: u32) -> Self {
                $typ(n)
            }
            fn destruct(self) -> u32 {
                self.0
            }
        }
    };
}

pub struct InternTable<ID: InternKey, T: Hash + Eq> {
    pub set: IndexSet<T>,
    _phantom: PhantomData<ID>,
}

impl<ID: InternKey, T: Hash + Eq> InternTable<ID, T> {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            set: IndexSet::with_capacity(n),
            _phantom: PhantomData,
        }
    }
    pub fn new() -> Self {
        Self {
            set: IndexSet::new(),
            _phantom: PhantomData,
        }
    }
    pub fn get_or_intern(&mut self, value: T) -> ID {
        let (idx, _existed) = self.set.insert_full(value);
        ID::construct(idx as u32)
    }
    pub fn get_panicking(&self, value: T) -> ID {
        let idx = self
            .set
            .get_index_of(&value)
            .expect("value not in intern table");
        ID::construct(idx as u32)
    }
    pub fn contains(&self, value: &T) -> bool {
        self.set.contains(value)
    }

    pub fn get_by_id_mut(&mut self, id: ID) -> Option<&mut T> {
        let idx = id.destruct() as usize;
        self.set.get_index_mut2(idx)
    }
    pub fn get_by_id(&self, id: ID) -> Option<&T> {
        let idx = id.destruct() as usize;
        self.set.get_index(idx)
    }
    pub fn iter(&self) -> indexmap::set::Iter<'_, T> {
        self.set.iter()
    }
}

impl<ID: InternKey, T: Hash + Eq + Clone> InternTable<ID, T> {
    /// Perform some modification on an entry, and reinsert it.
    ///
    /// This method is useful if multiple users share some common `ID`
    /// (like a string in `InternTable<usize, String>`),
    /// but one of the users wants to modify their `T` while not touching the others.
    ///
    /// using `get_mut_by_id`:
    /// ```ignore
    /// let mut table = InternTable::new();
    /// let idA = table.intern(String::from("bob"));
    /// let idB = table.intern(String::from("bob"));
    /// let idA_string = table.get_by_id_mut(idA).unwrap().push_str("cat");
    /// // idA, idB => "bobcat"
    ///
    /// let idA_string = table.get_by_id_mut(idA).unwrap().push_str("cat");
    /// // idA, idB => "bobcatcat"
    ///
    /// ```
    ///
    /// But using `clone_modify_reintern`:
    /// ```ignore
    ///
    /// let mut table = InternTable::new();
    /// let idA = table.intern(String::from("bob"));
    /// let idB = table.intern(String::from("bob"));
    ///
    /// let idA = table.clone_modify_reintern(idA, |s| s.push_str("cat"));
    /// // idB => "bob"
    /// // idA => "bobcat"
    /// assert_ne!(idA, idB);
    /// ```
    ///
    /// returns the ID that the reinterned item got assigned.
    /// if no changes to the `T` are made, this will return the `id` the method was given
    pub fn clone_modify_reintern<F: FnOnce(T) -> T>(&mut self, id: ID, f: F) -> Option<ID> {
        if let Some(shared) = self.get_by_id(id) {
            let owned = shared.clone();
            let modified = f(owned);

            Some(self.get_or_intern(modified))
        } else {
            None
        }
    }
}

impl<ID: InternKey + Debug, T: Hash + Eq + Debug> std::fmt::Debug for InternTable<ID, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_set();
        for (id_num, item) in self.set.iter().enumerate() {
            let id = ID::construct(id_num as u32);
            d.entry(&(id, item));
        }
        d.finish()
    }
}

impl<ID: InternKey, T: Hash + Eq> IntoIterator for InternTable<ID, T> {
    type Item = T;
    type IntoIter = <IndexSet<T> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.set.into_iter()
    }
}
impl ILModule {
    pub fn display_module(&self) -> String {
        self.display_module_item(&self.ctx)
    }
}
mod pretty_print;
