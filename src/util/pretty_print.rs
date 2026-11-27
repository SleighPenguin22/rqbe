use crate::il::{
    ILABITyp, ILBlockData, ILBlockItem, ILCall, ILDataDefinition, ILDataLayoutKind, ILFunctionData,
    ILGlobal, ILGlobalSymbol, ILInstruction, ILModule, ILModuleContext, ILOperand, ILPhiNode,
    ILTemp, ILTerminator, SSATemporary, UnOp,
};
use crate::il::{ILDataLayout, ILType};
use crate::{StringKey, il};
use keyvec::InternKey;

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
            ILBlockItem::AssignInstr(iltemp, typ, ilval_instr) => format!(
                "{} ={} {}",
                iltemp.display_module_item(ctx),
                typ.display_module_item(ctx),
                ilval_instr.display_module_item(ctx)
            ),
            ILBlockItem::Store(typ, ilvalue, addr) => format!(
                "store{} {}, {}",
                typ.display_module_item(ctx),
                ilvalue.display_module_item(ctx),
                addr.display_module_item(ctx),
            ),
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
impl DisplayModuleItem for ILABITyp {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let base = self.base.display_module_item(ctx);
        let prefix = match self.sign {
            Some(true) => "s",
            Some(false) => "u",
            None => "",
        };
        format!("{prefix}{base}")
    }
}

impl DisplayModuleItem for UnOp {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self {
            UnOp::Neg => "neg".to_string(),
            UnOp::Alloc4 => "alloc4".to_string(),
            UnOp::Alloc8 => "alloc8".to_string(),
            UnOp::Alloc16 => "alloc16".to_string(),
            UnOp::Ext(ilabityp) => format!("ext{}", ilabityp.display_module_item(ctx)),

            UnOp::ExtS => "exts".to_string(),
            UnOp::TruncD => "truncd".to_string(),
            UnOp::ToFloat(ilabityp) => format!("{}tof", ilabityp.display_module_item(ctx)),
            UnOp::FromF64(ilabityp) => format!("dto{}", ilabityp.display_module_item(ctx)),
            UnOp::FromF32(ilabityp) => format!("sto{}", ilabityp.display_module_item(ctx)),
        }
        .to_string()
    }
}

impl DisplayModuleItem for ILCall {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let fname = crate::intern::unintern_boxstr(self.func);
        let args = join_display_module_items_tuple(", ", " ", &self.args, ctx);
        match self.assignee {
            Some((temp, typ)) => {
                format!(
                    "{} ={} call ${}({args})",
                    temp.display_module_item(ctx),
                    typ.display_module_item(ctx),
                    fname
                )
            }
            None => {
                format!("call ${}({args})", fname)
            }
        }
    }
}
impl DisplayModuleItem for ILGlobal {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_global(*self);
        let name = crate::intern::unintern_boxstr(data.name);
        format!(
            "${name} : {} = {:?}\n",
            data.layout.display_module_item(ctx),
            data.items
        )
    }
}
impl DisplayModuleItem for ILDataDefinition {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let name = crate::intern::unintern_boxstr(self.name);
        format!(
            "${name} : {} = {:?}\n",
            self.layout.display_module_item(ctx),
            self.items
        )
    }
}

impl DisplayModuleItem for (ILType, usize) {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        format!("{} {}", self.0.display_module_item(ctx), self.1)
    }
}

impl DisplayModuleItem for ILDataLayout {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self.layout_kind {
            ILDataLayoutKind::Padded => {
                format!(
                    "Padded {{{}}}",
                    join_display_module_items(", ", &self.field_declarations, ctx)
                )
            }
            ILDataLayoutKind::Packed => {
                format!(
                    "Packed {{{}}}",
                    join_display_module_items(", ", &self.field_declarations, ctx)
                )
            }
        }
    }
}
impl DisplayModuleItem for ILType {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        format!("{self}")
    }
}

impl DisplayModuleItem for &[(StringKey, ILOperand)] {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let items: Vec<String> = self
            .iter()
            .map(|(block, val)| format!("{} {}", block.unintern(), val.display_module_item(ctx)))
            .collect();
        items.join(", ")
    }
}

impl DisplayModuleItem for ILBlockData {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let mut buf = String::new();
        let sep = "\n    ";
        for p in &self.phis {
            buf += &format!(
                "    {} ={} {}\n",
                p.assignee.display_module_item(ctx),
                p.typ.display_module_item(ctx),
                p.incoming.as_slice().display_module_item(ctx)
            );
        }
        let items = join_display_block_items(&self.items, ctx);
        let term = self.terminator.display_module_item(ctx);
        format!(
            "  @{}\n{buf}    {items}{sep}{term}\n",
            self.label.unintern()
        )
    }
}

