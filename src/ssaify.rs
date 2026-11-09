//! This module contains the code necessary to convert a non-SSA IR into an SSA one
//!
//! Accoring to QBE's spec, block labels are scoped to their function,
//! This means identical block labels can occur across different functions:
//! ```ignore
//! function $foo() {
//! @block1
//! @block2
//!   ret 2
//! }
//!
//! function $bar() {
//! @start
//! @block1
//!   %t = copy 3
//!   ret %t
//! }
//! ```
//!
//! in the above example, the label `@block1` occurs twice.
//!
//! We must somehow handle this in the CFG, as we cannot identify a block by its label alone,
//! we must instead also keep track of its enclosing function.
//!
//! The current (working on it) CFG implementation has its node-type be a `(func: ILFunction, label: StringID)`.

use idset::InternKey;
use log::trace;
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
pub enum CFGSymbolEvent {
    PhiDef(StringID),
    PhiUse(StringID),
    Def(StringID),
    Use(StringID),
    Call(StringID),
}

impl CFGSymbolEvent {
    const fn into_string_id(self) -> StringID {
        match self {
            CFGSymbolEvent::PhiDef(string_id)
            | CFGSymbolEvent::PhiUse(string_id)
            | CFGSymbolEvent::Call(string_id)
            | CFGSymbolEvent::Def(string_id)
            | CFGSymbolEvent::Use(string_id) => string_id,
        }
    }
}
impl DisplayModuleItem for CFGSymbolEvent {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let s = self.into_string_id().display_module_item(ctx);
        match self {
            CFGSymbolEvent::Def(_) => {
                format!("{s} <-")
            }
            CFGSymbolEvent::Use(_) => {
                format!("<- {s}")
            }
            CFGSymbolEvent::Call(_) => format!("<- ${s}()"),
            CFGSymbolEvent::PhiDef(_) => format!("{s} <- phi"),
            CFGSymbolEvent::PhiUse(_) => format!("<- phi {s}"),
        }
    }
}

/// Used to get an ordered list of all symbol definitions and usages
/// within a basic block
trait EmitCFGEvents {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>);
}
impl EmitCFGEvents for il::ILInstruction {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>) {
        match self {
            ILInstruction::Load(_, iloperand) => iloperand.emit_cfg_events(sink),
            ILInstruction::Binop(il::BinOp::Add, iloperand, iloperand1) => {
                iloperand.emit_cfg_events(sink);
                iloperand1.emit_cfg_events(sink);
            }
            ILInstruction::Copy(iloperand) => {
                iloperand.emit_cfg_events(sink);
            }
            ILInstruction::Cmp(_op, _t, a, b) => {
                a.emit_cfg_events(sink);
                b.emit_cfg_events(sink);
            }
            _ => {}
        }
    }
}

impl EmitCFGEvents for il::ILOperand {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>) {
        match self {
            il::ILOperand::Temp(iltemp) => sink.push(CFGSymbolEvent::Use(iltemp.name())),
            il::ILOperand::Global(string_id) => sink.push(CFGSymbolEvent::Use(*string_id)),
            _ => {}
        }
    }
}

impl EmitCFGEvents for il::ILBlockItem {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>) {
        match self {
            il::ILBlockItem::Call(ilcall) => {
                ilcall.emit_cfg_events(sink);
            }
            il::ILBlockItem::AssignInstr(iltemp, _iltype, ilinstruction) => {
                ilinstruction.emit_cfg_events(sink);
                sink.push(CFGSymbolEvent::Def(iltemp.name()));
            }
            il::ILBlockItem::Store(_iltype, iloperand, iloperand1) => {
                iloperand.emit_cfg_events(sink);
                iloperand1.emit_cfg_events(sink);
            }
        }
    }
}
impl EmitCFGEvents for il::ILBlockData {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>) {
        for (iltemp, _iltype, ilphi_node) in &self.phis {
            for (_origin, value) in &ilphi_node.incoming {
                match value {
                    il::ILOperand::Temp(iltemp) => sink.push(CFGSymbolEvent::PhiUse(iltemp.name())),
                    il::ILOperand::Global(string_id) => {
                        sink.push(CFGSymbolEvent::PhiUse(*string_id))
                    }
                    _ => {}
                }
            }
            sink.push(CFGSymbolEvent::Def(iltemp.name()));
        }
        for item in &self.items {
            item.emit_cfg_events(sink);
        }
        match self.terminator {
            ILTerminator::BranchIf(iloperand, ..) => iloperand.emit_cfg_events(sink),
            ILTerminator::ReturnVal(iloperand) => iloperand.emit_cfg_events(sink),
            _ => {}
        }
    }
}

