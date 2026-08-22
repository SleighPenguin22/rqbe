use indexmap::IndexSet;
use indexmap::set::MutableValues;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use crate::il::{
    ILBlock, ILBlockData, ILDataLayoutKind, ILFunctionData, ILGlobal, ILGlobalData, ILGlobalSymbol,
    ILLayout, ILLayoutData, ILModule, ILModuleContext, ILPhiNode, ILTemporary, ILTerminator,
    ILType, ILValue, ILValueData, StringID,
};

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

pub trait DisplayModuleItem {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String;
}
fn join_display_module_items<T: DisplayModuleItem>(
    sep: &str,
    items: &[T],
    ctx: &ILModuleContext,
) -> String {
    items
        .iter()
        .map(|item| item.display_module_item(ctx))
        .collect::<Vec<_>>()
        .join(sep)
}
fn join_display_module_items_tuple<T: DisplayModuleItem, U: DisplayModuleItem>(
    item_sep: &str,
    inter_item_sep: &str,
    items: &[(T, U)],
    ctx: &ILModuleContext,
) -> String {
    items
        .iter()
        .map(|(t, u)| {
            format!(
                "{}{inter_item_sep}{}",
                t.display_module_item(ctx),
                u.display_module_item(ctx)
            )
        })
        .collect::<Vec<_>>()
        .join(item_sep)
}

impl DisplayModuleItem for ILGlobal {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_global(*self);
        let layout = ctx.get_layout(data.layout);
        let name = ctx.get_string(data.name);
        format!(
            "${name} : {} = {:?}\n",
            layout.display_module_item(ctx),
            data.bits
        )
    }
}
impl DisplayModuleItem for ILGlobalData {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let layout = ctx.get_layout(self.layout);
        let name = ctx.get_string(self.name);
        format!(
            "${name} : {} = {:?}\n",
            layout.display_module_item(ctx),
            self.bits
        )
    }
}
impl DisplayModuleItem for ILLayout {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        ctx.get_layout(*self).display_module_item(ctx)
    }
}
impl DisplayModuleItem for ILLayoutData {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self.layout_kind {
            ILDataLayoutKind::Padded => {
                format!(
                    "Padded {{{}}}",
                    join_display_module_items(", ", &self.fields, ctx)
                )
            }
            ILDataLayoutKind::Packed => {
                format!(
                    "Packed {{{}}}",
                    join_display_module_items(", ", &self.fields, ctx)
                )
            }
        }
    }
}
impl DisplayModuleItem for ILType {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let typ = ctx.get_typ(*self);
        match typ {
            crate::il::ILTypeData::I8 => "I8".to_string(),
            crate::il::ILTypeData::I16 => "I16".to_string(),
            crate::il::ILTypeData::I32 => "I32".to_string(),
            crate::il::ILTypeData::I64 => "I64".to_string(),
            crate::il::ILTypeData::F32 => "F32".to_string(),
            crate::il::ILTypeData::F64 => "F64".to_string(),
            crate::il::ILTypeData::Zero => "Zero".to_string(),
            crate::il::ILTypeData::Aggregate(illayout) => illayout.display_module_item(ctx),
            crate::il::ILTypeData::Pointer => "Ptr".to_string(),
        }
    }
}

impl DisplayModuleItem for ILBlockData {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let sep = "\n    ";
        let items = join_display_module_items_tuple(sep, " = ", &self.items, ctx);
        let term = self.terminator.display_module_item(ctx);
        format!("  {}:\n    {items}{sep}{term}\n", self.label)
    }
}
impl DisplayModuleItem for ILBlock {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        format!("f{}b{}", self.func_id, self.block_id)
    }
}

impl DisplayModuleItem for ILTemporary {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        format!("%{}", self.destruct())
    }
}

