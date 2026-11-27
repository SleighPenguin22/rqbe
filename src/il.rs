use keyvec::{KeySet, KeyVec, internkey};

use crate::StringKey;

internkey!(ILGlobalSymbol);

internkey!(ILGlobal);

internkey!(ILFunction);

internkey!(SSATemporary);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ILBlockIdentifier(pub ILFunction, pub StringKey);

impl ILBlockIdentifier {
    pub const fn func(self) -> ILFunction {
        self.0
    }
    pub const fn label(self) -> StringKey {
        self.1
    }
}

#[derive(Debug)]
pub struct ILModuleContext {
    pub(crate) symbols: KeySet<ILGlobalSymbol, ILGlobalSymbolData>,
    pub(crate) globals: KeySet<ILGlobal, ILDataDefinition>,
    pub(crate) functions: KeyVec<ILFunction, ILFunctionData>,
}

impl ILModuleContext {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            symbols: KeySet::with_capacity(n),
            globals: KeySet::with_capacity(n),
            functions: KeyVec::with_capacity(n),
        }
    }
    pub fn symbols(&self) -> &KeySet<ILGlobalSymbol, ILGlobalSymbolData> {
        &self.symbols
    }
    pub fn get_symbol(&self, symbol: ILGlobalSymbol) -> &ILGlobalSymbolData {
        &self.symbols[symbol]
    }
    pub fn functions(&self) -> &KeyVec<ILFunction, ILFunctionData> {
        &self.functions
    }
    pub fn get_function(&self, function: ILFunction) -> &ILFunctionData {
        &self.functions[function]
    }
    pub fn globals(&self) -> &KeySet<ILGlobal, ILDataDefinition> {
        &self.globals
    }
    pub fn get_global(&self, global: ILGlobal) -> &ILDataDefinition {
        &self.globals[global]
    }
    pub fn find_function(&self, funcname: StringKey) -> Option<&ILFunctionData> {
        self.functions.values().find(|f| f.name == funcname)
    }
    pub fn function_entry_block(&self, function: ILFunction) -> &ILBlockData {
        self.functions[function].entry_block()
    }
    pub fn function_entry_block_mut(&mut self, function: ILFunction) -> &ILBlockData {
        self.functions[function].entry_block_mut()
    }

    pub fn all_symbols(&self) -> &KeySet<ILGlobalSymbol, ILGlobalSymbolData> {
        &self.symbols
    }
    pub fn layout_as_aggregate_type(&mut self, layout: StringKey) -> ILType {
        ILType::Aggregate(layout)
    }

    #[allow(nonstandard_style)]
    pub fn get_type_I8(&self) -> ILType {
        ILType::I8
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I16(&self) -> ILType {
        ILType::I16
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I32(&self) -> ILType {
        ILType::I32
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I64(&self) -> ILType {
        ILType::I64
    }
    #[allow(nonstandard_style)]
    pub fn get_type_F32(&self) -> ILType {
        ILType::F32
    }
    #[allow(nonstandard_style)]
    pub fn get_type_F64(&self) -> ILType {
        ILType::F64
    }

    pub(crate) fn get_func_name(&self, func: ILFunction) -> StringKey {
        self.functions[func].name
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
    ) -> impl Iterator<Item = (ILBlockIdentifier, &'f ILBlockData)> {
        func.blocks
            .iter()
            .map(move |v| (ILBlockIdentifier(func_id, v.label), v))
    }
    pub fn entry_block_of_func(&self, f: ILFunction) -> &ILBlockData {
        &self.ctx.functions[f].blocks[0]
    }
    pub fn get_func_name(&self, f: ILFunction) -> StringKey {
        self.ctx.functions[f].name
    }

    pub fn ctx(&self) -> &ILModuleContext {
        &self.ctx
    }

    pub fn get_block_mut(
        &mut self,
        ILBlockIdentifier(func_id, string_key): ILBlockIdentifier,
    ) -> &mut ILBlockData {
        self.ctx.functions[func_id].find_block_mut(string_key)
    }
    pub fn get_block(
        &self,
        ILBlockIdentifier(func_id, string_key): ILBlockIdentifier,
    ) -> &ILBlockData {
        self.ctx.functions[func_id].find_block(string_key)
    }

    pub(crate) fn entry_identifier_of_func(&self, f: ILFunction) -> ILBlockIdentifier {
        let data = &self.functions()[f];
        let label = data.entry_block().label;
        ILBlockIdentifier(f, label)
    }

    pub(crate) fn iter_block_labels(
        &self,
        f: ILFunction,
    ) -> impl Iterator<Item = ILBlockIdentifier> {
        self.ctx.functions[f]
            .blocks
            .iter()
            .map(move |b| ILBlockIdentifier(f, b.label))
    }
}

impl ILModule {
    pub fn functions(&self) -> &KeyVec<ILFunction, ILFunctionData> {
        &self.ctx.functions
    }
    pub fn get_function(&self, function: ILFunction) -> &ILFunctionData {
        self.ctx.functions.get(function).unwrap()
    }
    pub fn globals(&self) -> &KeySet<ILGlobal, ILDataDefinition> {
        &self.ctx.globals
    }
    pub fn get_global(&self, global: ILGlobal) -> &ILDataDefinition {
        self.ctx.globals.get_by_key(global).unwrap()
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILGlobalSymbolData {
    Global(StringKey),
    Func(ILFunction),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILBlockData {
    /// The label of the block.
    ///
    /// Automatically generated labels have a name in the format of `BB<N>`, where N is an
    /// integer unique to this block, user defined labels are predixed with `BB<N>.`, like
    /// `BB3.mylabel`, N is once again a unique label.
    pub label: StringKey,
    // we keep these as Vec<T>'s instead of Box<[T]>'s
    // since they might mutate during optimizations
    pub phis: Vec<ILPhiNode>,
    pub items: Vec<ILBlockItem>,

    pub terminator: ILTerminator,
}

impl ILBlockData {
    /// Merge two blocks `(a, b)` into one.
    ///
    /// The following must hold:
    /// `a` must have `b` as its only successor.
    /// `b` msut have `a` as its only predecessor.
    /// i.e. the CFG must look like:
    ///```ignore
    ///  \|/
    ///   a
    ///   |
    ///   b
    ///  /|\
    /// ```
    /// The resulting CFG will now look like
    /// ```ignore
    /// \|/
    /// a>b
    /// /|\
    /// ```
    /// where `a>b` represents the merged blocks
    ///
    /// more specifically, `a>b` looks as follows:
    /// ```text
    /// a>b := {
    ///   label: a.label,
    ///   phis: a.phis ++ b.phis,
    ///   items: a.items ++ b.items,
    ///   terminator: b.terminator
    /// }```
    fn merge_linear_control_flow(&mut self, new: Self) -> Option<()> {
        if self.terminator != ILTerminator::Jmp(new.label) {
            return None;
        }

        self.phis.extend(new.phis);
        self.items.extend(new.items);
        self.terminator = new.terminator;
        Some(())
    }
    /// Move the first item in `other` to right after the last item of `self`
    /// (before `self`'s terminator)
    fn move_item_upwards(&mut self, other: &mut Self) -> Option<()> {
        let popped = other.items.try_remove(0)?;
        self.items.push(popped);
        Some(())
    }
}
#[derive(Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTerminator {
    Halt,
    Jmp(StringKey),
    BranchIf(ILOperand, StringKey, StringKey),
    Return,
    ReturnVal(ILOperand),
    #[default]
    FallThrough,
}

impl ILInstruction {
    fn imm_i64(n: u64) -> ILInstruction {
        ILInstruction::Immi64(n)
    }

    fn add(a: ILOperand, b: ILOperand) -> ILInstruction {
        ILInstruction::Binop(BinOp::Add, a, b)
    }

    fn cmp_z(as_typ: ILType, val: ILOperand) -> ILInstruction {
        ILInstruction::Cmp(CmpOp::Eq, as_typ, val, ILOperand::ConstInt(0))
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
impl ILBlockItem {}

// we should maybe intern calls, it could be useful for the fancy kind of optimization
// were you 'specialize' functions which are often called with the same parameters.
//
// using an KeyVec might prove useful to facilitate this?
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILCall {
    pub assignee: Option<(ILTemp, ILType)>,
    pub func: StringKey,
    pub args: Box<[(ILType, ILOperand)]>,
    pub is_variadic: bool,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct ILABITyp {
    pub(crate) base: ILType,
    pub(crate) sign: Option<bool>,
}

impl ILABITyp {
    pub fn signed(base: ILType) -> Self {
        Self {
            base,
            sign: Some(true),
        }
    }
    pub fn unsigned(base: ILType) -> Self {
        Self {
            base,
            sign: Some(false),
        }
    }
    pub fn nonsigned(base: ILType) -> Self {
        Self { base, sign: None }
    }
}

/// An instruction that assign to a temporary.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum ILInstruction {
    Load(ILABITyp, ILOperand),
    Immi64(u64),
    Immf64(u64),
    Copy(ILOperand),
    Cmp(CmpOp, ILType, ILOperand, ILOperand),
    Binop(BinOp, ILOperand, ILOperand),
    Unop(UnOp, ILOperand),
}
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum CmpOp {
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
    Ne,
    O,
    Sge,
    Sgt,
    Sle,
    Slt,
    Uge,
    Ugt,
    Ule,
    Ult,
    Uo,
}
impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmpOp::Slt => "slt",
            CmpOp::Ult => "ult",
            CmpOp::Eq => "eq",
            CmpOp::Ge => "ge",
            CmpOp::Gt => "gt",
            CmpOp::Le => "le",
            CmpOp::Lt => "lt",
            CmpOp::Ne => "ne",
            CmpOp::O => "o",
            CmpOp::Sge => "sge",
            CmpOp::Sgt => "sgt",
            CmpOp::Sle => "sle",
            CmpOp::Uge => "uge",
            CmpOp::Ugt => "ugt",
            CmpOp::Ule => "ule",
            CmpOp::Uo => "uo",
        }
        .fmt(f)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Rem,
    Div,
    And,
    Or,
    Sar,
    Shl,
    Shr,
    Udiv,
    Urem,
    Xor,
}
impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Rem => "rem",
            BinOp::Div => "div",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Sar => "sar",
            BinOp::Shl => "shl",
            BinOp::Shr => "shr",
            BinOp::Udiv => "udiv",
            BinOp::Urem => "urem",
            BinOp::Xor => "xor",
        }
        .fmt(f)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum UnOp {
    Neg,
    Alloc4,
    Alloc8,
    Alloc16,
    Ext(ILABITyp),
    /// extend an F32 into an F64
    ExtS,
    /// truncate precision of an F64 into an F32
    TruncD,
    /// Itof instructions
    ToFloat(ILABITyp),
    /// stoI instructions
    FromF64(ILABITyp),
    /// dtoI instructions
    FromF32(ILABITyp),
}

// ILValue is proably what we will place in the E-graphs, as they are the subject of
// constant folding etc.
/// A value that can be an operand of an [`ILInstr`]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILOperand {
    Temp(ILTemp),
    Global(StringKey),
    ConstInt(u64),
    /// an f32's bits stored verbatim to allow eq
    ConstFloat(u32),
    /// an f64's bits stored verbatim to allow eq
    ConstDouble(u64),
}
impl std::fmt::Display for ILOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match *self {
            ILOperand::Temp(iltemp) => iltemp.to_string(),
            ILOperand::Global(string_id) => string_id.unintern().into_string(),
            ILOperand::ConstInt(i) => i.to_string(),
            ILOperand::ConstFloat(f) => format!("s_{}", f32::from_bits(f)),
            ILOperand::ConstDouble(d) => format!("d_{}", f64::from_bits(d)),
        };
        write!(f, "{s}")
    }
}