impl EmitCFGEvents for il::ILCall {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>) {
        for (_iltype, iloperand) in &self.args {
            iloperand.emit_cfg_events(sink);
        }
        sink.push(CFGSymbolEvent::Call(self.func));
        if let Some((iltemp, _iltype)) = self.assignee {
            sink.push(CFGSymbolEvent::Def(iltemp.name()));
        }
    }
}
type CFGNodeType = (ILFunction, StringID);

/// A Control-Flow graph
/// that marks its edges a strictly-dominating edge
/// or as a join edge.
#[derive(Debug, Clone)]
pub struct DJControlFlowGraph {
    cfg: DiGraphMap<CFGNodeType, CFGEdgeKind>,
    join_nodes: HashSet<CFGNodeType>,
}

impl DJControlFlowGraph {
    pub fn empty() -> Self {
        Self {
            cfg: Default::default(),
            join_nodes: HashSet::new(),
        }
    }

    pub fn build(for_module: &ILModule) -> Option<(Self, DJGraph)> {
        let start = std::time::Instant::now();
        let mut s = CFGGraphBuilder::new(for_module).build();
        for (f, fdata) in for_module.functions().iter_kv() {
            for b in &fdata.blocks {
                s.build_dj_edges(f, b.label);
            }
        }
        let dj = DJGraph::new(&s, &for_module.ctx);
        let elapsed = start.elapsed();
        trace!(
            "DJ-graph construction took {} microseconds",
            elapsed.as_micros()
        );
        Some((s, dj))
    }

    pub fn clear(&mut self) {
        self.cfg.clear();
    }

    /// Perform `add_predecessor(dest,src)` and `add_successor(src,dest)`
    fn add_link(&mut self, func_id: ILFunction, src: StringID, dest: StringID) {
        self.add_node(func_id, src);
        self.add_node(func_id, dest);
        self.add_intra_functional_cfg_edge(func_id, src, dest);
    }

    /// introduce an edge into the graph within one function,
    /// calling this function multiple times for the same pair of nodes has no effect
    fn add_intra_functional_cfg_edge(
        &mut self,
        func_id: ILFunction,
        src: StringID,
        dest: StringID,
    ) {
        self.cfg
            .add_edge((func_id, src), (func_id, dest), CFGEdgeKind::Dominator);
    }

    // /// introduce an edge into the graph, spanning two functions, like a call-instruction
    // /// calling this function multiple times for the same pair of nodes has no effect
    // fn add_inter_functional_cfg_edge(
    //     &mut self,
    //     src_func: ILFunction,
    //     src: StringID,
    //     dest_func: ILFunction,
    //     dest: StringID,
    // ) {
    //     self.cfg
    //         .add_edge((src_func, src), (dest_func, dest), CFGEdgeKind::Dominator);
    // }

    /// introduce a node into the graph,
    /// calling this function multiple times for the same node
    /// has no effect
    fn add_node(&mut self, func_id: ILFunction, id: StringID) {
        self.cfg.add_node((func_id, id));
    }

    fn fmt_func_block_id_uninterned(
        ctx: &ILModuleContext,
        func_id: ILFunction,
        block: StringID,
    ) -> String {
        format!(
            "{}::{}",
            ctx.get_function(func_id).name.display_module_item(ctx),
            block.display_module_item(ctx)
        )
    }
    fn fmt_func_block_id_interned(func_id: ILFunction, block: StringID) -> String {
        format!("{}b{}", func_id.into_usize(), block.into_usize())
    }

