use std::{error::Error, sync::atomic::ATOMIC_BOOL_INIT};

use librqbe::{
    CFGGraphBuilder, CompilationTarget, DJControlFlowGraph, DJGraph, il::*, parse::Tokenize,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut b = builder::ILModuleBuilder::start();
    let i8_ = b.get_type_I8();
    let i32_ = b.get_type_I32();
    let _glob = b
        .add_global_data()
        .with_name("glob")
        .build_layout(ILDataLayoutKind::Packed)
        .add_field(i32_)
        .add_field(i8_)
        .finish_layout()
        .with_bits(&[2, 2])
        .finish_global();
    let mut foo = b
        .add_function("foo")
        .build_signature(i8_)
        .add_param(i8_)
        .finish_signature();

    let binit = foo.new_fresh_block();
    let bcond = foo.new_fresh_block();
    let bthen = foo.new_fresh_block();
    let belse = foo.new_fresh_block();
    let bjoin = foo.new_fresh_block();
    let bprejoin = foo.new_fresh_block();

    foo.switch_to_block(binit);
    let a = foo.add_assignment("t0", ILTypeData::I32).imm_u64(3);
    foo.terminate_branch(a, bcond, bprejoin);
    let binit = foo.finish_active_block();

    foo.switch_to_block(bprejoin);
    let a = foo.add_assignment("t0", ILTypeData::I32).imm_u64(3);
    foo.terminate_branch(a, bcond, bjoin);
    let bprejoin = foo.finish_active_block();

    foo.switch_to_block(bcond);
    let a = foo.add_assignment("t0", ILTypeData::I32).imm_u64(4);
    foo.terminate_branch(a, bthen, belse);
    let bcond = foo.finish_active_block();

    foo.switch_to_block(bthen);
    let c = foo.add_assignment("t0", ILTypeData::I32).imm_u64(4);
    foo.terminate_jmp(bjoin);
    let bthen = foo.finish_active_block();

    foo.switch_to_block(belse);
    let sum = foo.add_assignment("t0", ILTypeData::I64).add(a, c);
    foo.terminate_jmp(bjoin);
    let belse = foo.finish_active_block();

    foo.switch_to_block(bjoin);
    let sum = foo.add_assignment("t0", ILTypeData::I64).add(a, c);
    foo.terminate_return_value(sum);
    let bjoin = foo.finish_active_block();

    foo.set_entry_block(binit);

    let foo = foo.finish_function().unwrap();

    let b = b.finish();

    println!("{}", b.display_module());
    let (cfg, dj) = DJControlFlowGraph::build(&b, foo);
    let pretty_cfg = cfg.dump_dot(&b.ctx(), true);
    let pretty_dj = dj.dump_dot(&b.ctx());
    println!("digraph {{\n{pretty_cfg}\n{pretty_dj}\n}}");
    let t = CompilationTarget::host();

    let s = include_str!("../../qbe/test/abi1.ssa");
    let mut t = Tokenize::new(s);
    // while let Some(tok) = t.next_token() {
    //     println!("{}", tok.to_string(&t));
    // }
    //
    //
    Ok(())
}
