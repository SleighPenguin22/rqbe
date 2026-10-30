use idset::{InternKey, KeySet, KeyVec, UsizeLike, internkey};
use log::trace;
use paste::paste;

use crate::StringID;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub struct ILBlock {
    pub block_idx: u32,
    pub func_id: ILFunction,
}

internkey!(ILGlobalSymbol);
internkey!(ILType);
internkey!(ILGlobal);
internkey!(ILFunction);
internkey!(ILPhiNode);
internkey!(SSATemporary);

fn construct_basic_il_types() -> KeySet<ILType, ILTypeData> {
    let mut table = KeySet::new();
    table.get_or_intern(ILTypeData::I8);
    table.get_or_intern(ILTypeData::I16);
    table.get_or_intern(ILTypeData::I32);
    table.get_or_intern(ILTypeData::I64);
    table.get_or_intern(ILTypeData::F32);
    table.get_or_intern(ILTypeData::F64);
    table.get_or_intern(ILTypeData::Zero);
    table
}
#[macro_export]
macro_rules! get_type_ {
    ($typ:ident) => {
        paste! {
        #[allow(nonstandard_style)]
        pub fn [<get_type_ $typ>]  (&self) -> ILType {
            self.typs().get_id_of(&ILTypeData::$typ).expect("missing integer type")
        }
        }
    };
}

macro_rules! module_getters {
    (ILModule, $ident:ident, $keytype:ty, $valuetype:ty, set) => {
        paste! {
        pub fn [<$ident s>](&self) -> &KeySet<$keytype, $valuetype> {
            &self.ctx.[<$ident s>]
        }
        pub fn [<get_ $ident>](&self, $ident: $keytype) -> &$valuetype {
            self.ctx.[<$ident s>].get_by_id($ident).unwrap()
        }
        }
    };
    (ILModuleContext, $ident:ident, $keytype:ty, $valuetype:ty, set) => {
        paste! {
        pub fn [<$ident s>](&self) -> &KeySet<$keytype, $valuetype> {
            &self.[<$ident s>]
        }
        pub fn [<get_ $ident>](&self, $ident: $keytype) -> &$valuetype {
            self.[<$ident s>].get_by_id($ident).unwrap()
        }
        }
    };
    (ILModuleBuilder, $ident:ident, $keytype:ty, $valuetype:ty, set) => {
        module_getters!(ILModule, $ident, $keytype, $valuetype)
    };
    (ILModule, $ident:ident, $keytype:ty, $valuetype:ty, vec) => {
        paste! {
        pub fn [<$ident s>](&self) -> &KeyVec<$keytype, $valuetype> {
            &self.ctx.[<$ident s>]
        }
        pub fn [<get_ $ident>](&self, $ident: $keytype) -> &$valuetype {
            self.ctx.[<$ident s>].get($ident).unwrap()
        }
        }
    };
    (ILModuleContext, $ident:ident, $keytype:ty, $valuetype:ty, vec) => {
        paste! {
        pub fn [<$ident s>](&self) -> &KeyVec<$keytype, $valuetype> {
            &self.[<$ident s>]
        }
        pub fn [<get_ $ident>](&self, $ident: $keytype) -> &$valuetype {
            self.[<$ident s>].get($ident).unwrap()
        }
        }
    };
    (ILModuleBuilder, $ident:ident, $keytype:ty, $valuetype:ty, vec) => {
        module_getters!(ILModule, $ident, $keytype, $valuetype, vec)
    };
}
#[derive(Debug)]
pub struct ILModuleContext {
    pub(crate) symbols: KeySet<ILGlobalSymbol, ILGlobalSymbolData>,
    pub(crate) globals: KeySet<ILGlobal, ILGlobalData>,
    pub(crate) layouts: KeySet<StringID, ILLayoutData>,
    pub(crate) functions: KeyVec<ILFunction, ILFunctionData>,
    pub(crate) phi_nodes: KeySet<ILPhiNode, ILPhiNodeData>,
    pub(crate) typs: KeySet<ILType, ILTypeData>,
    temp_gen: u32,
    func_gen: u32,
}

