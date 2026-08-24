#![allow(unused)]
use crate::ssa_il::builder::ILModuleBuilder;
pub struct ParserState<'s> {
    ctx: ILModuleBuilder,
    source: &'s str,
}

impl<'s> ParserState<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            ctx: ILModuleBuilder::start(),
            source,
        }
    }
}
