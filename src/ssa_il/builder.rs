use crate::{
    ssa_il::{
        ILAssignee, ILBlock, ILBlockData, ILDataLayoutKind, ILFunction, ILFunctionData, ILGlobal,
        ILGlobalData, ILGlobalSymbol, ILGlobalSymbolData, ILLayout, ILLayoutData, ILModule,
        ILModuleContext, ILSignatureData, ILTemporary, ILTerminator, ILType, ILTypeData, ILValue,
        ILValueData, StringID,
    },
    util::InternTable,
};
use bitvec::vec::BitVec;
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

    fn intern_symbol_data(&mut self, symb: ILGlobalSymbolData) -> ILGlobalSymbol {
        self.ctx.symbols.get_or_intern(symb)
    }
    fn intern_value_data(&mut self, instruction_data: ILValueData) -> ILValue {
        self.ctx.values.get_or_intern(instruction_data)
    }
    pub fn add_function(&'thisbuilder mut self, name: &str) -> ILFunctionBuilder<'thisbuilder> {
        let string_id = self.intern_string(name.to_string());
        self.ctx.func_gen += 1;
        ILFunctionBuilder::new(self, string_id, self.ctx.func_gen)
    }

    pub fn finish(self) -> ILModule {
        ILModule { ctx: self.ctx }
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
    id: u32,
    signature: Option<ILSignatureData>,
    name: StringID,
    blocks: Vec<ILBlockData>,
    entry_block_idx: Option<u32>,
    /// block label counter
    label_gen: u32,
    /// the block that instructions and terminators are inserted into
    active_block: Option<u32>,
    /// inidices in `.blocks` that are not finished
    finished_blocks: BitVec,
}

impl ILBlock {
    pub fn get_finished<'a>(&self, func_builder: &ILFunctionBuilder<'a>) -> Option<ILBlock> {
        if self.func_id == func_builder.id && !func_builder.finished_blocks[self.block_id as usize]
        {
            Some(ILBlock {
                block_id: self.block_id,
                func_id: func_builder.id,
            })
        } else {
            None
        }
    }
}

impl<'module> ILFunctionBuilder<'module> {
    pub fn new(module: &'module mut ILModuleBuilder, name: StringID, id: u32) -> Self {
        Self {
            for_module: module,
            id,
            name,
            blocks: vec![],
            entry_block_idx: None,
            signature: None,
            label_gen: 0,
            active_block: None,
            finished_blocks: BitVec::with_capacity(4),
        }
    }
    pub fn build_signature(self, return_type: ILType) -> ILFunctionSignatureBuilder<'module> {
        ILFunctionSignatureBuilder::new(self).returns(return_type)
    }

    pub fn set_entry_block(&mut self, block: ILBlock) {
        self.entry_block_idx = Some(block.block_id);
    }
    pub fn switch_to_block(&mut self, block: ILBlock) {
        self.active_block = Some(block.block_id);
    }

    pub fn new_fresh_block(&mut self) -> ILBlock {
        let block = self.prepare_new_empty_block();
        self.switch_to_block(block);
        let idx = self.get_active_block_index();
        self.finished_blocks.push(false);
        ILBlock {
            block_id: idx,
            func_id: self.id,
        }
    }

    pub fn add_instruction<'a>(&'a mut self) -> ILInstructionBuilder<'a, 'module, MissingValue> {
        ILInstructionBuilder::new(self)
    }
    pub fn terminate_jmp(&mut self, block_id: ILBlock) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::Jmp(block_id);
        });
    }
    pub fn terminate_return_value(&mut self, val: ILTemporary) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::ReturnVal(val);
        });
    }
    pub fn terminate_branch(&mut self, condition: ILTemporary, taken: ILBlock, not_taken: ILBlock) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::BranchIf(condition, taken, not_taken);
        });
    }

    pub fn finish_active_block(&mut self) -> ILBlock {
        let active_idx = self.get_active_block_index();
        self.finished_blocks.set(active_idx as usize, true);
        self.active_block = None;
        ILBlock {
            block_id: active_idx,
            func_id: self.id,
        }
    }
    pub fn finish_function(mut self) -> Option<ILFunction> {
        if self.finished_blocks.not_all() {
            return None;
        }
        let has_entry = self.ensure_entry_block_at_idx0();
        if has_entry && let Some(signature) = self.signature {
            let f = ILFunctionData {
                signature,
                name: self.name,
                blocks: self.blocks,
            };
            Some(self.for_module.ctx.functions.get_or_intern(f))
        } else {
            None
        }
    }
}
impl<'module> ILFunctionBuilder<'module> {
    fn prepare_new_empty_block(&mut self) -> ILBlock {
        let name_str = self.for_module.ctx.strings.get_by_id(self.name).unwrap();
        let label = format!("{name_str}_{}", self.label_gen);
        self.label_gen += 1;
        let empty_block = ILBlockData {
            for_function: self.id,
            label,
            items: vec![],
            terminator: super::ILTerminator::Unspecified,
        };
        self.blocks.push(empty_block);
        ILBlock {
            block_id: self.blocks.len() as u32 - 1,
            func_id: self.id,
        }
    }
    /// get a reference to the active block, and its index in the `.blocks` vector
    fn get_active_block_index(&mut self) -> u32 {
        match self.active_block {
            Some(idx) => idx,
            None => {
                let empty_block = self.prepare_new_empty_block();
                self.switch_to_block(empty_block);
                empty_block.block_id
            }
        }
    }

