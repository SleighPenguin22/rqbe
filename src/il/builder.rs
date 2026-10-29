use crate::{StringID, il::*};
use bitvec::vec::BitVec;
use idset::KeySet;
use paste::paste;

pub struct ILModuleBuilder {
    ctx: ILModuleContext,
}
impl<'thisbuilder> ILModuleBuilder {
    pub fn start() -> Self {
        Self {
            ctx: ILModuleContext::with_capacity(16),
        }
    }
    pub fn typs_mut(&mut self) -> &mut KeySet<ILType, ILTypeData> {
        &mut self.ctx.typs
    }
    pub fn typs(&self) -> &KeySet<ILType, ILTypeData> {
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
    pub fn add_layout(
        &'thisbuilder mut self,
        kind: ILDataLayoutKind,
    ) -> ILLayoutBuilder<'thisbuilder> {
        ILLayoutBuilder::new(self, kind)
    }
    fn intern_layout_data(&mut self, data: ILLayoutData) -> StringID {
        self.ctx.layouts.get_or_intern(data)
    }

    pub fn add_function(&'thisbuilder mut self, name: &str) -> ILFunctionBuilder<'thisbuilder> {
        let string_id = crate::intern::intern_str(name);
        let skeleton = ILFunctionData {
            signature: ILSignatureData {
                param_types: vec![],
                param_temporaries: vec![],
                returns: self.get_type_I32(),
            },
            name: string_id,
            blocks: vec![],
            linkage: ILLinkage::Export,
        };
        let f_id = self.ctx.functions.push(skeleton);
        ILFunctionBuilder::new(self, string_id, f_id)
    }

    pub fn finish(self) -> ILModule {
        ILModule { ctx: self.ctx }
    }
}

pub struct ILGlobalDataBuilder<'module> {
    module: &'module mut ILModuleBuilder,
    layout: Option<StringID>,
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
    pub fn with_existing_layout(mut self, layout: StringID) -> Self {
        self.layout = Some(layout);
        self
    }
    pub fn with_bits(mut self, bits: &[u8]) -> Self {
        self.bits = Some(bits.to_vec());
        self
    }
    pub fn with_name(mut self, name: &str) -> Self {
        let sid = crate::intern::intern_str(name);
        self.name = Some(sid);
        self
    }
    pub fn finish_global(self) -> ILGlobal {
        let data = ILGlobalData {
            layout: self.layout.unwrap(),
            name: self.name.unwrap(),
            bits: self.bits.unwrap(),
            linkage: ILLinkage::Export,
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
    pub fn finish_layout(self) -> StringID {
        let layout = ILLayoutData {
            fields: self.fields,
            layout_kind: self.kind,
        };

        self.for_module.intern_layout_data(layout)
    }
}

pub struct ILFunctionBuilder<'module> {
    pub for_module: &'module mut ILModuleBuilder,
    id: ILFunction,
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
        if self.func_id == func_builder.id && !func_builder.finished_blocks[self.block_idx as usize]
        {
            Some(ILBlock {
                block_idx: self.block_idx,
                func_id: func_builder.id,
            })
        } else {
            None
        }
    }
}

impl<'module> ILFunctionBuilder<'module> {
    pub fn new(module: &'module mut ILModuleBuilder, name: StringID, id: ILFunction) -> Self {
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
        self.entry_block_idx = Some(block.block_idx);
    }

    pub fn switch_to_block(&mut self, block: StringID) {
        let idx = self
            .blocks
            .iter()
            .enumerate()
            .find_map(|(i, b)| (b.label == block).then_some(i))
            .expect("block does not exist in function");
        self.active_block = Some(idx as u32);
    }

    pub fn new_fresh_block(&mut self) -> StringID {
        let (block, _idx) = self.prepare_new_empty_block();
        self.switch_to_block(block);
        self.finished_blocks.push(false);
        block
    }

    pub fn add_assignment<'a>(
        &'a mut self,
        assignee: &str,
        typ: ILTypeData,
    ) -> ILInstructionBuilder<'a, 'module, MissingValue> {
        let id = crate::intern::intern_str(assignee);
        let typ = self.for_module.typs_mut().get_or_intern(typ);
        ILInstructionBuilder::new(self, id, typ)
    }
    /// Insert a function call, discarding its return value
    ///
    /// If you want to save the returned value, use [`add_instruction().call()`]
    pub fn add_call<'a>(&'a mut self, func: &str, args: &[(ILType, ILOperand)]) {
        let func = crate::intern::intern_str(func);
        self.push_nonassigned_call_into_block(ILCall {
            assignsee: None,
            func,
            args: args.into(),
        });
    }
    pub fn terminate_jmp(&mut self, block_id: StringID) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::Jmp(block_id);
        });
    }
    pub fn terminate_return_value(&mut self, val: ILTemp) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::ReturnVal(ILOperand::Temp(val));
        });
    }
    pub fn terminate_branch(&mut self, condition: ILTemp, taken: StringID, not_taken: StringID) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::BranchIf(ILOperand::Temp(condition), taken, not_taken);
        });
    }

    pub fn finish_active_block(&mut self) -> ILBlock {
        let active_idx = self.get_active_block_index();
        self.finished_blocks.set(active_idx as usize, true);
        self.active_block = None;
        ILBlock {
            block_idx: active_idx,
            func_id: self.id,
        }
    }
    pub fn finish_function(mut self) -> Option<ILFunction> {
        if self.finished_blocks.not_all() {
            return None;
        }
        let has_entry = self.ensure_entry_block_at_idx0();
        if has_entry && let Some(signature) = self.signature {
            let f_data = ILFunctionData {
                signature,
                name: self.name,
                blocks: self.blocks,
                linkage: ILLinkage::Export,
            };
            let f_data_refm = self.for_module.ctx.functions.get_mut(self.id).unwrap();
            *f_data_refm = f_data;
            Some(self.id)
        } else {
            None
        }
    }
}
impl<'module> ILFunctionBuilder<'module> {
    fn generate_block_label(&self, label_num: u32) -> String {
        let fname = crate::intern::unintern_str(self.name).unwrap();
        format!("{}:{}.{}", fname, self.id.into_usize(), label_num)
    }

