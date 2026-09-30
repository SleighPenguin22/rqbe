//! This module contains the code necessary to convert a non-SSA IR into an SSA one

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use idset::{InternKey, KeyVec, internkey};

use crate::il::{
    ILBlock, ILBlockData, ILFunction, ILModule, ILModuleContext, ILTerminator, StringID,
};
use crate::util::pretty_print::DisplayModuleItem;

internkey!(CFGBlock);
#[derive(Debug, Clone)]
pub struct CFGBlockData {
    preds: HashSet<StringID>,
    succs: HashSet<StringID>,
    block: crate::il::StringID,
}

impl PartialEq for CFGBlockData {
    fn eq(&self, other: &Self) -> bool {
        self.block == other.block
    }
}
impl Eq for CFGBlockData {}
impl Hash for CFGBlockData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.block.hash(state);
    }
}

impl CFGBlockData {
    pub fn empty(block: StringID) -> Self {
        {
            Self {
                preds: HashSet::new(),
                succs: HashSet::new(),
                block,
            }
        }
    }
}
#[derive(Debug, Clone)]
pub struct CFGGraph<'m> {
    blocks: KeyVec<CFGBlock, CFGBlockData>,
    map: HashMap<StringID, CFGBlock>,
    for_module: &'m ILModule,
}

impl<'m> CFGGraph<'m> {
    pub fn empty(for_module: &'m ILModule) -> Self {
        Self {
            for_module,
            map: HashMap::new(),
            blocks: KeyVec::new(),
        }
    }

    /// Get the [CFGBlock] associated with the given [ILBLock],
    /// or create it if it did not exist.
    fn get_cfg_id(&mut self, il_id: StringID) -> CFGBlock {
        match self.map.get(&il_id) {
            Some(&cfg_id) => cfg_id,
            None => {
                let dummy = CFGBlockData::empty(il_id);
                let cfg_id = self.blocks.push(dummy);
                self.map.insert(il_id, cfg_id);
                cfg_id
            }
        }
    }
    /// Get the [CFGBlockData] associated with the given [ILBLock],
    /// or create it if it did not exist.
    fn get_cfgblock_mut(&mut self, il_id: StringID) -> (CFGBlock, &mut CFGBlockData) {
        let id = self.get_cfg_id(il_id);
        // unwrap because the above call guarantees il_id now has `CFGBlockData` associated with it
        let block = self.blocks.get_mut(id).unwrap();
        (id, block)
    }
    /// Perform `add_predecessor(dest,src)` and `add_successor(src,dest)`
    fn add_link(&mut self, src: StringID, dest: StringID) {
        {
            let (_, block) = (&mut *self).get_cfgblock_mut(dest);
            block.preds.insert(src);
            let (_, block) = (&mut *self).get_cfgblock_mut(src);
            block.succs.insert(dest);
        };
    }

    pub fn pretty(&self) -> String {
        let mut s = String::with_capacity(128);
        for block in self.blocks.as_vec().iter() {
            let ppretty = |i: StringID| i.display_module_item(&self.for_module.ctx);
            let preds: Vec<String> = block.preds.iter().copied().map(ppretty).collect();
            let preds = if preds.is_empty() {
                "()"
            } else {
                &preds.join(", ")
            };
            let succs: Vec<String> = block.succs.iter().copied().map(ppretty).collect();
            let succs = if succs.is_empty() {
                "()"
            } else {
                &succs.join(", ")
            };
            s += &format!(
                "{preds} -> @{} -> {succs}\n",
                block.block.display_module_item(&self.for_module.ctx)
            );
        }
        s
    }
}

pub struct CFGGraphBuilder<'module> {
    graph: CFGGraph<'module>,
}

impl<'module> CFGGraphBuilder<'module> {
    pub fn new(module: &'module ILModule) -> Self {
        Self {
            graph: CFGGraph::empty(module),
        }
    }
    pub fn build(mut self) -> CFGGraph<'module> {
        self.populate_blocks();
        self.graph
    }

    fn populate_blocks(&mut self) {
        for (func_id, func) in self.graph.for_module.functions().iter_kv() {
            self.populate_blocks_for_func(func_id, func);
        }
    }
    fn populate_blocks_for_func(&mut self, func_id: ILFunction, func: &crate::il::ILFunctionData) {
        for (_idx, block) in self.graph.for_module.iter_blocks(func_id, func) {
            let _b = self.cfgify_block(block.label, block);
        }
    }

    fn cfgify_block(&mut self, block_id: StringID, block: &crate::il::ILBlockData) -> CFGBlock {
        self.cfgify_block_items(block_id, block);
        self.cfigify_terminator(block_id, block);
        self.graph.get_cfg_id(block_id)
    }

    fn cfgify_block_items(&mut self, block_id: StringID, block: &ILBlockData) {
        for (_, phi_id) in block.phis.iter().copied() {
            let phi = self.graph.for_module.get_phi_node(phi_id);
            for (src_block, _) in phi.incoming.iter().copied() {
                self.graph.add_link(src_block, block_id);
            }
        }

        for item in block.items.iter() {
            if let crate::il::ILBlockItem::Call(ilcall) = item {
                let func_entry = self.graph.for_module.entry_block_of_func(ilcall.func);

                self.graph.add_link(block_id, func_entry.label);
            }
        }
    }

    fn cfigify_terminator(&mut self, block_id: StringID, block: &ILBlockData) {
        match block.terminator {
            ILTerminator::Halt => todo!(),
            ILTerminator::Jmp(dest) => {
                self.graph.add_link(block_id, dest);
            }
            ILTerminator::BranchIf(_, btrue, bfalse) => {
                self.graph.add_link(block_id, btrue);
                self.graph.add_link(block_id, bfalse);
            }
            _ => {}
        }
    }
}
