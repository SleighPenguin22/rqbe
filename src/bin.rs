use std::error::Error;

use librqbe::ssaify::DJControlFlowGraph;
use librqbe::target::CompilationTarget;
use librqbe::{il::*, parse::Tokenize};

fn main() -> Result<(), Box<dyn Error>> {
    simplelog::SimpleLogger::init(log::LevelFilter::Trace, simplelog::Config::default()).unwrap();
    let mut b = builder::ILModuleBuilder::start();
    let i8_ = b.get_type_I8();
    let i32_ = b.get_type_I32();
    let mut foo = b
        .add_function("foo")
        .build_signature(i8_)
        .add_param(i8_)
        .finish_signature();

    let bcond = foo.new_fresh_block();
    let bthen = foo.new_fresh_block();
    let belse = foo.new_fresh_block();
    let bjoin = foo.new_fresh_block();

    foo.switch_to_block(bcond);
    let t0 = foo.add_assignment("t0", ILTypeData::I32).imm_u64(3);
    let t1 = foo.add_assignment("t1", ILTypeData::I32).imm_u64(3);
    foo.terminate_branch(t1, bthen, belse);
    let binit = foo.finish_active_block();

    foo.switch_to_block(bthen);
    let t3 = foo.add_assignment("t3", ILTypeData::I32).imm_u64(4);
    foo.terminate_jmp(bjoin);
    let bthen = foo.finish_active_block();

    foo.switch_to_block(belse);
    let t4 = foo.add_assignment("t4", ILTypeData::I64).add(t3, t3);
    foo.terminate_jmp(bjoin);
    let belse = foo.finish_active_block();

    foo.switch_to_block(bjoin);
    let t5 = foo.add_assignment("t5", ILTypeData::I64).add(t0, t1);
    foo.terminate_return_value(t5);
    let bjoin = foo.finish_active_block();

    foo.set_entry_block(binit);

    let foo = foo.finish_function().unwrap();

    let mut b = b.finish();

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
