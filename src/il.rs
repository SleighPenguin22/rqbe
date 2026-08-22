use paste::paste;

use crate::{
    impl_internkey, internkey,
    util::{InternKey, InternTable},
};

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub struct ILBlock {
    pub block_id: u32,
    pub func_id: u32,
}

internkey!(ILGlobalSymbol);
internkey!(ILType);
internkey!(ILValue);
internkey!(StringID);
internkey!(ILGlobal);
internkey!(ILLayout);
internkey!(ILFunction);
internkey!(ILPhiNode);
internkey!(ILTemporary);

fn construct_basic_il_types() -> InternTable<ILType, ILTypeData> {
    let mut table = InternTable::new();
    table.get_or_intern(ILTypeData::I8);
    table.get_or_intern(ILTypeData::I16);
    table.get_or_intern(ILTypeData::I32);
    table.get_or_intern(ILTypeData::I64);
    table.get_or_intern(ILTypeData::F32);
    table.get_or_intern(ILTypeData::F64);
    table.get_or_intern(ILTypeData::Zero);
    table.get_or_intern(ILTypeData::Pointer);
    table
}
#[macro_export]
macro_rules! get_type_ {
    ($typ:ident) => {
        paste! {
        #[allow(nonstandard_style)]
        pub fn [<get_type_ $typ>]  (&self) -> ILType {
            self.typs().get_panicking(ILTypeData::$typ)
        }
        }
    };
}

macro_rules! module_getters {
    (ILModule, $ident:ident, $keytype:ty, $valuetype:ty) => {
        paste! {
        pub fn [<$ident s>](&self) -> &InternTable<$keytype, $valuetype> {
            &self.ctx.[<$ident s>]
        }
        pub fn [<get_ $ident>](&self, $ident: $keytype) -> &$valuetype {
            self.ctx.[<$ident s>].get_by_id($ident).unwrap()
        }
        }
    };
    (ILModuleContext, $ident:ident, $keytype:ty, $valuetype:ty) => {
        paste! {
        pub fn [<$ident s>](&self) -> &InternTable<$keytype, $valuetype> {
            &self.[<$ident s>]
        }
        pub fn [<get_ $ident>](&self, $ident: $keytype) -> &$valuetype {
            self.[<$ident s>].get_by_id($ident).unwrap()
        }
        }
    };
    (ILModuleBuilder, $ident:ident, $keytype:ty, $valuetype:ty) => {
        module_getters!(ILModule, $ident, $keytype, $valuetype)
    };
}
#[derive(Debug)]
pub struct ILModuleContext {
    symbols: InternTable<ILGlobalSymbol, ILGlobalSymbolData>,
    strings: InternTable<StringID, String>,
    globals: InternTable<ILGlobal, ILGlobalData>,
    layouts: InternTable<ILLayout, ILLayoutData>,
    functions: InternTable<ILFunction, ILFunctionData>,
    phi_nodes: InternTable<ILPhiNode, ILPhiNodeData>,
    values: InternTable<ILValue, ILValueData>,
    typs: InternTable<ILType, ILTypeData>,
    temp_gen: u32,
    func_gen: u32,
}

impl ILModuleContext {
    fn with_capacity(n: usize) -> Self {
        Self {
            symbols: InternTable::with_capacity(n),
            strings: InternTable::with_capacity(n),
            globals: InternTable::with_capacity(n),
            layouts: InternTable::with_capacity(n),
            functions: InternTable::with_capacity(n),
            phi_nodes: InternTable::with_capacity(n),
            values: InternTable::with_capacity(n),
            typs: construct_basic_il_types(),
            temp_gen: 0,
            func_gen: 0,
        }
    }
    module_getters!(ILModuleContext, layout, ILLayout, ILLayoutData);
    module_getters!(ILModuleContext, symbol, ILGlobalSymbol, ILGlobalSymbolData);
    module_getters!(ILModuleContext, function, ILFunction, ILFunctionData);
    module_getters!(ILModuleContext, phi_node, ILPhiNode, ILPhiNodeData);
    module_getters!(ILModuleContext, global, ILGlobal, ILGlobalData);
    module_getters!(ILModuleContext, string, StringID, String);
    module_getters!(ILModuleContext, value, ILValue, ILValueData);
    module_getters!(ILModuleContext, typ, ILType, ILTypeData);
    pub fn typs_mut(&mut self) -> &mut InternTable<ILType, ILTypeData> {
        &mut self.typs
    }
    get_type_!(I8);
    get_type_!(I16);
    get_type_!(I32);
    get_type_!(I64);
    get_type_!(F32);
    get_type_!(F64);
    get_type_!(Zero);

    pub fn next_temp(&mut self) -> ILTemporary {
        let temp = ILTemporary::construct(self.temp_gen);
        self.temp_gen += 1;
        temp
    }
}

#[derive(Debug)]
pub struct ILModule {
    pub(crate) ctx: ILModuleContext,
}

impl ILModule {
    module_getters!(ILModule, layout, ILLayout, ILLayoutData);
    module_getters!(ILModule, function, ILFunction, ILFunctionData);
    module_getters!(ILModule, phi_node, ILPhiNode, ILPhiNodeData);
    module_getters!(ILModule, global, ILGlobal, ILGlobalData);
    module_getters!(ILModule, string, StringID, String);
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILGlobalSymbolData {
    Global(StringID),
    Func(ILFunction),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILBlockData {
    pub(crate) for_function: u32,
    pub(crate) label: String,
    pub(crate) items: Vec<(ILTemporary, ILValue)>,
    pub(crate) terminator: ILTerminator,
}
#[derive(Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTerminator {
    #[default]
    BuildingNotFinished,
    BuildingJmp(ILBlock),
    BuildingJmpZ(ILBlock, ILBlock, ILBlock),
    Halt,
    Jmp(ILBlock),
    JmpZ(ILTemporary, ILBlock, ILBlock),
    Return,
    ReturnVal(ILTemporary),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILValueData {
    Load(usize),
    Store(ILTemporary, usize),
    ImmInt(u64),
    ImmFloat(u64),
    Add(ILTemporary, ILTemporary),
    RetNone,
    Call(ILFunction),
    Phi(ILPhiNode),
    Temp(ILTemporary),
    Global(StringID),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILSignatureData {
    pub(crate) param_types: Vec<ILType>,
    pub(crate) param_temporaries: Vec<ILTemporary>,
    pub(crate) returns: ILType,
}
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct ILPhiNodeData {
    pub(crate) incoming: Vec<(ILBlock, ILTemporary)>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILGlobalData {
    pub layout: ILLayout,
    pub name: StringID,
    pub bits: Vec<u8>,
}
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILLayoutData {
    pub fields: Vec<ILType>,
    pub layout_kind: ILDataLayoutKind,
}
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILDataLayoutKind {
    Padded,
    Packed,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTypeData {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Zero,
    Aggregate(ILLayout),
    Pointer,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILFunctionData {
    pub(crate) signature: ILSignatureData,
    pub(crate) name: StringID,
    /// Entry block is at index 0
    pub(crate) blocks: Vec<ILBlockData>,
}

pub mod builder;
