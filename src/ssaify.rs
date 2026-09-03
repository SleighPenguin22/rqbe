//! This module contains the code necessary to convert a non-SSA IR into an SSA one

use std::hash::Hash;

use idset::{KeySet, internkey};

use crate::il::{ILBlock, ILBlockData, ILModule, ILTerminator};

internkey!(CFGBlock);
#[derive(Debug, Clone)]
pub struct CFGBlockData {
    preds: Vec<ILBlock>,
    succs: Vec<ILBlock>,
    block: crate::il::ILBlock,
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
    pub fn new(preds: Vec<ILBlock>, succs: Vec<ILBlock>, block: crate::il::ILBlock) -> Self {
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
    blocks: KeySet<CFGBlock, CFGBlockData>,
}

impl CFGGraph {
    pub fn new(blocks: KeySet<CFGBlock, CFGBlockData>) -> Self {
        Self { blocks }
    }
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn intern_cfgblock_data(&mut self, data: CFGBlockData) -> CFGBlock {
        self.blocks.get_or_intern(data)
    }
    fn il_to_cfg_id(&mut self, il_id: ILBlock) -> CFGBlock {
        let dummy = CFGBlockData::empty(il_id);
        self.blocks.get_or_intern(dummy)
    }
    fn il_to_cfg_mut(&mut self, il_id: ILBlock) -> &mut CFGBlockData {
        let dummy = CFGBlockData::empty(il_id);
        let id = self.blocks.get_or_intern(dummy);
        self.blocks.get_by_id_mut(id).unwrap()
    }
    pub fn add_predecessor(&mut self, ilblock_id: ILBlock, pred: ILBlock) {
        self.il_to_cfg_mut(ilblock_id).preds.push(pred);
    }
    pub fn add_successor(&mut self, ilblock_id: ILBlock, succ: ILBlock) {
        self.il_to_cfg_mut(ilblock_id).succs.push(succ);
    }
}

pub struct CFGGraphBuilder<'module> {
    for_module: &'module ILModule,
    graph: CFGGraph,
}

impl<'module> CFGGraphBuilder<'module> {
    pub fn new(module: &'module ILModule) -> Self {
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

    fn cfgify_block(
        &mut self,
        func: &crate::il::ILFunctionData,
        block_id: ILBlock,
        block: &crate::il::ILBlockData,
    ) -> CFGBlock {
        let mut cfgb = CFGBlockData::empty(block_id);
        match block.terminator {
            ILTerminator::Halt => todo!(),
            ILTerminator::Jmp(ilblock) => {
                self.graph.add_predecessor(ilblock, block_id);
            }
            ILTerminator::BranchIf(_ilassignee, btrue, bfalse) => {
                cfgb.succs.push(btrue);
                cfgb.succs.push(bfalse);
            }
            ILTerminator::Return => todo!(),
            ILTerminator::ReturnVal(ilassignee) => todo!(),
            ILTerminator::Unspecified => todo!(),
        };
        todo!()
    }

    fn populate_blocks_for_func(&mut self, func: &crate::il::ILFunctionData) {
        for (idx, block) in self.for_module.iter_blocks(func) {
            match block.terminator {
                ILTerminator::Halt => {}
                ILTerminator::Jmp(ilblock) => {}
                ILTerminator::BranchIf(ilassignee, ilblock, ilblock1) => todo!(),
                ILTerminator::Return => todo!(),
                ILTerminator::ReturnVal(ilassignee) => todo!(),
                ILTerminator::Unspecified => todo!(),
            }
        }
    }

    pub fn backfill_preds(&mut self) {}
}
