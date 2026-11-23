use crate::{StringKey, il::*};
use keyvec::KeySet;

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
        &mut self.ctx.types
    }
    pub fn typs(&self) -> &KeySet<ILType, ILTypeData> {
        &self.ctx.types
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I8(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::I8)
            .expect("missing integer type")
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I16(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::I16)
            .expect("missing integer type")
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I32(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::I32)
            .expect("missing integer type")
    }
    #[allow(nonstandard_style)]
    pub fn get_type_I64(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::I64)
            .expect("missing integer type")
    }
    #[allow(nonstandard_style)]
    pub fn get_type_F32(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::F32)
            .expect("missing integer type")
    }
    #[allow(nonstandard_style)]
    pub fn get_type_F64(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::F64)
            .expect("missing integer type")
    }
    #[allow(nonstandard_style)]
    pub fn get_type_Zero(&self) -> ILType {
        self.ctx
            .types
            .get_key_of(&ILTypeData::Zero)
            .expect("missing integer type")
    }

    pub fn add_global_data(&'thisbuilder mut self) -> ILGlobalDataBuilder<'thisbuilder> {
        ILGlobalDataBuilder::new(self)
    }
    fn intern_global_data(&mut self, data: ILGlobalData) -> ILGlobal {
        self.ctx.globals.get_key(data)
    }
    pub fn add_layout(&'thisbuilder mut self) -> ILLayoutBuilder {
        ILLayoutBuilder::new()
    }

    pub fn add_function(&'thisbuilder mut self, name: &str) -> ILFunctionBuilder<'thisbuilder> {
        let string_id = crate::intern::intern_str(name);
        let skeleton = ILFunctionData {
            signature: ILSignatureData {
                param_types: Box::new([]),
                param_temporaries: Box::new([]),
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
    layout: Option<ILDataLayout>,
    bits: Option<Vec<u8>>,
    name: Option<StringKey>,
    align: Option<usize>,
}

impl<'module> ILGlobalDataBuilder<'module> {
    fn new(module: &'module mut ILModuleBuilder) -> Self {
        Self {
            module,
            layout: None,
            bits: None,
            name: None,
            align: None,
        }
    }
    /// Build the layout for this global.
    pub fn build_layout(self, kind: ILDataLayoutKind) -> ILGlobalDataLayoutBuilder<'module> {
        ILGlobalDataLayoutBuilder::new(self, kind)
    }
    pub fn with_bits(mut self, bits: &[u8]) -> Self {
        self.bits = Some(bits.to_vec());
        self
    }
    pub fn spcify_align(mut self, align: usize) -> Self {
        self.align = Some(align);
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
            align: self.align,
            items: self
                .bits
                .unwrap_or_default()
                .into_iter()
                .map(|n| ILDataItem::ConstInt(n as u64))
                .collect(),
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
        let layout = ILDataLayout {
            fields: self.fields,
            layout_kind: self.kind,
        };
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
pub struct ILLayoutBuilder {
    fields: Vec<ILType>,
    kind: ILDataLayoutKind,
}

impl ILLayoutBuilder {
    pub fn new() -> Self {
        Self {
            fields: vec![],
            kind: ILDataLayoutKind::Packed,
        }
    }

    pub fn add_field(mut self, field: ILType) -> Self {
        self.fields.push(field);
        self
    }
    pub fn finish_layout(self) -> ILDataLayout {
        let layout = ILDataLayout {
            fields: self.fields,
            layout_kind: self.kind,
        };

        layout
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FunctionBuilderError {
    MissingSignature,
    EmptyFunction,
    LastBlockFallsThrough,
}

pub struct ILFunctionBuilder<'module> {
    pub for_module: &'module mut ILModuleBuilder,
    defined_id: ILFunction,
    signature: Option<ILSignatureData>,
    name: StringKey,
    blocks: Vec<ILBlockData>,
    /// The index into `self.blocks` where the entry block is located,
    /// This defaults to 0
    entry_block_idx: u32,
    /// block label counter
    label_gen: u32,
    /// the block that instructions and terminators are inserted into
    active_block: Option<u32>,
}

impl ILBlock {
    pub fn get_finished<'a>(&self, func_builder: &ILFunctionBuilder<'a>) -> Option<ILBlock> {
        if self.func_id == func_builder.defined_id {
            Some(ILBlock {
                block_idx: self.block_idx,
                func_id: func_builder.defined_id,
            })
        } else {
            None
        }
    }
}

impl<'module> ILFunctionBuilder<'module> {
    pub fn new(
        module: &'module mut ILModuleBuilder,
        name: StringKey,
        defined_id: ILFunction,
    ) -> Self {
        Self {
            for_module: module,
            defined_id,
            name,
            blocks: vec![],
            entry_block_idx: 0,
            signature: None,
            label_gen: 0,
            active_block: None,
        }
    }
    pub fn build_signature(self, return_type: ILType) -> ILFunctionSignatureBuilder<'module> {
        ILFunctionSignatureBuilder::new(self).returns(return_type)
    }

    pub fn set_entry_block(&mut self, block: ILBlock) {
        self.entry_block_idx = block.block_idx;
    }

    pub fn switch_to_block(&mut self, block: StringKey) {
        let idx = self
            .blocks
            .iter()
            .enumerate()
            .find_map(|(i, b)| (b.label == block).then_some(i))
            .expect("block does not exist in function");
        self.active_block = Some(idx as u32);
    }

    pub fn new_unnamed_block(&mut self) -> StringKey {
        let (block, _idx) = self.prepare_new_empty_block();
        self.switch_to_block(block);
        block
    }
    pub fn new_named_block(&mut self, name: &str) -> Option<StringKey> {
        let (block, _idx) = self.prepare_new_named_block(name)?;
        self.switch_to_block(block);
        Some(block)
    }

    pub fn add_assignment<'a>(
        &'a mut self,
        assignee: &str,
        typ: ILTypeData,
    ) -> ILInstructionBuilder<'a, 'module, MissingValue> {
        let id = crate::intern::intern_str(assignee);
        let typ = self.for_module.typs_mut().get_key(typ);
        ILInstructionBuilder::new(self, id, typ)
    }
    /// Insert a function call, discarding its return value
    ///
    /// If you want to save the returned value, use [`add_instruction().call()`]
    pub fn add_call<'a>(&'a mut self, func: &str, args: &[(ILType, ILOperand)]) {
        let func = crate::intern::intern_str(func);
        self.push_nonassigned_call_into_block(ILCall {
            assignee: None,
            func,
            args: args.into(),
            is_variadic: false,
        });
    }
    /// Insert a function call, discarding its return value
    ///
    /// If you want to save the returned value, use [`add_instruction().call()`]
    pub fn add_variadic_call<'a>(&'a mut self, func: &str, args: &[(ILType, ILOperand)]) {
        let func = crate::intern::intern_str(func);
        self.push_nonassigned_call_into_block(ILCall {
            assignee: None,
            func,
            args: args.into(),
            is_variadic: true,
        });
    }
    pub fn terminate_jmp(&mut self, block_id: StringKey) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::Jmp(block_id);
        });
    }
    pub fn terminate_return_value(&mut self, val: &str) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::ReturnVal(ILOperand::Temp(ILTemp::NonSSA(
                crate::intern::intern_str(val),
            )));
        });
    }
    pub fn terminate_branch(&mut self, condition: ILTemp, taken: StringKey, not_taken: StringKey) {
        self.modify_active_block_with(|block| {
            block.terminator = ILTerminator::BranchIf(ILOperand::Temp(condition), taken, not_taken);
        });
    }

    pub fn finish_active_block(&mut self) -> ILBlock {
        let active_idx = self.get_active_block_index();
        self.active_block = None;
        ILBlock {
            block_idx: active_idx,
            func_id: self.defined_id,
        }
    }
    /// Pass over each block, and replace [`ILTerminator::FallThrough`]'s with explicit jumps to the next block.
    ///
    /// Returns an error if the last block has a Fallthrough terminator (where would control jump to?)
    ///
    /// This is useful because it allows us to reorder blocks later.
    fn make_fallthroughs_explicit(&mut self) -> Result<(), FunctionBuilderError> {
        for i in 0..(self.blocks.len() - 1) {
            if self.blocks[i].terminator == ILTerminator::FallThrough {
                let next_block = self.blocks.get(i + 1);
                let bnext_label = next_block
                    .ok_or(FunctionBuilderError::LastBlockFallsThrough)?
                    .label;
                self.blocks[i].terminator = ILTerminator::Jmp(bnext_label);
            }
        }

        Ok(())
    }

    pub fn finish_function(mut self) -> Result<ILFunction, FunctionBuilderError> {
        if self.blocks.is_empty() {
            return Err(FunctionBuilderError::EmptyFunction);
        }
        self.make_fallthroughs_explicit()?;

        // We may now safely reorder blocks, as there are no FallThrough terminators anymore

        self.blocks.swap(0, self.entry_block_idx as usize);

        if let Some(signature) = self.signature {
            let f_data = ILFunctionData {
                signature,
                name: self.name,
                blocks: self.blocks,
                linkage: ILLinkage::Export,
            };
            self.for_module.ctx.functions[self.defined_id] = f_data;
            Ok(self.defined_id)
        } else {
            Err(FunctionBuilderError::MissingSignature)
        }
    }
}
impl<'module> ILFunctionBuilder<'module> {
    fn generate_block_label(&self, label_num: u32) -> String {
        let fname = crate::intern::unintern_boxstr(self.name);
        format!("{}:{}.{}", fname, self.defined_id.into_usize(), label_num)
    }

