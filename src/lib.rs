#![feature(min_generic_const_args)]
#![feature(generic_const_args)]
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}
pub mod il;
pub mod parse;
mod ssaify;
mod target;
mod util;
pub use ssaify::{CFGGraph, CFGGraphBuilder};
pub use target::{CompilationTarget, InvalidTargetError, ToTargetEndianBytes};
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