    pub fn dump_dot(&self, ctx: &ILModuleContext, mark_join_edges: bool) -> String {
        let mut s = String::from("subgraph cluster_cfg {\nlabel=\"Controlflow graph\"\n");
        for (func, n) in self.cfg.nodes() {
            let fmt_block_events = self.fmt_block_events(func, n, ctx);
            s += &format!(
                "    cfg{} [ shape=rect,label = \"{}\n{}\"]\n",
                DJControlFlowGraph::fmt_func_block_id_interned(func, n),
                DJControlFlowGraph::fmt_func_block_id_uninterned(ctx, func, n),
                fmt_block_events
            );
        }
        for ((f_a, b_a), (f_b, b_b), dj) in self.cfg.all_edges() {
            s += &format!(
                "    cfg{} -> cfg{}",
                DJControlFlowGraph::fmt_func_block_id_interned(f_a, b_a),
                DJControlFlowGraph::fmt_func_block_id_interned(f_b, b_b)
            );
            if mark_join_edges && dj.is_join() {
                s += "[ label = \"j\"]\n";
            } else {
                s += "\n";
            }
        }
        s += "}";
        s
    }

    fn dominators(
        &self,
        func_id: ILFunction,
        root: StringID,
    ) -> petgraph::algo::dominators::Dominators<(ILFunction, StringID)> {
        debug_assert!(self.cfg.contains_node((func_id, root)));
        petgraph::algo::dominators::simple_fast(&self.cfg, (func_id, root))
    }

