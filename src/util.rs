use crate::{il::ILModule, util::pretty_print::DisplayModuleItem};

impl ILModule {
    pub fn display_module(&self) -> String {
        self.display_module_item(&self.ctx)
    }
}
pub(crate) mod pretty_print;
