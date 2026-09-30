use std::io::Write;

use librqbe::{CFGGraphBuilder, CompilationTarget, ToTargetEndianBytes, il::*, parse::Tokenize};

fn main() {
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
    let mut f = b
        .add_function("foo")
        .build_signature(i8_)
        .add_param(i8_)
        .finish_signature();

    let b0 = f.new_fresh_block();
    let b1 = f.new_fresh_block();
    let b2 = f.new_fresh_block();

    f.switch_to_block(b0);
    let a = f.add_instruction("bob").imm_u64(3);
    f.terminate_jmp(b1);
    let b0 = f.finish_active_block();

    f.switch_to_block(b1);
    let c = f.add_instruction("cat").imm_u64(4);
    f.terminate_jmp(b2);
    let _b1 = f.finish_active_block();

    f.switch_to_block(b2);
    let sum = f.add_instruction("bar").add(a, c);
    f.terminate_return_value(sum);
    let _b2 = f.finish_active_block();

    f.set_entry_block(b0);

    let _f = f.finish_function().unwrap();
    let b = b.finish();

    println!("{}", b.display_module());
    let cfg = CFGGraphBuilder::new(&b).build();
    let pretty = cfg.pretty();
    println!("cfg: {}", pretty);
    let t = CompilationTarget::host();

    let s = include_str!("../../qbe/test/abi1.ssa");
    let mut t = Tokenize::new(s);
    // while let Some(tok) = t.next_token() {
    //     println!("{}", tok.to_string(&t));
    // }
}