    fn prepare_new_empty_block(&mut self) -> (StringID, u32) {
        let label = self.generate_block_label(self.label_gen);
        let label = crate::intern::intern_str(&label);
        self.label_gen += 1;
        let empty_block = ILBlockData {
            label,
            items: vec![],
            terminator: super::ILTerminator::Unspecified,
            phis: vec![],
        };
        self.blocks.push(empty_block);
        (label, self.label_gen - 1)
    }
    /// get a reference to the active block, and its index in the `.blocks` vector
    fn get_active_block_index(&mut self) -> u32 {
        match self.active_block {
            Some(idx) => idx,
            None => {
                let (empty_block, idx) = self.prepare_new_empty_block();
                self.switch_to_block(empty_block);
                idx
            }
        }
    }

    fn push_instruction_data_into_block(
        &mut self,
        value: ILInstruction,
        typ: ILType,
        assignee: StringID,
    ) -> ILTemp {
        let assignee = ILTemp::NonSSA(assignee);
        self.modify_active_block_with(|block| {
            let item = ILBlockItem::AssignInstr(assignee, typ, value);
            block.items.push(item);
        });
        assignee
    }
    fn push_assigned_call_into_block(&mut self, call: ILCall) -> ILTemp {
        let assignee = call.assignsee.unwrap().0;
        self.modify_active_block_with(|block| {
            let item = ILBlockItem::Call(call);
            block.items.push(item);
        });
        assignee
    }
    fn push_nonassigned_call_into_block(&mut self, call: ILCall) {
        self.modify_active_block_with(|block| {
            let item = ILBlockItem::Call(call);
            block.items.push(item);
        });
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
    param_temps: Vec<ILTemp>,
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
        let param_idx = self.param_types.len();
        let id = format!("p{param_idx}");
        let id = crate::intern::intern_str(&id);
        let temp = ILTemp::SSA(self.for_function.for_module.ctx.next_temp(), id);
        self.param_types.push(typ);
        self.param_temps.push(temp);
        self
    }
    fn returns(mut self, typ: ILType) -> Self {
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
pub struct HasValue(ILInstruction);
pub struct MissingValue;
impl InstructionBuilderProgress for HasValue {}
impl InstructionBuilderProgress for MissingValue {}

pub struct ILInstructionBuilder<'a, 'thisbuilder, S: InstructionBuilderProgress> {
    for_function: &'a mut ILFunctionBuilder<'thisbuilder>,
    assignee: StringID,
    typ: ILType,
    progress: S,
}

impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, MissingValue> {
    pub fn new(
        for_function: &'a mut ILFunctionBuilder<'thisbuilder>,
        assignee: StringID,
        typ: ILType,
    ) -> Self {
        Self {
            for_function,
            assignee,
            typ,
            progress: MissingValue,
        }
    }

    fn into_has_instruction(
        self,
        value: ILInstruction,
    ) -> ILInstructionBuilder<'a, 'thisbuilder, HasValue> {
        ILInstructionBuilder {
            for_function: self.for_function,
            assignee: self.assignee,
            typ: self.typ,
            progress: HasValue(value),
        }
    }
}
impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, HasValue> {
    /// push this instruction into the block, and return its reference
    pub fn epilogue(self) -> ILTemp {
        let value = self.progress.0;
        self.for_function
            .push_instruction_data_into_block(value, self.typ, self.assignee)
    }
}

impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, MissingValue> {
    pub fn imm_u64(self, n: u64) -> ILTemp {
        let data = ILInstruction::imm_i64(n);
        self.into_has_instruction(data).epilogue()
    }
    pub fn add(self, a: ILTemp, b: ILTemp) -> ILTemp {
        let data = ILInstruction::add(ILOperand::Temp(a), ILOperand::Temp(b));
        self.into_has_instruction(data).epilogue()
    }
    pub fn cmp_is_zero(self, val: ILTemp) -> ILTemp {
        let data = ILInstruction::cmp_z(ILOperand::Temp(val));
        self.into_has_instruction(data).epilogue()
    }

    pub fn call(self, func: &str, args: &[(ILType, ILOperand)]) -> ILTemp {
        let assigns_to = Some((ILTemp::NonSSA(self.assignee), self.typ));
        let func = crate::intern::intern_str(func);
        self.for_function.push_assigned_call_into_block(ILCall {
            assignsee: assigns_to,
            func,
            args: args.into(),
        })
    }
}
