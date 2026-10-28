//! This module contains the code necessary to convert a non-SSA IR into an SSA one
//!
//! The ultimate goal is to implement one of the famous algorithms from the paper,
//! But i first want to hack and try a proposed algorithm myself:
//!
//! We walk all bindings, and convert their bound name to a unique number (simply using a counter),
//! still keeping track of the original binding's name.
//!
//! Then, we walk the program again in the program's control flow,
//! and for each usage of a binding, keep track what variables a scope binds:
//! a map M : scope -> list(var_n) a lexical scope:
//! each variable var_n: (defining_block, bound_name)
//!
//! then we process each usage of a temporary:
//! - When we encounter a usage that is not defined:
//!     emit an error (usage of undefined binding)
//! - When we encounter a binding to a symbol that is defined exactly once:
//!     this is a correct def-use chain:
//!     We replace the name with the most recently bound `var_n`
//! - When we encounter a binding to a symbol that is in scope more than once,
//!   and each occurence is from a different predecessor
//!   (i.e. blocks that flow from a common ancestor, and each define the binding):
//!     Insert a Phi function, with each of the predecessors as argument
//!     We must choose which bindings are applicable as the Phi's arguments,
//!     I belive this is done by walking the CFG backwards from the usage, and keeping track
//!     of where a usage stops being unique.
//!     I believe this is exactly what a dominator tree is for,

use idset::InternKey;
use petgraph::graphmap::DiGraphMap;
use std::{collections::HashSet, fmt::Debug};

use crate::StringID;
use crate::il::{
    self, ILBlockData, ILFunction, ILInstruction, ILModule, ILModuleContext, ILTerminator,
};
use crate::util::pretty_print::DisplayModuleItem;

/// The kind of DJ-node that a CFG-edge is
/// For any edge in the CFG,
/// it is either a dominator, or a join edge,
/// It cannot be both.
#[derive(Clone, Copy, Debug)]
pub enum CFGEdgeKind {
    Dominator,
    Join,
}

impl CFGEdgeKind {
    /// Returns `true` if the djedge kind is [`Join`].
    ///
    /// [`Join`]: DJEdgeKind::Join
    #[must_use]
    fn is_join(&self) -> bool {
        matches!(self, Self::Join)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CFGNodeKind {
    Def(StringID),
    Use(StringID),
}

impl CFGNodeKind {
    fn into_usize(self) -> usize {
        self.into_string_id().into_usize()
    }
    const fn into_string_id(self) -> StringID {
        match self {
            CFGNodeKind::Def(string_id) | CFGNodeKind::Use(string_id) => string_id,
        }
    }
}
impl DisplayModuleItem for CFGNodeKind {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let s = self.into_string_id().display_module_item(ctx);
        match self {
            CFGNodeKind::Def(_) => {
                format!("{s} <-")
            }
            CFGNodeKind::Use(_) => {
                format!("<- {s}")
            }
        }
    }
}

/// A Control-Flow graph
/// that marks its edges a strictly-dominating edge
/// or as a join edge.
#[derive(Debug, Clone)]
pub struct DJControlFlowGraph {
    cfg: DiGraphMap<StringID, CFGEdgeKind>,
    join_nodes: HashSet<StringID>,
}

impl DJControlFlowGraph {
    pub fn empty() -> Self {
        Self {
            cfg: Default::default(),
            join_nodes: HashSet::new(),
        }
    }

    pub fn build(for_module: &ILModule, start_function: ILFunction) -> (Self, DJGraph) {
        let start_node = crate::intern_str("_START");
        let mut s = CFGGraphBuilder::new(for_module).build(start_node);
        let id = for_module.ctx.functions.get(start_function).unwrap().blocks[0].label;
        s.build_dj_edges(id);
        let dj = DJGraph::new(&s, id);
        (s, dj)
    }

    pub fn clear(&mut self) {
        self.cfg.clear();
    }

    /// Perform `add_predecessor(dest,src)` and `add_successor(src,dest)`
    fn add_link(&mut self, src: StringID, dest: StringID) {
        self.add_node(src);
        self.add_node(dest);
        self.add_cfg_edge(src, dest);
    }

