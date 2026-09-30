use idset::InternKey;

use crate::il::{
    ILBlock, ILBlockData, ILBlockItem, ILCall, ILDataLayoutKind, ILFunctionData, ILGlobal,
    ILGlobalData, ILGlobalSymbol, ILLayout, ILLayoutData, ILModule, ILModuleContext, ILPhiNode,
    ILTemp, ILTerminator, ILType, ILValInstr, ILValInstrData, ILValue, SSATemporary, StringID,
};

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

fn join_display_block_items(items: &[ILBlockItem], ctx: &ILModuleContext) -> String {
    items
        .iter()
        .map(|item| match item {
            ILBlockItem::Call(ilcall) => ilcall.display_module_item(ctx),
            ILBlockItem::AssignInstr(iltemp, ilval_instr) => format!(
                "{} = {}",
                iltemp.display_module_item(ctx),
                ilval_instr.display_module_item(ctx)
            ),
            ILBlockItem::Store(iltemp, _) => format!("store {}", iltemp.display_module_item(ctx)),
        })
        .collect::<Vec<String>>()
        .join("\n    ")
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
impl DisplayModuleItem for ILCall {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let fname = ctx.get_function(self.func).name;
        let args = join_display_module_items_tuple(", ", " ", &self.args, ctx);
        match self.assigns_to {
            Some((temp, typ)) => {
                format!(
                    "{} ={} call {}({args})",
                    temp.display_module_item(ctx),
                    typ.display_module_item(ctx),
                    fname.display_module_item(ctx)
                )
            }
            None => {
                format!("call {}({args})", fname.display_module_item(ctx))
            }
        }
    }
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
        let items = join_display_block_items(&self.items, ctx);
        let term = self.terminator.display_module_item(ctx);
        format!(
            "  @{}:\n    {items}{sep}{term}\n",
            self.label.display_module_item(ctx)
        )
    }
}

impl DisplayModuleItem for ILTemp {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self {
            ILTemp::NonSSA(string_id) => format!("%{}", string_id.display_module_item(ctx)),
            ILTemp::SSA(iltemporary) => iltemporary.display_module_item(ctx),
        }
    }
}

impl DisplayModuleItem for ILBlock {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        format!("{}:{}", self.func_id.into_usize(), self.block_id)
    }
}

impl DisplayModuleItem for SSATemporary {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        format!("%{}", self.into_backing())
    }
}

impl DisplayModuleItem for ILTerminator {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self {
            ILTerminator::Unspecified => "!!BuilderNotFinished!!".to_string(),
            ILTerminator::Halt => "hlt".to_string(),
            ILTerminator::Jmp(ilblock) => {
                format!("jmp {}", ilblock.display_module_item(ctx))
            }
            ILTerminator::BranchIf(ilvalue, ilblock, ilblock1) => {
                let v = ilvalue.display_module_item(ctx);
                let bt = ilblock.display_module_item(ctx);
                let bf = ilblock1.display_module_item(ctx);
                format!("jmpz {v}, {bt}, {bf}")
            }
            ILTerminator::Return => "ret".to_string(),
            ILTerminator::ReturnVal(ilvalue) => {
                format!("ret {}", ilvalue.display_module_item(ctx))
            }
        }
    }
}

impl DisplayModuleItem for ILValue {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match *self {
            ILValue::Assignee(iltemp) => iltemp.display_module_item(ctx),
            ILValue::Global(string_id) => string_id.display_module_item(ctx),
            ILValue::ConstInt(i) => i.to_string(),
            ILValue::ConstFloat(f) => format!("s_{}", f32::from_bits(f)),
            ILValue::ConstDouble(d) => format!("d_{}", f64::from_bits(d)),
        }
    }
}

impl DisplayModuleItem for ILValInstr {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_value(*self);
        let suffix = format!("\t// {:?}", self);
        let mut stem = match data {
            ILValInstrData::Load(i) => format!("load {}", i.display_module_item(ctx)),
            ILValInstrData::Immi64(i) => format!("immi64 {i}"),
            ILValInstrData::Immf64(f) => format!("immf64 {f}"),
            ILValInstrData::Copy(id) => id.display_module_item(ctx),
            ILValInstrData::Global(string_id) => {
                format!("${}", string_id.display_module_item(ctx))
            }
            ILValInstrData::CmpZ(v) => {
                format!("CmpZ {}", v.display_module_item(ctx))
            }
            ILValInstrData::Add(v1, v2) => {
                format!(
                    "Add {}, {}",
                    v1.display_module_item(ctx),
                    v2.display_module_item(ctx)
                )
            }
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
            .map(|(block, val)| {
                format!(
                    "{}: {}",
                    block.display_module_item(ctx),
                    val.display_module_item(ctx)
                )
            })
            .collect();
        let items = items.join(", ");
        format!("    P{} := [{items}]", self.into_backing())
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