struct Poop;

fn foo() {
    let b = Box::new(Poop);
    // let a = Box::allocator(&b);
    // let p = *b;
    let (a, b) = Box::take(b);
}

impl DisplayModuleItem for ILTemp {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        format!("{self}")
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
            ILTerminator::Halt => "hlt".to_string(),
            ILTerminator::Jmp(ilblock) => {
                format!("jmp {}", ilblock.unintern())
            }
            ILTerminator::BranchIf(ilvalue, ilblock, ilblock1) => {
                let v = ilvalue.display_module_item(ctx);
                let bt = ilblock.unintern();
                let bf = ilblock1.unintern();
                format!("jnz {v}, @{bt}, @{bf}")
            }
            ILTerminator::Return => "ret".to_string(),
            ILTerminator::ReturnVal(ilvalue) => {
                format!("ret {}", ilvalue.display_module_item(ctx))
            }
            ILTerminator::FallThrough => "".to_string(),
        }
    }
}

impl DisplayModuleItem for ILOperand {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match *self {
            ILOperand::Temp(iltemp) => iltemp.display_module_item(ctx),
            ILOperand::Global(string_id) => string_id.unintern().into_string(),
            ILOperand::ConstInt(i) => i.to_string(),
            ILOperand::ConstFloat(f) => format!("s_{}", f32::from_bits(f)),
            ILOperand::ConstDouble(d) => format!("d_{}", f64::from_bits(d)),
        }
    }
}

impl DisplayModuleItem for ILInstruction {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        match self {
            ILInstruction::Load(t, i) => format!(
                "load{} {}",
                t.display_module_item(ctx),
                i.display_module_item(ctx)
            ),
            ILInstruction::Immi64(i) => format!("copy {i}"),
            ILInstruction::Immf64(f) => format!("copy {f}"),
            ILInstruction::Copy(id) => format!("copy {}", id.display_module_item(ctx)),
            ILInstruction::Cmp(op, t, a, b) => {
                format!(
                    "c{op}{} {}, {}",
                    t.display_module_item(ctx),
                    a.display_module_item(ctx),
                    b.display_module_item(ctx)
                )
            }
            ILInstruction::Binop(op, v1, v2) => {
                format!(
                    "{op} {}, {}",
                    v1.display_module_item(ctx),
                    v2.display_module_item(ctx)
                )
            }
            ILInstruction::Unop(un_op, iloperand) => {
                format!(
                    "{} {}",
                    un_op.display_module_item(ctx),
                    iloperand.display_module_item(ctx)
                )
            }
        }
    }
}

impl DisplayModuleItem for ILFunctionData {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let name = self.name.unintern();
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
        let linkage = self.linkage.display_module_item(ctx);
        format!("{linkage} function {returns} ${name} ({params}) {{\n{blocks}\n}}\n")
    }
}
impl DisplayModuleItem for il::ILLinkage {
    fn display_module_item(&self, _ctx: &ILModuleContext) -> String {
        match self {
            il::ILLinkage::Export => "export",
            il::ILLinkage::Thread => "thread",
            il::ILLinkage::ExportThread => "export thread",
            il::ILLinkage::Internal => "",
        }
        .to_string()
    }
}

impl DisplayModuleItem for ILGlobalSymbol {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let data = ctx.get_symbol(*self);
        match data {
            crate::il::ILGlobalSymbolData::Func(string_id) => {
                let funcname = ctx.get_function(*string_id).name;
                format!("${}", funcname.unintern())
            }
            crate::il::ILGlobalSymbolData::Global(string_id) => {
                format!("${}", string_id.unintern())
            }
        }
    }
}
impl DisplayModuleItem for ILPhiNode {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        format!(
            "    {} ={} phi {}\n",
            self.assignee.display_module_item(ctx),
            self.typ.display_module_item(ctx),
            self.incoming.as_slice().display_module_item(ctx)
        )
    }
}

impl DisplayModuleItem for ILModule {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let mut buffer = String::with_capacity(256);
        for glob in self.globals().values() {
            buffer += &glob.display_module_item(ctx);
        }
        for func in self.functions().values() {
            buffer += &func.display_module_item(ctx);
        }
        buffer
    }
}
