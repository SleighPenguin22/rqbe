use paste::paste;

use crate::{
    impl_internkey, internkey,
    util::{InternKey, InternTable},
};

internkey!(ILSymbol);
internkey!(ILBlock);
internkey!(ILType);
internkey!(ILValue);
internkey!(StringID);
internkey!(ILGlobal);
internkey!(ILLayout);
internkey!(ILFunction);
internkey!(ILPhiNode);

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
    symbols: InternTable<ILSymbol, ILSymbolData>,
    strings: InternTable<StringID, String>,
    blocks: InternTable<ILBlock, ILBlockData>,
    globals: InternTable<ILGlobal, ILGlobalData>,
    layouts: InternTable<ILLayout, ILLayoutData>,
    functions: InternTable<ILFunction, ILFunctionData>,
    phi_nodes: InternTable<ILPhiNode, ILPhiNodeData>,
    values: InternTable<ILValue, ILValueData>,
    typs: InternTable<ILType, ILTypeData>,
}

impl ILModuleContext {
    fn with_capacity(n: usize) -> Self {
        Self {
            symbols: InternTable::with_capacity(n),
            strings: InternTable::with_capacity(n),
            blocks: InternTable::with_capacity(n),
            globals: InternTable::with_capacity(n),
            layouts: InternTable::with_capacity(n),
            functions: InternTable::with_capacity(n),
            phi_nodes: InternTable::with_capacity(n),
            values: InternTable::with_capacity(n),
            typs: construct_basic_il_types(),
        }
    }
    module_getters!(ILModuleContext, layout, ILLayout, ILLayoutData);
    module_getters!(ILModuleContext, symbol, ILSymbol, ILSymbolData);
    module_getters!(ILModuleContext, block, ILBlock, ILBlockData);
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
}

#[derive(Debug)]
pub struct ILModule {
    pub(crate) ctx: ILModuleContext,
}

impl ILModule {
    module_getters!(ILModule, layout, ILLayout, ILLayoutData);
    module_getters!(ILModule, block, ILBlock, ILBlockData);
    module_getters!(ILModule, function, ILFunction, ILFunctionData);
    module_getters!(ILModule, phi_node, ILPhiNode, ILPhiNodeData);
    module_getters!(ILModule, global, ILGlobal, ILGlobalData);
    module_getters!(ILModule, string, StringID, String);
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILSymbolData {
    Global(StringID),
    Temporary(u32, StringID),
    Func(StringID),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILBlockData {
    pub(crate) label: String,
    pub(crate) items: Vec<ILValue>,
    pub(crate) terminator: ILTerminator,
}
#[derive(Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTerminator {
    #[default]
    BuilderNotFinished,
    Halt,
    Jmp(ILBlock),
    JmpZ(ILValue, ILBlock, ILBlock),
    Return,
    ReturnVal(ILValue),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILValueData {
    Load(usize),
    Store(ILValue, usize),
    ImmInt(u64),
    ImmFloat(u64),
    Add(ILValue, ILValue),
    RetNone,
    Call(ILSymbol),
    Phi(ILPhiNode),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILSignatureData {
    pub(crate) param_types: Vec<ILType>,
    pub(crate) param_symbols: Vec<ILSymbol>,
    pub(crate) returns: ILType,
}
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct ILPhiNodeData {
    incoming: Vec<(ILSymbol, ILValue)>,
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
    pub(crate) blocks: Vec<ILBlock>,
}

pub mod builder;

#[cfg(test)]
mod tests {
    use crate::il::builder::ILModuleBuilder;

    use super::*;

    #[test]
    fn test_name() {
        let mut b = ILModuleBuilder::start();
        let bob = b
            .add_global_data()
            .with_name("bob")
            .build_layout(ILDataLayoutKind::Packed)
            .add_field(get_type_I8())
            .add_field(get_type_I8())
            .finish_layout()
            .with_bits(&[2, 2])
            .finish_global();
    }
}
