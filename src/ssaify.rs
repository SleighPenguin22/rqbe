use std::collections::HashMap;

use interntable::{InternTable, internkey};

use crate::il::{ILBlock, ILBlockData};

internkey!(CFGBlock);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CFGBlockData {
    preds: Vec<CFGBlock>,
    succs: Vec<CFGBlock>,
    block: crate::il::ILBlock,
}

impl CFGBlockData {
    pub fn new(preds: Vec<CFGBlock>, succs: Vec<CFGBlock>, block: crate::il::ILBlock) -> Self {
        Self {
            preds,
            succs,
            block,
        }
    }
    pub fn empty(block: ILBlock) -> Self {
        {
            Self {
                preds: vec![],
                succs: vec![],
                block,
            }
        }
    }
}
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CFGGraph {
    blocks: InternTable<CFGBlock, CFGBlockData>,
}

impl CFGGraph {
    pub fn new(blocks: InternTable<CFGBlock, CFGBlockData>) -> Self {
        Self { blocks }
    }
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn intern_block_data(&mut self, data: CFGBlockData) -> CFGBlock {
        self.blocks.get_or_intern(data)
    }
}

pub struct CFGGraphBuilder<'lmodule> {
    for_module: &'lmodule crate::il::ILModule,
    graph: CFGGraph,
}

impl<'module, 'func> CFGGraphBuilder<'module> {
    pub fn new(module: &'module crate::il::ILModule) -> Self {
        Self {
            for_module: module,
            graph: CFGGraph::empty(),
        }
    }

    fn populate_blocks(&mut self) {
        for func in self.for_module.functions().iter() {
            self.populate_blocks_for_func(func);
        }
    }

    pub fn walk_func(&mut self) {}

    fn populate_blocks_for_func(&mut self, func: &crate::il::ILFunctionData) {
        for (idx, _block) in self.for_module.iter_blocks(func) {
            let b = CFGBlockData::empty(idx);
            self.graph.intern_block_data(b);
        }
    }
}