    fn prepare_new_empty_block(&mut self) -> (StringKey, u32) {
        let label = self.generate_block_label(self.label_gen);
        self.prepare_new_named_block(&label)
            .expect("generated label is not unique")
    }
    pub fn prepare_new_named_block(&mut self, label: &str) -> Option<(StringKey, u32)> {
        let label = crate::intern::intern_str(label);
        self.label_gen += 1;
        let empty_block = ILBlockData {
            label,
            items: vec![],
            terminator: super::ILTerminator::FallThrough,
            phis: vec![],
        };
        if self.blocks.iter().find(|b| b.label == label).is_some() {
            return None;
        }
        self.blocks.push(empty_block);
        Some((label, self.label_gen - 1))
    }
    /// get the index to the currently active block, or insert one if there is none.
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
        assignee: StringKey,
    ) -> ILTemp {
        let assignee = ILTemp::NonSSA(assignee);
        self.modify_active_block_with(|block| {
            let item = ILBlockItem::AssignInstr(assignee, typ, value);
            block.items.push(item);
        });
        assignee
    }
    fn push_assigned_call_into_block(&mut self, call: ILCall) -> ILTemp {
        let assignee = call.assignee.unwrap().0;
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
}

pub struct ILFunctionSignatureBuilder<'module> {
    for_function: ILFunctionBuilder<'module>,
    returns: Option<ILType>,
    param_types: Vec<ILType>,
    param_temps: Vec<ILTemp>,
    param_gen: u32,
}
impl<'module> ILFunctionSignatureBuilder<'module> {
    pub fn new(for_function: ILFunctionBuilder<'module>) -> Self {
        Self {
            for_function,
            returns: None,
            param_types: vec![],
            param_temps: vec![],
            param_gen: 0,
        }
    }
    pub fn add_param(mut self, typ: ILType) -> Self {
        let param_idx = self.param_types.len();
        let id = format!("p{param_idx}");
        let id = crate::intern::intern_str(&id);
        let temp = ILTemp::SSA(id, self.param_gen);
        self.param_gen += 1;
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
            param_types: self.param_types.into_boxed_slice(),
            param_temporaries: self.param_temps.into_boxed_slice(),
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
    assignee: StringKey,
    typ: ILType,
    progress: S,
}

impl<'a, 'thisbuilder> ILInstructionBuilder<'a, 'thisbuilder, MissingValue> {
    pub fn new(
        for_function: &'a mut ILFunctionBuilder<'thisbuilder>,
        assignee: StringKey,
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
    pub fn copy(self, name: &str) -> ILTemp {
        let data = ILInstruction::Copy(ILOperand::Temp(ILTemp::NonSSA(crate::intern::intern_str(
            name,
        ))));
        self.into_has_instruction(data).epilogue()
    }
    pub fn add(self, a: ILTemp, b: ILTemp) -> ILTemp {
        let data = ILInstruction::add(ILOperand::Temp(a), ILOperand::Temp(b));
        self.into_has_instruction(data).epilogue()
    }
    pub fn cmp_is_zero(self, as_typ: ILType, val: ILTemp) -> ILTemp {
        let data = ILInstruction::cmp_z(as_typ, ILOperand::Temp(val));
        self.into_has_instruction(data).epilogue()
    }

    pub fn call(self, func: &str, args: &[(ILType, ILOperand)]) -> ILTemp {
        let assigns_to = Some((ILTemp::NonSSA(self.assignee), self.typ));
        let func = crate::intern::intern_str(func);
        self.for_function.push_assigned_call_into_block(ILCall {
            assignee: assigns_to,
            func,
            args: args.into(),
            is_variadic: false,
        })
    }
}
/// A DSL to express an ILModule containing functions in a declarative way.
///
/// functions have a trailing return type, and have no parameter names.
///
/// temporaries are built as their literal name, but assigned to a variable that is named like so:
/// a declaration of `|binding_name| temp_name =<typ> (<value>);` generates an assignment to
/// `binding_name: ILTemp`
///
/// A block is declared like so:
/// `|ilblock_binding| @block_label
///  <temporaries>*
///  <terminator>
///`
/// A Key for the constructed [`ILBlock`] is assigned to `ilblock_binding`
/// and can be used later by the [`ILModule`] for querying the block
/// using `module.get_block(ilblock_binding)`.
///
/// ```rust
/// use librqbe::il::*;
/// use librqbe::il::builder::*;
/// let mut module_builder = librqbe::il::builder::ILModuleBuilder::start();
/// let t_i8 = librqbe::il::ILTypeData::I8;
/// let ti_i8 = module_builder.get_type_I8();
/// let builder = &mut module_builder;
///
/// use librqbe::builder_dsl;
/// use librqbe::intern;
/// builder_dsl!(func builder;
///     function foo(ti_i8) -> ti_i8 {
///         |start_block| @start
///         |start_foo_temp| foo =t_i8 (3);
///         ! (jmpz foo, @bt, @bf)
///
///         |bt_block| @bt
///         |bt_a_temp| a =t_i8 (4);
///         !(jmp @merge)
///
///         |bf_block| @bf
///         |bf_a_temp| a =t_i8 (5);
///         !(jmp @merge)
///
///         |merge_fin| @merge
///         !(jmp @start)
///     }
/// );
///
/// let m = module_builder.finish();
/// assert_eq!(m.entry_block_of_func(foo).label, start);
/// assert_eq!(m.get_block(start_block).label, start);
/// assert_eq!(m.get_block(bt_block).label, bt);
/// assert_eq!(m.get_block(bf_block).label, bf);
/// ```
#[macro_export]
macro_rules! builder_dsl {
        (term $builder:expr ; jmp @ $label:ident) => {
            $builder.terminate_jmp($label);
        };
        (term $builder:expr ; jmpz $cond:ident, @$bt:ident, @$bf:ident) => {
            $builder.terminate_branch(ILTemp::NonSSA($crate::intern::intern_str(stringify!($cond))), $bt, $bf);
        };
        (term $builder:expr ; ret $val:ident) => {
            $builder.terminate_return_value(stringify!($val));
        };
        (block $builder:expr ;
        @ $label:ident
            $(temp | $assignee_ident:ident| $assignee_name:ident = $typ:ident  ($($instr:tt)+) ;)*
            term ($($term:tt)+)
        ) => {
            $builder.switch_to_block($label);
            $(builder_dsl! {temp $builder ; $assignee_ident, $assignee_name, $typ,  $($instr)+})*
            builder_dsl!(term $builder ; $($term)+)
        };
        (temp $builder:expr ; $assignee_name:ident, $typ:ident) => {
            $builder.add_assignment(stringify!($assignee_name), $typ)
        };
        (temp $builder:expr ; $assignee_ident:ident, $assignee_name:ident , $typ:ident , add $a:ident, $b:ident) => {
            let $assignee_ident = builder_dsl!(temp $builder ; $assignee_name, $typ).add($a, $b);
            let _ = $assignee_ident;
        };
        (temp $builder:expr ; $assignee_ident:ident, $assignee_name:ident , $typ:ident , $usetemp:ident) => {
            let $assignee_ident = builder_dsl!(temp $builder ; $assignee_name, $typ).copy(stringify!($usetemp));
            let _ = $assignee_ident;
        };
        (temp $builder:expr ;  $assignee_ident:ident, $assignee_name:ident , $typ:ident , cmpz $a:ident as $typcmp:ident) => {
            let $assignee_ident = builder_dsl!(temp $builder ; $assignee_name, $typ).cmpz($typcmp, $a);
            let _ = $assignee_ident;

        };
        (temp $builder:expr ; $assignee_ident:ident, $assignee_name:ident , $typ:ident , $instr:literal) => {
            let $assignee_ident = builder_dsl!(temp $builder ; $assignee_name, $typ).imm_u64($instr);
            let _ = $assignee_ident;
        };
        (func $builder:expr ; function $name:ident () -> $rettype:ident
            {
                $(
                @$blocklabel:ident
                $(|$assignee_ident:ident| $assignee_name:ident = $typ:ident  ($($instr:tt)+) ;)*
                ! ($($term:tt)+) ;
                )+
            }
              ) => {
                let mut __builder = $builder.add_function(stringify!($name)).build_signature($rettype).finish_signature();
                $(
                    let $blocklabel = __builder.new_fresh_block();
                )+
                $(
                    builder_dsl!(block &mut __builder;
                    @$blocklabel in $name
                    $(temp |$assignee_ident| $assignee_name =$typ ($($instr)+) ;)*
                    term ($($term)+)
                    );
                    paste::paste!(
                    let [<block_ $blocklabel >] = __builder.finish_active_block();
                    );
                )+
                let $name = __builder.finish_function().unwrap();
                let _ = $name;
            };

        (func $builder:expr ; function $name:ident ($($argtyp:ident),+) -> $rettype:ident
            {
                $(
                |$blocklabelfinished:ident| @$blocklabel:ident
                $(|$assignee_ident:ident| $assignee_name:ident = $typ:ident  ($($instr:tt)+) ;)*
                ! ($($term:tt)+)
                )+
            }
              ) => {
                  let mut __args = $builder.add_function(stringify!($name)).build_signature($rettype);
                  $(__args = __args.add_param($argtyp);)+
                let mut __builder = __args.finish_signature();
                $(
                    let $blocklabel = __builder.new_named_block(stringify!($blocklabel)).expect(concat!("duplicate block label ", stringify!($blocklabel), "."));
                )+
                $(
                    builder_dsl!(block &mut __builder;
                    @$blocklabel
                    $(temp |$assignee_ident| $assignee_name =$typ ($($instr)+) ;)*
                    term ($($term)+)
                    );
                    let $blocklabelfinished = __builder.finish_active_block();
                    let _ = $blocklabelfinished;
                )+
                let $name = __builder.finish_function().unwrap();
                let _ = $name;
            };
}
