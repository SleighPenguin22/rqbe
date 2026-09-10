use crate::{il::ILModule, util::pretty_print::DisplayModuleItem};

impl ILModule {
    pub fn display_module(&self) -> String {
        self.display_module_item(&self.ctx)
    }
}
#[inline]
pub(crate) fn decode_label(block: &str) -> Option<(usize, usize)> {
    let s = block.find(':')?;
    let (f, b) = block.split_at_checked(s)?;
    let f = f.parse().ok()?;
    let b = b.parse().ok()?;
    Some((f, b))
}
pub(crate) mod pretty_print;