impl ILOperand {}

/// A symbol that can be assigned to, which can exist in SSA or non-SSA form.
///
/// Non SSA temporaries are just strings really, but we intern them to save space.
///
/// SSA Temporaries are just integers.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILTemp {
    NonSSA(StringKey),
    SSA(StringKey, u32),
}
impl std::fmt::Display for ILTemp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ILTemp::NonSSA(string_key) => write!(f, "%{}", string_key.unintern()),
            ILTemp::SSA(string_key, num) => write!(f, "%{}.{num}", string_key.unintern()),
        }
    }
}
impl ILTemp {
    pub(crate) fn as_unique(
        self,
        in_func: ILFunction,
    ) -> Result<(ILFunction, StringKey, u32), StringKey> {
        match self {
            ILTemp::NonSSA(string_key) => Err(string_key),
            ILTemp::SSA(string_key, num) => Ok((in_func, string_key, num)),
        }
    }
}

impl ILTemp {
    pub const fn name(self) -> StringKey {
        match self {
            Self::NonSSA(n) => n,
            Self::SSA(n, _) => n,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILSignatureData {
    pub(crate) param_types: Box<[ILType]>,

    pub(crate) param_temporaries: Box<[ILTemp]>,
    pub(crate) returns: ILType,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILPhiNode {
    pub assignee: ILTemp,
    pub typ: ILType,
    pub incoming: Vec<(StringKey, ILOperand)>,
}
impl ILPhiNode {
    pub(crate) fn sort_incoming_by_label(&mut self) {
        self.incoming.sort_by_key(|a| a.0);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILDataDefinition {
    pub linkage: ILLinkage,
    pub layout: ILDataLayout,
    pub align: Option<usize>,
    pub name: StringKey,
    pub items: Box<[ILDataItem]>,
}
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILDataLayout {
    pub field_declarations: Box<[ILType]>,
    pub layout_kind: ILDataLayoutKind,
}
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILDataLayoutKind {
    /// All items are alligned to their [`ILType`]'s alignment
    Padded,
    /// Items are packed, with no padding or alignment
    Packed,
}
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILDataItem {
    Global(StringKey),
    ConstInt(u64),
    ConstFloat(u32),
    ConstDouble(u64),
    ConstString { interned: StringKey },
    Zeroed { num_bytes: usize },
}

/// Some Type in the IL
/// These types carry no sign information. [`ILAbiType`] does carry a sign.
///
/// This really only defines the layout of a value.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ILType {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Zero,
    Aggregate(StringKey),
    PhiFix,
}
impl std::fmt::Display for ILType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ILType::I8 => write!(f, "I8"),
            ILType::I16 => write!(f, "I16"),
            ILType::I32 => write!(f, "I32"),
            ILType::I64 => write!(f, "I64"),
            ILType::F32 => write!(f, "F32"),
            ILType::F64 => write!(f, "F64"),
            ILType::Zero => write!(f, "Zero"),
            ILType::Aggregate(string_key) => write!(f, "{}", string_key.unintern()),
            ILType::PhiFix => write!(f, "!!PhiFix!!"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ILAttributes {
    items: Box<[ILAttribute]>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ILAttribute {
    /// This block is called often
    Hot,
    /// This block is not called often and should be placed with other cold blocks
    Cold,
    /// If this block is also marked [`ConstructLoop(n)`], then we should unroll that
    /// loop construct
    HintUnroll,
    /// This set of blocks is part of some looping construct
    ConstructLoop(u8),
    /// This set of blocks is part of some looping construct
    ConstructBranch(u8),
    /// This set of blocks is part of some atomic operation
    ConstructAtomic(u8),

    //function attributes
    /// We should inline the marked function
    InlineHint,
    /// We should always inline the marked function
    InlineAlways,

    /// The marked function produces its output using only the provided arguments, without
    /// accessing global state. It might be a good candidate for inlining
    ComputePure,

    /// This function might write to memory
    MemWrites,

    /// This function might read from memory
    MemReads,

    /// This function should be emitted without a prologue or epilogue, probably because
    /// it was part of some ASM- or QBE-block in the frontend
    ABINaked,

    ABINoMangle,
}

/// A Function in the IR
///
/// Blocks are sorted in order of their label.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ILFunctionData {
    // pub(crate) attributes: ILAttributes,
    pub linkage: ILLinkage,
    pub(crate) signature: ILSignatureData,
    pub(crate) name: StringKey,
    /// Entry block is at index 0
    pub(crate) blocks: Vec<ILBlockData>,
    pub(crate) entry_idx: u32,
}

impl ILFunctionData {
    pub fn entry_block(&self) -> &ILBlockData {
        &self.blocks[self.entry_idx as usize]
    }
    pub fn entry_block_mut(&mut self) -> &mut ILBlockData {
        &mut self.blocks[self.entry_idx as usize]
    }
    fn find_block_idx(needle: StringKey, haystack: &[ILBlockData]) -> Option<usize> {
        haystack
            .binary_search_by(|probe| probe.label.cmp(&needle))
            .ok()
    }
    fn sort_blocks_by_label(blocks: &mut [ILBlockData]) {
        blocks.sort_by_key(|a| a.label);
    }

    pub(crate) fn find_block(&self, label: StringKey) -> &ILBlockData {
        let idx = Self::find_block_idx(label, &self.blocks).unwrap();
        &self.blocks[idx]
    }
    pub(crate) fn find_block_mut(&mut self, label: StringKey) -> &mut ILBlockData {
        let idx = Self::find_block_idx(label, &self.blocks).unwrap();
        &mut self.blocks[idx]
    }
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
#[macro_use]
pub mod builder;
