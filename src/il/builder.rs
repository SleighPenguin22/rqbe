use crate::{
    il::{
        ILBlock, ILBlockData, ILDataLayoutKind, ILFunction, ILFunctionData, ILGlobal, ILGlobalData,
        ILLayout, ILLayoutData, ILModule, ILModuleContext, ILSignatureData, ILSymbol, ILSymbolData,
        ILTerminator, ILType, ILTypeData, ILValue, ILValueData, StringID,
    },
    util::InternTable,
};
use paste::paste;
impl ILLayout {
    pub fn as_aggregate_type(self, ctx: &mut ILModuleContext) -> ILType {
        ctx.typs_mut().get_or_intern(ILTypeData::Aggregate(self))
    }
}

pub struct ILModuleBuilder {
    ctx: ILModuleContext,
}
impl<'thisbuilder> ILModuleBuilder {
    pub fn start() -> Self {
        Self {
            ctx: ILModuleContext::with_capacity(16),
        }
    }
    pub fn typs_mut(&mut self) -> &mut InternTable<ILType, ILTypeData> {
        &mut self.ctx.typs
    }
    pub fn typs(&self) -> &InternTable<ILType, ILTypeData> {
        &self.ctx.typs
    }
    get_type_!(I8);
    get_type_!(I16);
    get_type_!(I32);
    get_type_!(I64);
    get_type_!(F32);
    get_type_!(F64);
    get_type_!(Zero);
    pub fn add_global_data(&'thisbuilder mut self) -> ILGlobalDataBuilder<'thisbuilder> {
        ILGlobalDataBuilder::new(self)
    }
    fn intern_global_data(&mut self, data: ILGlobalData) -> ILGlobal {
        self.ctx.globals.get_or_intern(data)
    }
    pub fn intern_string(&mut self, s: String) -> StringID {
        self.ctx.strings.get_or_intern(s)
    }
    pub fn add_layout(
        &'thisbuilder mut self,
        kind: ILDataLayoutKind,
    ) -> ILLayoutBuilder<'thisbuilder> {
        ILLayoutBuilder::new(self, kind)
    }
    fn intern_layout_data(&mut self, data: ILLayoutData) -> ILLayout {
        self.ctx.layouts.get_or_intern(data)
    }

    fn intern_symbol_data(&mut self, symb: ILSymbolData) -> ILSymbol {
        self.ctx.symbols.get_or_intern(symb)
    }
    fn intern_value_data(&mut self, instruction_data: ILValueData) -> ILValue {
        self.ctx.values.get_or_intern(instruction_data)
    }
    pub fn add_function(&'thisbuilder mut self, name: &str) -> ILFunctionBuilder<'thisbuilder> {
        let string_id = self.intern_string(name.to_string());
        ILFunctionBuilder::new(self, string_id)
    }

    pub fn finish(self) -> ILModule {
        ILModule { ctx: self.ctx }
    }

    fn intern_block_data(&mut self, block_data: ILBlockData) -> ILBlock {
        self.ctx.blocks.get_or_intern(block_data)
    }
    #[allow(unused)]
    fn get_block(&mut self, id: ILBlock) -> &ILBlockData {
        self.ctx.blocks.get_by_id(id).unwrap()
    }
    #[allow(unused)]
    fn get_block_mut(&mut self, id: ILBlock) -> &mut ILBlockData {
        self.ctx.blocks.get_by_id_mut(id).unwrap()
    }
}

pub struct ILGlobalDataBuilder<'module> {
    module: &'module mut ILModuleBuilder,
    layout: Option<ILLayout>,
    bits: Option<Vec<u8>>,
    name: Option<StringID>,
}

impl<'module> ILGlobalDataBuilder<'module> {
    fn new(module: &'module mut ILModuleBuilder) -> Self {
        Self {
            module,
            layout: None,
            bits: None,
            name: None,
        }
    }
    /// Build the layout for this global.
    pub fn build_layout(self, kind: ILDataLayoutKind) -> ILGlobalDataLayoutBuilder<'module> {
        ILGlobalDataLayoutBuilder::new(self, kind)
    }
    /// Build this global using some layout built earlier
    pub fn with_existing_layout(mut self, layout: ILLayout) -> Self {
        self.layout = Some(layout);
        self
    }
    pub fn with_bits(mut self, bits: &[u8]) -> Self {
        self.bits = Some(bits.to_vec());
        self
    }
    pub fn with_name(mut self, name: &str) -> Self {
        let sid = self.module.intern_string(name.to_string());
        self.name = Some(sid);
        self
    }
    pub fn finish_global(self) -> ILGlobal {
        let data = ILGlobalData {
            layout: self.layout.unwrap(),
            name: self.name.unwrap(),
            bits: self.bits.unwrap(),
        };
        self.module.intern_global_data(data)
    }
}
pub struct ILGlobalDataLayoutBuilder<'module> {
    for_global: ILGlobalDataBuilder<'module>,
    fields: Vec<ILType>,
    kind: ILDataLayoutKind,
}