impl ILModuleContext {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            symbols: KeySet::with_capacity(n),
            globals: KeySet::with_capacity(n),
            layouts: KeySet::with_capacity(n),
            functions: KeyVec::with_capacity(n),
            phi_nodes: KeySet::with_capacity(n),
            typs: construct_basic_il_types(),
            temp_gen: 0,
            func_gen: 0,
        }
    }
    module_getters!(ILModuleContext, layout, StringID, ILLayoutData, set);
    module_getters!(
        ILModuleContext,
        symbol,
        ILGlobalSymbol,
        ILGlobalSymbolData,
        set
    );
    module_getters!(ILModuleContext, function, ILFunction, ILFunctionData, vec);
    module_getters!(ILModuleContext, phi_node, ILPhiNode, ILPhiNodeData, set);
    module_getters!(ILModuleContext, global, ILGlobal, ILGlobalData, set);
    module_getters!(ILModuleContext, typ, ILType, ILTypeData, set);
    pub fn typs_mut(&mut self) -> &mut KeySet<ILType, ILTypeData> {
        &mut self.typs
    }
    pub fn get_function_by_name(&self, funcname: StringID) -> Option<&ILFunctionData> {
        self.functions.iter().find(|f| f.name == funcname)
    }
    pub fn layout_as_aggregate_type(&mut self, layout: StringID) -> ILType {
        self.typs.get_or_intern(ILTypeData::Aggregate(layout))
    }
    get_type_!(I8);
    get_type_!(I16);
    get_type_!(I32);
    get_type_!(I64);
    get_type_!(F32);
    get_type_!(F64);
    get_type_!(Zero);

    pub fn next_temp(&mut self) -> SSATemporary {
        let temp = SSATemporary::from_backing(self.temp_gen);
        self.temp_gen += 1;
        temp
    }

    pub fn get_block(&self, block: ILBlock) -> &ILBlockData {
        let ILBlock { block_idx, func_id } = block;
        &self.get_function(func_id).blocks[block_idx as usize]
    }

    /// Find a block by its label
    /// This can take quite a long time
    ///
    /// Labels generated by the builders are able to be retrieved quicker
    pub fn get_block_by_label(&self, block: StringID) -> Option<&ILBlockData> {
        match Self::parse_block_label(block) {
            Some(b) => Some(self.get_block(b)),
            None => self
                .functions()
                .iter()
                .find_map(|f| f.blocks.iter().find(|b| b.label == block)),
        }
    }
    fn parse_block_label(label: StringID) -> Option<ILBlock> {
        let s = crate::intern::unintern_str(label)?;
        let (_rest, spec) = s.rsplit_once(":")?;
        let (func_idx, block_idx) = spec.split_once(".")?;
        let block_idx = block_idx.parse().ok()?;
        let func_idx: usize = func_idx.parse().ok()?;

        Some(ILBlock {
            block_idx,
            func_id: ILFunction::from_usize(func_idx),
        })
    }
}

#[derive(Debug)]
struct UnionFind<K: InternKey> {
    union_find: Vec<K::Backing>,
}

impl<K: InternKey> UnionFind<K> {
    fn new() -> Self {
        Self {
            union_find: Vec::new(),
        }
    }
    fn bump(&mut self) -> usize {
        let len = self.union_find.len();
        self.union_find.push(K::Backing::from_usize(len));
        len + 1
    }
    pub fn union(&mut self, val: K, to: K) {
        let (n_val, n_to) = (val.into_usize(), to.into_usize());
        let n_max = n_val.max(n_to);
        self.ensure_exists(K::Backing::from_usize(n_max));
        let res_to = self.find(to);
        self.union_find[n_val] = res_to.into_backing();
    }
    pub fn find(&mut self, val: K) -> K {
        let mut n_val = val.into_backing();
        loop {
            self.ensure_exists(n_val);
            n_val = self.union_find[n_val.into_usize()];
            if self.union_find[n_val.into_usize()] == n_val {
                break K::from_backing(n_val);
            }
        }
    }
    fn ensure_exists(&mut self, val: K::Backing) {
        while self.union_find.len() < val.into_usize() {
            self.bump();
        }
    }
    fn clear(&mut self) {
        self.union_find.clear();
    }
}
#[derive(Debug)]
pub struct ILModule {
    pub(crate) ctx: ILModuleContext,
}

impl ILModule {
    pub fn iter_blocks<'f>(
        &self,
        func_id: ILFunction,
        func: &'f ILFunctionData,
    ) -> impl Iterator<Item = (ILBlock, &'f ILBlockData)> {
        func.blocks.iter().enumerate().map(move |(i, v)| {
            (
                ILBlock {
                    block_idx: i as u32,
                    func_id,
                },
                v,
            )
        })
    }
    pub fn entry_block_of_func(&self, f: ILFunction) -> &ILBlockData {
        &self.get_function(f).blocks[0]
    }

    pub fn ctx(&self) -> &ILModuleContext {
        &self.ctx
    }
}