impl DisplayModuleItem for ILTerminator {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self {
            ILTerminator::BuildingNotFinished => "!!BuilderNotFinished!!".to_string(),
            ILTerminator::Halt => "hlt".to_string(),
            ILTerminator::Jmp(ilblock) => {
                format!("jmp {}", ilblock.display_module_item(ctx))
            }
            ILTerminator::JmpZ(ilvalue, ilblock, ilblock1) => {
                let v = ilvalue.display_module_item(ctx);
                let bt = ilblock.display_module_item(ctx);
                let bf = ilblock1.display_module_item(ctx);
                format!("jmpz {v}, {bt}, {bf}")
            }
            ILTerminator::Return => "ret".to_string(),
            ILTerminator::ReturnVal(ilvalue) => {
                format!("ret %{}", ilvalue.destruct())
            }
            _ => unimplemented!(),
        }
    }
}
impl DisplayModuleItem for ILValue {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_value(*self);
        let suffix = format!("\t// {:?}", self);
        let mut stem = match data {
            ILValueData::Load(i) => format!("load {i:x}"),
            ILValueData::Store(ilvalue, i) => {
                format!("store {}, {i}", ilvalue.destruct())
            }
            ILValueData::ImmInt(i) => format!("immi64 {i}"),
            ILValueData::ImmFloat(f) => format!("immf64 {f}"),
            ILValueData::Add(ilvalue, ilvalue1) => {
                format!(
                    "add {}, {}",
                    ilvalue.display_module_item(ctx),
                    ilvalue1.display_module_item(ctx)
                )
            }
            ILValueData::RetNone => "ret".to_string(),
            ILValueData::Call(ilsymbol) => {
                let fname = ctx.get_function(*ilsymbol);
                format!("call {}", fname.display_module_item(ctx))
            }
            ILValueData::Phi(ilphi_node) => ilphi_node.display_module_item(ctx),
            ILValueData::Temp(id) => id.display_module_item(ctx),
            ILValueData::Global(string_id) => format!("${}", string_id.display_module_item(ctx)),
        };
        stem += &suffix;
        stem
    }
}

impl DisplayModuleItem for StringID {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        ctx.get_string(*self).clone()
    }
}

impl DisplayModuleItem for ILFunctionData {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let name = self.name.display_module_item(ctx);
        let mut param_buffer = vec![];
        for (typ, symb) in self
            .signature
            .param_types
            .iter()
            .zip(&self.signature.param_temporaries)
        {
            let typ = typ.display_module_item(ctx);
            let symb = symb.display_module_item(ctx);
            param_buffer.push(format!("{symb}: {typ}"))
        }
        let params = param_buffer.join(", ");
        let returns = self.signature.returns.display_module_item(ctx);
        let blocks = join_display_module_items("\n", &self.blocks, ctx);
        format!("${name} ({params}) -> {returns} {{\n{blocks}\n}}\n")
    }
}
impl DisplayModuleItem for ILGlobalSymbol {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_symbol(*self);
        match data {
            crate::il::ILGlobalSymbolData::Func(string_id) => {
                let funcname = ctx.get_function(*string_id).name;
                format!("${}", funcname.display_module_item(ctx))
            }
            crate::il::ILGlobalSymbolData::Global(string_id) => {
                format!("${}", string_id.display_module_item(ctx))
            }
        }
    }
}
impl DisplayModuleItem for ILPhiNode {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_phi_node(*self);
        let items: Vec<String> = data
            .incoming
            .iter()
            .map(|(block, val)| format!("{}: {}", block.display_module_item(ctx), val.destruct()))
            .collect();
        let items = items.join(", ");
        format!("    P{} := [{items}]", self.destruct())
    }
}

impl DisplayModuleItem for ILModule {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let mut buffer = String::with_capacity(256);
        for glob in self.globals().iter() {
            buffer += &glob.display_module_item(ctx);
        }
        for func in self.functions().iter() {
            buffer += &func.display_module_item(ctx);
        }
        buffer
    }
}
impl ILModule {
    pub fn display_module(&self) -> String {
        self.display_module_item(&self.ctx)
    }
}