    /// introduce an edge into the graph,
    /// calling this function multiple times for the same pair of nodes has no effect
    fn add_cfg_edge(&mut self, src: StringID, dest: StringID) {
        self.cfg.add_edge(src, dest, CFGEdgeKind::Dominator);
    }

    /// introduce a node into the graph,
    /// calling this function multiple times for the same node
    /// has no effect
    fn add_node(&mut self, id: StringID) {
        self.cfg.add_node(id);
    }

    pub fn dump_dot(&self, ctx: &ILModuleContext, mark_join_edges: bool) -> String {
        let mut s = String::from("subgraph cluster_cfg {\nlabel=\"Controlflow graph\"\n");
        for n in self.cfg.nodes() {
            s += &format!(
                "    cfg{} [ label = \"{}\n\"]\n",
                n.into_usize(),
                n.display_module_item(ctx),
            );
        }
        for (a, b, dj) in self.cfg.all_edges() {
            s += &format!("    cfg{} -> cfg{}", a.into_usize(), b.into_usize());
            if mark_join_edges && dj.is_join() {
                s += "[ label = \"j\"]\n";
            } else {
                s += "\n";
            }
        }
        s += "}";
        s
    }

    fn dominators(&self, root: StringID) -> petgraph::algo::dominators::Dominators<StringID> {
        debug_assert!(self.cfg.contains_node(root));
        petgraph::algo::dominators::simple_fast(&self.cfg, root)
    }

    fn build_dj_edges(&mut self, root: StringID) {
        let doms = self.dominators(root);
        for (x, y, dj) in self.cfg.all_edges_mut() {
            if let Some(sdoms) = doms.strict_dominators(y) {
                let sdoms = sdoms.collect::<Vec<_>>();
                if sdoms.contains(&x) {
                    *dj = CFGEdgeKind::Dominator;
                } else {
                    *dj = CFGEdgeKind::Join;
                    self.join_nodes.insert(y);
                }
            }
        }
    }
}

pub struct CFGGraphBuilder<'module> {
    graph: DJControlFlowGraph,
    pub for_module: &'module ILModule,
}

impl<'m> CFGGraphBuilder<'m> {
    pub fn new(module: &'m ILModule) -> Self {
        Self {
            graph: DJControlFlowGraph::empty(),
            for_module: module,
        }
    }
    pub fn build(mut self, start_block: crate::StringID) -> DJControlFlowGraph {
        self.populate_blocks();
        self.graph
    }

    fn populate_blocks(&mut self) {
        for (func_id, func) in self.for_module.functions().iter_kv() {
            self.populate_blocks_for_func(func_id, func);
        }
    }
    fn populate_blocks_for_func(&mut self, func_id: ILFunction, func: &il::ILFunctionData) {
        let blocks = self.for_module.iter_blocks(func_id, func);
        for (_idx, block) in blocks {
            self.cfgify_block(block);
        }
    }

    fn cfgify_block(&mut self, block: &il::ILBlockData) {
        let cur_block = block.label;
        self.cfgify_block_items(cur_block, block);
        self.cfigify_terminator(cur_block, block);
    }

    fn cfgify_block_items(&mut self, cur_block: StringID, block: &ILBlockData) {
        self.graph.add_node(cur_block);
        for (phi_def, _, phi_id) in block.phis.iter().copied() {
            let phi = self.for_module.get_phi_node(phi_id);

            // self.advance_prev_node(prev_node, CFGNodeKind::Def(phi_def.name()));

            for (src_block, _) in phi.incoming.iter().copied() {
                self.graph.add_link(src_block, phi_def.name());
            }
        }

        for item in block.items.iter() {
            match item {
                il::ILBlockItem::Call(ilcall) => {
                    let func = self
                        .for_module
                        .ctx
                        .get_function_by_name(ilcall.func)
                        .unwrap();
                    self.graph.add_link(cur_block, func.blocks[0].label);
                }
                il::ILBlockItem::AssignInstr(assignee, _, instr) => {
                    self.cfgify_assign_instr(cur_block, assignee, instr);
                }
                il::ILBlockItem::Store(_, val, addr) => {
                    self.cfgify_usage(cur_block, val);
                    self.cfgify_usage(cur_block, addr);
                }
            }
        }
    }