impl ILModule {
    module_getters!(ILModule, layout, StringID, ILLayoutData, set);
    module_getters!(ILModule, function, ILFunction, ILFunctionData, vec);
    module_getters!(ILModule, phi_node, ILPhiNode, ILPhiNodeData, set);
    module_getters!(ILModule, global, ILGlobal, ILGlobalData, set);
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILGlobalSymbolData {
    Global(StringID),
    Func(ILFunction),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILBlockData {
    pub(crate) label: StringID,
    pub(crate) phis: Vec<(ILTemp, ILType, ILPhiNode)>,
    pub(crate) items: Vec<ILBlockItem>,
    pub(crate) terminator: ILTerminator,
}
#[derive(Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTerminator {
    #[default]
    Halt,
    Jmp(StringID),
    BranchIf(ILOperand, StringID, StringID),
    Return,
    ReturnVal(ILOperand),
    Unspecified,
    StartBLockFallThrough,
}

impl ILInstruction {
    fn imm_i64(n: u64) -> ILInstruction {
        ILInstruction::Immi64(n)
    }

    fn add(a: ILOperand, b: ILOperand) -> ILInstruction {
        ILInstruction::Add(a, b)
    }

    fn cmp_z(val: ILOperand) -> ILInstruction {
        ILInstruction::Cmp(ComparisonOp::eq, val, ILOperand::ConstInt(0))
    }
}

/// An item within a block, which can be an ILInstr assigning to a temporary,
/// a call or a memory store,
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum ILBlockItem {
    Call(ILCall),
    AssignInstr(ILTemp, ILType, ILInstruction),
    Store(ILType, ILOperand, ILOperand),
}

// we should maybe intern calls, it could be useful for the fancy kind of optimization
// were you 'specialize' functions which are often called with the same parameters.
//
// using an KeyVec might prove useful to facilitate this?
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILCall {
    pub assignee: Option<(ILTemp, ILType)>,
    pub func: StringID,
    pub args: Box<[(ILType, ILOperand)]>,
}
#[allow(nonstandard_style)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, strum::Display)]
pub enum ComparisonOp {
    slt,
    ult,
    eq,
}

/// An instruction that assign to a temporary.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum ILInstruction {
    Load(ILOperand),
    Immi64(u64),
    Immf64(u64),
    Add(ILOperand, ILOperand),
    Copy(ILOperand),
    Cmp(ComparisonOp, ILOperand, ILOperand),
}

// ILValue is proably what we will place in the E-graphs, as they are the subject of constant folding etc.
/// A value that can be an operand of an [`ILInstr`]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILOperand {
    Temp(ILTemp),
    Global(StringID),
    ConstInt(u64),
    /// an f32's bits stored verbatim to allow eq
    ConstFloat(u32),
    /// an f64's bits stored verbatim to allow eq
    ConstDouble(u64),
}

/// A symbol that can be assigned to, which can exist in SSA or non-SSA form.
///
/// Non SSA temporaries are just strings really, but we intern them to save space.
///
/// SSA Temporaries are just integers.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTemp {
    NonSSA(StringID),
    SSA(SSATemporary, StringID),
}

impl ILTemp {
    pub fn name(self) -> StringID {
        match self {
            Self::NonSSA(n) => n,
            Self::SSA(_, n) => n,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILSignatureData {
    pub(crate) param_types: Box<[ILType]>,
    pub(crate) param_temporaries: Box<[ILTemp]>,
    pub(crate) returns: ILType,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct ILPhiNodeData {
    pub(crate) incoming: Vec<(StringID, ILOperand)>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILGlobalData {
    pub linkage: ILLinkage,
    pub layout: StringID,
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
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, strum::Display)]
pub enum ILTypeData {
    #[strum(to_string = "b")]
    I8,
    #[strum(to_string = "h")]
    I16,
    #[strum(to_string = "w")]
    I32,
    #[strum(to_string = "l")]
    I64,
    #[strum(to_string = "f")]
    F32,
    #[strum(to_string = "d")]
    F64,
    Zero,
    Aggregate(StringID),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILFunctionData {
    pub linkage: ILLinkage,
    pub(crate) signature: ILSignatureData,
    pub(crate) name: StringID,
    /// Entry block is at index 0
    pub(crate) blocks: Vec<ILBlockData>,
}
#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ILLinkage {
    #[default]
    Export,
    Thread,
    ExportThread,

    Internal,
}

// pub struct ILSectionLinkage {
//     export: bool,
//     thread_local: bool,
//     section: StringID,
//     section_flags: StringID,
// }

pub mod builder;
