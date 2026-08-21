use librqbe::il::*;

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

    f.switch_to_fresh_block();
    let a = f.add_instruction().imm_u64(3);
    let a2 = f.add_instruction().imm_u64(4);
    let sum = f.add_instruction().add(a, a2);
    let b0 = f.finish_active_block(ILTerminator::ReturnVal(sum));

    f.switch_to_fresh_block();
    let b1 = f.finish_active_block(ILTerminator::Jmp(b0));

    f.set_entry_block(b1);

    let _f = f.finish_function();
    let b = b.finish();

    println!("{}", b.display_module())
}