    fn cfgify_assign_instr(
        &mut self,
        cur_block: StringID,
        assignee: &il::ILTemp,
        value: &ILInstruction,
    ) {
        match value {
            il::ILInstruction::Load(arg) => self.cfgify_usage(cur_block, arg),
            il::ILInstruction::Add(a, b) => {
                self.cfgify_usage(cur_block, a);
                self.cfgify_usage(cur_block, b);
            }
            il::ILInstruction::Copy(arg) => {
                self.cfgify_usage(cur_block, arg);
            }
            il::ILInstruction::CmpZ(arg) => {
                self.cfgify_usage(cur_block, arg);
            }
            _ => {}
        };
    }

    fn cfgify_usage(&mut self, cur_block: StringID, iloperand: &il::ILOperand) {
        match iloperand {
            il::ILOperand::Temp(temp) => {}
            il::ILOperand::Global(glob) => {}
            _ => {}
        }
    }

    fn cfigify_terminator(&mut self, cur_block: StringID, block: &ILBlockData) {
        match block.terminator {
            ILTerminator::Halt => {}
            ILTerminator::Jmp(dest) => {
                self.graph.add_link(cur_block, dest);
            }
            ILTerminator::BranchIf(val, btrue, bfalse) => {
                self.cfgify_usage(cur_block, &val);
                self.graph.add_link(cur_block, btrue);
                self.graph.add_link(cur_block, bfalse);
            }
            _ => {}
        }
    }
}

/// The dominator tree structure holding all immediate dominators (D-edges)
/// and join-edges (J-edges) forming a DJ-graph
///
/// This graph is used to compute dominance frontiers and to decide placement
/// of PHI-nodes
pub struct DJGraph {
    /// (idom, n) means idom immedately dominates n
    idom_edges: HashSet<(StringID, StringID)>,
    join_edges: HashSet<(StringID, StringID)>,
    root: StringID,
}

impl DJGraph {
    pub fn new(cfg: &DJControlFlowGraph, root: StringID) -> Self {
        let mut new = Self {
            idom_edges: HashSet::new(),
            join_edges: HashSet::new(),
            root,
        };
        new.build_domtree(cfg);
        new.insert_join_edges(cfg);
        new
    }

    fn build_domtree(&mut self, cfg: &DJControlFlowGraph) {
        let doms = cfg.dominators(self.root);
        for n in cfg.cfg.nodes() {
            if let Some(idom) = doms.immediate_dominator(n) {
                self.idom_edges.insert((idom, n));
            }
        }
    }

    fn insert_join_edges(&mut self, cfg: &DJControlFlowGraph) {
        for (x, y, dj) in cfg.cfg.all_edges() {
            if cfg.join_nodes.contains(&y) && dj.is_join() {
                self.join_edges.insert((x, y));
            }
        }
    }
    pub fn dump_dot(&self, ctx: &ILModuleContext) -> String {
        let all = self.all_nodes();
        let mut s = String::from("subgraph cluster_dom {\nlabel=\"DJ-graph\"\n");

        for n in all {
            s += &format!(
                "    dom{} [ label = \"{}\"]\n",
                n.into_usize(),
                n.display_module_item(ctx)
            );
        }
        for (a, b) in &self.idom_edges {
            s += &format!(
                "    dom{} -> dom{} [weight=100]\n",
                a.into_usize(),
                b.into_usize()
            );
        }
        for (a, b) in &self.join_edges {
            s += &format!(
                "    dom{} -> dom{} [weight=1,constraint=false,style=dashed,arrowsize=0.5]\n",
                a.into_usize(),
                b.into_usize()
            );
        }
        s += "}\n";

        s
    }

    fn all_nodes(&self) -> HashSet<StringID> {
        let mut all = HashSet::new();
        for (a, b) in self.idom_edges.iter().copied() {
            all.insert(a);
            all.insert(b);
        }
        all
    }
}
