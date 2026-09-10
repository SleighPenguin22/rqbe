pub fn add(left: u64, right: u64) -> u64 {
    left + right
}
pub mod il;
mod parser;
mod ssaify;
pub mod textparser;
mod util;
pub use ssaify::{CFGGraph, CFGGraphBuilder};
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