impl<'module> ILGlobalDataLayoutBuilder<'module> {
    pub fn add_field(mut self, field: ILType) -> Self {
        self.fields.push(field);
        self
    }
    pub fn finish_layout(mut self) -> ILGlobalDataBuilder<'module> {
        let layout = ILLayoutData {
            fields: self.fields,
            layout_kind: self.kind,
        };
        let layout = self.for_global.module.ctx.layouts.get_or_intern(layout);
        self.for_global.layout = Some(layout);
        self.for_global
    }

    fn new(arg: ILGlobalDataBuilder<'module>, kind: ILDataLayoutKind) -> Self {
        Self {
            for_global: arg,
            fields: vec![],
            kind,
        }
    }
}
pub struct ILLayoutBuilder<'module> {
    for_module: &'module mut ILModuleBuilder,
    fields: Vec<ILType>,
    kind: ILDataLayoutKind,
}

impl<'module> ILLayoutBuilder<'module> {
    pub fn new(for_module: &'module mut ILModuleBuilder, kind: ILDataLayoutKind) -> Self {
        Self {
            for_module,
            fields: vec![],
            kind,
        }
    }

    pub fn add_field(mut self, field: ILType) -> Self {
        self.fields.push(field);
        self
    }
    pub fn finish_layout(self) -> ILLayout {
        let layout = ILLayoutData {
            fields: self.fields,
            layout_kind: self.kind,
        };

        self.for_module.intern_layout_data(layout)
    }
}

pub struct ILFunctionBuilder<'module> {
    pub for_module: &'module mut ILModuleBuilder,
    signature: Option<ILSignatureData>,
    name: StringID,
    blocks: Vec<ILBlock>,
    entry_block: Option<ILBlock>,
    label_gen: u32,
    active_block: Option<ILBlock>,
}

impl<'module> ILFunctionBuilder<'module> {
    pub fn new(module: &'module mut ILModuleBuilder, name: StringID) -> Self {
        Self {
            for_module: module,
            name,
            blocks: vec![],
            entry_block: None,
            signature: None,
            label_gen: 0,
            active_block: None,
        }
    }
    pub fn build_signature(self, return_type: ILType) -> ILFunctionSignatureBuilder<'module> {
        ILFunctionSignatureBuilder::new(self).returns(return_type)
    }
    fn generate(&mut self, label: StringID) -> ILSymbol {
        let symb = ILSymbolData::Temporary(self.label_gen, label);
        self.label_gen += 1;
        self.for_module.intern_symbol_data(symb)
    }
    pub fn unnamed_temp(&mut self) -> ILSymbol {
        let empty_string = self.for_module.intern_string(String::from("__unnamed"));
        self.generate(empty_string)
    }
    pub fn named_temp(&mut self, label: impl Into<String>) -> ILSymbol {
        let label = self.for_module.intern_string(label.into());
        self.generate(label)
    }
    fn prepare_new_empty_block(&mut self) -> ILBlock {
        let name_str = self.for_module.ctx.strings.get_by_id(self.name).unwrap();
        let label = format!("{name_str}_{}", self.label_gen);
        self.label_gen += 1;
        let empty_block = ILBlockData {
            label,
            items: vec![],
            terminator: super::ILTerminator::BuilderNotFinished,
        };
        self.for_module.intern_block_data(empty_block)
    }
    pub fn set_entry_block(&mut self, block: ILBlock) {
        self.entry_block = Some(block);
    }
    pub fn switch_to_block(&mut self, block: ILBlock) {
        self.active_block = Some(block);
        self.push_dedup_block(block);
    }

    fn push_dedup_block(&mut self, block: ILBlock) {
        if !self.blocks.contains(&block) {
            self.blocks.push(block);
        }
    }

    pub fn switch_to_fresh_block(&mut self) {
        let block = self.prepare_new_empty_block();
        self.switch_to_block(block);
    }

    /// get a reference to the active block, and its index in the `.blocks` vector
    fn get_active_block_with_index(&mut self) -> (usize, ILBlock) {
        let active_block_id = self.active_block.unwrap_or_else(|| {
            let empty_block = self.prepare_new_empty_block();
            self.switch_to_block(empty_block);
            self.set_entry_block(empty_block);
            empty_block
        });
        let active_block_idx = if self.blocks.is_empty() {
            self.push_dedup_block(active_block_id);
            0
        } else {
            self.blocks
                .iter()
                .enumerate()
                .find_map(|(idx, id)| (*id == active_block_id).then_some(idx))
                .unwrap()
        };

        (active_block_idx, active_block_id)
    }

    pub fn add_instruction<'a>(&'a mut self) -> ILInstructionBuilder<'a, 'module, MissingValue> {
        // A clone is necessary here, as modifying the block in-place
        // would modify the empty block within the table, thus making it appear
        // modified for all functions holding a reference to that block.
        // say we have two functions that have some block in common, if one of them were to insert
        // an instruction into that block, the other function would have its block changed too.
        ILInstructionBuilder::new(self)
    }
    fn push_instruction_data_into_block(&mut self, value: ILValueData) -> ILValue {
        let value_id = self.for_module.intern_value_data(value);
        self.modify_active_block_with(|mut block| {
            block.items.push(value_id);
            block
        });
        value_id
    }
    fn commit_updated_active_block(&mut self, idx: usize, id: ILBlock) {
        self.blocks[idx] = id;
        self.active_block = Some(id);
    }
    pub fn finish_active_block(&mut self, terminator: ILTerminator) -> ILBlock {
        self.modify_active_block_with(|mut block| {
            block.terminator = terminator;
            block
        });
        self.active_block.unwrap()
    }

    fn modify_active_block_with<F: FnOnce(ILBlockData) -> ILBlockData>(&mut self, f: F) {
        let (active_idx, active_id) = self.get_active_block_with_index();
        let active_id = self
            .for_module
            .ctx
            .blocks
            .clone_modify_reintern(active_id, f)
            .unwrap();
        self.commit_updated_active_block(active_idx, active_id);
    }

    pub fn finish_function(mut self) -> ILFunction {
        self.ensure_entry_block_at_idx0();
        let f = ILFunctionData {
            signature: self.signature.unwrap(),
            name: self.name,
            blocks: self.blocks,
        };
        self.for_module.ctx.functions.get_or_intern(f)
    }
    fn ensure_entry_block_at_idx0(&mut self) {
        let entry_block = self.entry_block.unwrap();
        self.switch_to_block(entry_block);
        let (entry_idx, _) = self.get_active_block_with_index();
        if entry_idx != 0 {
            self.blocks.swap(0, entry_idx);
        }
    }
}