    fn push_instruction_data_into_block(&mut self, value: ILValueData) -> ILTemporary {
        let value_id = self.for_module.intern_value_data(value);
        let temp = self.for_module.ctx.next_temp();
        self.modify_active_block_with(|block| {
            block.items.push((ILAssignee::SSA(temp), value_id));
        });
        temp
    }

    fn modify_active_block_with<F: FnOnce(&mut ILBlockData)>(&mut self, f: F) {
        let idx = self.get_active_block_index();
        let active_block_mut = self.blocks.get_mut(idx as usize).unwrap();
        f(active_block_mut);
    }

    /// If an entry block is specified,
    /// return `true` and move it to index 0, otherwise return `false`.
    fn ensure_entry_block_at_idx0(&mut self) -> bool {
        if let Some(entry_idx) = self.entry_block_idx {
            if entry_idx != 0 {
                self.blocks.swap(0, entry_idx as usize);
            }
            true
        } else {
            false
        }
    }
}

pub struct ILFunctionSignatureBuilder<'module> {
    for_function: ILFunctionBuilder<'module>,
    returns: Option<ILType>,
    param_types: Vec<ILType>,
    param_temps: Vec<ILTemporary>,
}
impl<'module> ILFunctionSignatureBuilder<'module> {
    pub fn new(for_function: ILFunctionBuilder<'module>) -> Self {
        Self {
            for_function,
            returns: None,
            param_types: vec![],
            param_temps: vec![],
        }
    }
    pub fn add_param(mut self, typ: ILType) -> Self {
        let temp = self.for_function.for_module.ctx.next_temp();
        self.param_types.push(typ);
        self.param_temps.push(temp);
        self
    }
    pub fn returns(mut self, typ: ILType) -> Self {
        self.returns = Some(typ);
        self
    }
    pub fn finish_signature(mut self) -> ILFunctionBuilder<'module> {
        let sig = ILSignatureData {
            param_types: self.param_types,
            param_temporaries: self.param_temps,
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
    pub fn epilogue(self) -> ILTemporary {
        let value = self.progress.0;
        self.for_function.push_instruction_data_into_block(value)
    }
}

impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, MissingValue> {
    pub fn imm_u64(self, n: u64) -> ILTemporary {
        let data = ILValueData::Immi64(n);
        self.into_has_instruction(data).epilogue()
    }
    pub fn add(self, a: ILTemporary, b: ILTemporary) -> ILTemporary {
        let data = ILValueData::add(a, b);
        self.into_has_instruction(data).epilogue()
    }
    pub fn cmp_is_zero(self, val: ILTemporary) -> ILTemporary {
        let data = ILValueData::CmpZ(val);
        self.into_has_instruction(data).epilogue()
    }
}
