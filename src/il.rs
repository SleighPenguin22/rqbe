use idset::{InternKey, KeySet, KeyVec, internkey};
use paste::paste;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub struct ILBlock {
    pub block_id: u32,
    pub func_id: ILFunction,
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

fn construct_basic_il_types() -> KeySet<ILType, ILTypeData> {
    let mut table = KeySet::new();
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
    pub(crate) strings: KeySet<StringID, String>,
pub(crate)     globals: KeySet<ILGlobal, ILGlobalData>,
pub(crate)     layouts: KeySet<ILLayout, ILLayoutData>,
pub(crate)     functions: KeyVec<ILFunction, ILFunctionData>,
 pub(crate)    phi_nodes: KeySet<ILPhiNode, ILPhiNodeData>,
 pub(crate)    values: KeySet<ILValue, ILValueData>,
 pub(crate)    typs: KeySet<ILType, ILTypeData>,
    temp_gen: u32,
    func_gen: u32,
}

impl ILModuleContext {
    fn with_capacity(n: usize) -> Self {
        Self {
            symbols: KeySet::with_capacity(n),
            strings: KeySet::with_capacity(n),
            globals: KeySet::with_capacity(n),
            layouts: KeySet::with_capacity(n),
            functions: KeyVec::with_capacity(n),
            phi_nodes: KeySet::with_capacity(n),
            values: KeySet::with_capacity(n),
            typs: construct_basic_il_types(),
            temp_gen: 0,
            func_gen: 0,
        }
    }
    module_getters!(ILModuleContext, layout, ILLayout, ILLayoutData, set);
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
    module_getters!(ILModuleContext, string, StringID, String, set);
    module_getters!(ILModuleContext, value, ILValue, ILValueData, set);
    module_getters!(ILModuleContext, typ, ILType, ILTypeData, set);
    pub fn typs_mut(&mut self) -> &mut KeySet<ILType, ILTypeData> {
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
        let temp = ILTemporary::from_backing(self.temp_gen);
        self.temp_gen += 1;
        temp
    }
    pub fn intern_string(&mut self, s: &str) -> StringID {
        if let Some(id) = self.strings.get_id_of_str(s) {
            id
        } else {
            self.strings.get_or_intern(s.to_string())
        }
    }
}

#[derive(Debug)]
struct ILValueForwarding {
    forwardings: Vec<u32>,
}

impl ILValueForwarding {
    fn new() -> Self {
        Self {
            forwardings: Vec::new(),
        }
    }
    pub fn bump(&mut self) -> usize {
        let len = self.forwardings.len();
        self.forwardings.push(len as u32);
        len + 1
    }
    pub fn forward(&mut self, val: ILValue, to: ILValue) {
        let (n_val, n_to) = (val.into_backing(), to.into_backing());
        let n_max = n_val.max(n_to);
        self.ensure_exists(n_max);
        let res_to = self.resolve(to);
        self.forwardings[n_val as usize] = res_to.into_backing();
    }
    pub fn resolve(&mut self, val: ILValue) -> ILValue {
        let mut n_val = val.into_backing();
        loop {
            self.ensure_exists(n_val);
            n_val = self.forwardings[n_val as usize];
            if self.forwardings[n_val as usize] == n_val {
                break ILValue::from_backing(n_val);
            }
        }
    }
    fn ensure_exists(&mut self, val: u32) {
        while self.forwardings.len() < val as usize {
            self.bump();
        }
    }
    fn clear(&mut self) {
        self.forwardings.clear();
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
                    block_id: i as u32,
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
    module_getters!(ILModule, layout, ILLayout, ILLayoutData, set);
    module_getters!(ILModule, function, ILFunction, ILFunctionData, vec);
    module_getters!(ILModule, phi_node, ILPhiNode, ILPhiNodeData, set);
    module_getters!(ILModule, global, ILGlobal, ILGlobalData, set);
    module_getters!(ILModule, string, StringID, String, set);
    module_getters!(ILModule, value, ILValue, ILValueData, set);
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILGlobalSymbolData {
    Global(StringID),
    Func(ILFunction),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILBlockData {
    pub(crate) label: StringID,
    pub(crate) phis: Vec<(ILAssignee, ILPhiNode)>,
    pub(crate) items: Vec<(ILAssignee, ILValue)>,
    pub(crate) terminator: ILTerminator,
}
#[derive(Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTerminator {
    #[default]
    Halt,
    Jmp(StringID),
    BranchIf(ILAssignee, StringID, StringID),
    Return,
    ReturnVal(ILAssignee),
    Unspecified,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILValueData {
    pub(crate) kind: ILValueDataKind,
}
impl ILValueData {
    fn no_forward(kind: ILValueDataKind) -> Self {
        Self { kind }
    }
    fn imm_i64(n: u64) -> ILValueData {
        Self::no_forward(ILValueDataKind::Immi64(n))
    }

    fn add(a: ILAssignee, b: ILAssignee) -> ILValueData {
        Self::no_forward(ILValueDataKind::Add(a, b))
    }

    fn cmp_z(val: ILAssignee) -> ILValueData {
        Self::no_forward(ILValueDataKind::CmpZ(val))
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum ILValueDataKind {
    Load(usize),
    Store(ILAssignee, usize),
    Immi64(u64),
    Immf64(u64),
    Add(ILAssignee, ILAssignee),
    Call(ILFunction, Vec<(ILType, ILAssignee)>),
    Temp(ILAssignee),
    NonSSATemp(StringID),
    Global(StringID),
    CmpZ(ILAssignee),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILAssignee {
    NonSSA(StringID),
    SSA(ILTemporary),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILSignatureData {
    pub(crate) param_types: Vec<ILType>,
    pub(crate) param_temporaries: Vec<ILAssignee>,
    pub(crate) returns: ILType,
}
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct ILPhiNodeData {
    pub(crate) incoming: Vec<(StringID, ILAssignee)>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILGlobalData {
    pub linkage: ILLinkage,
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
}

pub mod builder;