pub struct ILFunctionSignatureBuilder<'module> {
    for_function: ILFunctionBuilder<'module>,
    returns: Option<ILType>,
    param_types: Vec<ILType>,
    param_symbols: Vec<ILSymbol>,
}
impl<'module> ILFunctionSignatureBuilder<'module> {
    pub fn new(for_function: ILFunctionBuilder<'module>) -> Self {
        Self {
            for_function,
            returns: None,
            param_types: vec![],
            param_symbols: vec![],
        }
    }
    pub fn add_param(mut self, typ: ILType) -> Self {
        let temp = self.for_function.unnamed_temp();
        self.param_types.push(typ);
        self.param_symbols.push(temp);
        self
    }
    pub fn returns(mut self, typ: ILType) -> Self {
        self.returns = Some(typ);
        self
    }
    pub fn finish_signature(mut self) -> ILFunctionBuilder<'module> {
        let sig = ILSignatureData {
            param_types: self.param_types,
            param_symbols: self.param_symbols,
            returns: self.returns.unwrap(),
        };
        self.for_function.signature = Some(sig);
        self.for_function
    }
}

pub trait InstructionBuilderProgress {}
pub struct HasValue(ILValueData);
pub struct MissingValue;
impl InstructionBuilderProgress for HasValue {}
impl InstructionBuilderProgress for MissingValue {}

pub struct ILInstructionBuilder<'a, 'thisbuilder, S: InstructionBuilderProgress> {
    for_function: &'a mut ILFunctionBuilder<'thisbuilder>,
    progress: S,
}

impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, MissingValue> {
    pub fn new(for_function: &'a mut ILFunctionBuilder<'thisbuilder>) -> Self {
        Self {
            for_function,
            progress: MissingValue,
        }
    }

    fn into_has_instruction(
        self,
        value: ILValueData,
    ) -> ILInstructionBuilder<'a, 'thisbuilder, HasValue> {
        ILInstructionBuilder {
            for_function: self.for_function,
            progress: HasValue(value),
        }
    }
}
impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, HasValue> {
    /// push this instruction into the block, and return its reference
    pub fn epilogue(self) -> ILValue {
        let value = self.progress.0;
        self.for_function.push_instruction_data_into_block(value)
    }
}

impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, MissingValue> {
    pub fn imm_u64(self, n: u64) -> ILValue {
        let data = ILValueData::ImmInt(n);
        self.into_has_instruction(data).epilogue()
    }
    pub fn add(self, a: ILValue, b: ILValue) -> ILValue {
        let data = ILValueData::Add(a, b);
        self.into_has_instruction(data).epilogue()
    }
}