    fn build_dj_edges(&mut self, func_id: ILFunction, root: StringID) {
        let doms = self.dominators(func_id, root);
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

    fn fmt_block_events(&self, func_id: ILFunction, n: StringID, ctx: &ILModuleContext) -> String {
        let mut events = vec![];
        let b = ctx.get_block_by_label(func_id, n).unwrap();
        b.emit_cfg_events(&mut events);
        let mut s = String::new();
        for e in events {
            s += &format!("{}\n", e.display_module_item(ctx))
        }
        s
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
    pub fn build(mut self) -> DJControlFlowGraph {
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
        let blocks: Vec<_> = blocks.collect();

        for [(_, block), (_, next_block)] in blocks.array_windows() {
            self.cfgify_block(func_id, block, Some(next_block.label));
        }
        if let Some((_, last)) = blocks.last() {
            self.cfgify_block(func_id, last, None);
        }
    }

    fn cfgify_block(
        &mut self,
        func_id: ILFunction,
        block: &il::ILBlockData,
        next_block_label: Option<StringID>,
    ) {
        let cur_block = block.label;
        self.cfgify_block_items(func_id, cur_block, block);
        self.cfigify_terminator(func_id, cur_block, block, next_block_label);
    }

    fn cfgify_block_items(
        &mut self,
        func_id: ILFunction,
        cur_block: StringID,
        block: &ILBlockData,
    ) {
        self.graph.add_node(func_id, cur_block);
        for (_phi_def, _, phi) in block.phis.iter() {
            for (src_block, _) in phi.incoming.iter().copied() {
                self.graph
                    .add_intra_functional_cfg_edge(func_id, src_block, cur_block);
            }
        }

        // for item in block.items.iter() {
        //     match item {
        //         il::ILBlockItem::Call(_) => {
        //             // calls are shown as CFGSymbolEvents instead, as we cannot
        //             // determine the start block of a function that is not defined in this module
        //         }
        //         il::ILBlockItem::AssignInstr(.., instr) => {
        //             self.cfgify_assign_instr(cur_block, instr);
        //         }
        //         il::ILBlockItem::Store(_, val, addr) => {
        //             self.cfgify_usage(cur_block, val);
        //             self.cfgify_usage(cur_block, addr);
        //         }
        //     }
        // }
    }

    // fn cfgify_assign_instr(&mut self, cur_block: StringID, value: &ILInstruction) {
    //     match value {
    //         il::ILInstruction::Load(_, arg) => self.cfgify_usage(cur_block, arg),
    //         il::ILInstruction::Binop(_, a, b) => {
    //             self.cfgify_usage(cur_block, a);
    //             self.cfgify_usage(cur_block, b);
    //         }
    //         il::ILInstruction::Copy(arg) => {
    //             self.cfgify_usage(cur_block, arg);
    //         }
    //         il::ILInstruction::Cmp(_op, _t, a, b) => {
    //             self.cfgify_usage(cur_block, a);
    //             self.cfgify_usage(cur_block, b);
    //         }
    //         _ => {}
    //     };
    // }

    // fn cfgify_usage(&mut self, cur_block: StringID, iloperand: &il::ILOperand) {
    //     match iloperand {
    //         il::ILOperand::Temp(temp) => {}
    //         il::ILOperand::Global(glob) => {}
    //         _ => {}
    //     }
    // }

    fn cfigify_terminator(
        &mut self,
        func_id: ILFunction,
        cur_block: StringID,
        block: &ILBlockData,
        next_block_label: Option<StringID>,
    ) {
        match block.terminator {
            ILTerminator::Halt => {}
            ILTerminator::Jmp(dest) => {
                self.graph.add_link(func_id, cur_block, dest);
            }
            ILTerminator::BranchIf(_, btrue, bfalse) => {
                self.graph
                    .add_intra_functional_cfg_edge(func_id, cur_block, btrue);
                self.graph
                    .add_intra_functional_cfg_edge(func_id, cur_block, bfalse);
            }
            ILTerminator::FallThrough => {
                if let Some(next_block_label) = next_block_label {
                    self.graph
                        .add_intra_functional_cfg_edge(func_id, cur_block, next_block_label);
                }
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
    idom_edges: HashSet<(CFGNodeType, CFGNodeType)>,
    join_edges: HashSet<(CFGNodeType, CFGNodeType)>,
}

impl DJGraph {
    pub fn new(cfg: &DJControlFlowGraph, ctx: &ILModuleContext) -> Self {
        let mut new = Self {
            idom_edges: HashSet::new(),
            join_edges: HashSet::new(),
        };
        new.build_domtree(cfg, ctx);
        new.insert_join_edges(cfg);
        new
    }

    fn build_domtree(&mut self, cfg: &DJControlFlowGraph, ctx: &ILModuleContext) {
        for (ilfunction, ilfunction_data) in ctx.functions().iter_kv() {
            let doms = cfg.dominators(ilfunction, ilfunction_data.blocks[0].label);
            for n in cfg.cfg.nodes() {
                if let Some(idom) = doms.immediate_dominator(n) {
                    self.idom_edges.insert((idom, n));
                }
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
                DJControlFlowGraph::fmt_func_block_id_interned(n.0, n.1),
                DJControlFlowGraph::fmt_func_block_id_uninterned(ctx, n.0, n.1)
            );
        }
        for (a, b) in &self.idom_edges {
            s += &format!(
                "    dom{} -> dom{} [weight=100]\n",
                DJControlFlowGraph::fmt_func_block_id_interned(a.0, a.1),
                DJControlFlowGraph::fmt_func_block_id_interned(b.0, b.1),
            );
        }
        for (a, b) in &self.join_edges {
            s += &format!(
                "    dom{} -> dom{} [weight=1,constraint=false,style=dashed,arrowsize=0.5]\n",
                DJControlFlowGraph::fmt_func_block_id_interned(a.0, a.1),
                DJControlFlowGraph::fmt_func_block_id_interned(b.0, b.1),
            );
        }
        s += "}\n";

        s
    }

    fn all_nodes(&self) -> HashSet<CFGNodeType> {
        let mut all = HashSet::new();
        for (a, b) in self.idom_edges.iter().copied() {
            all.insert(a);
            all.insert(b);
        }
        all
    }
}
