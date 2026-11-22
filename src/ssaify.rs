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
//! The current (working on it) CFG implementation has its node-type be a
//! `(func: ILFunction, label: StringID)`.
//!
//!
//! The CFG and Phi-insertion algorithm used is described in a paper by Cytron et al. called
//! "Efficiently Computing Static Single Assignment Form and the Control Dependence Graph" that I found at
//! [this link](https://www.cs.utexas.edu/~pingali/CS380C/2010/papers/ssaCytron.pdf)
//!
//! The placement of phi-nodes is scoped to functions, i.e. a Phi node's origin can never be another function's block.
//! As such, there is never a need to place phi nodes for jumps between functions, as the only way
//!  to jump between function is through calls, which start a new scope.
//!
//! The paper describes an algorithm to compute the dominance frontier of all the nodes in a CFG
//! by traversing its DomTree bottom-up (that is, depth-first postorder).
//!
//! This makes the complexity of the DF algorithm `O(E + N^2)`, with `E` and `N` being the number
//! of edges and nodes in the CFG, respectively.

use keyvec::InternKey;
use log::trace;
use petgraph::algo::dominators::Dominators;
use petgraph::graphmap::DiGraphMap;
use std::collections::{HashMap, VecDeque};
use std::{collections::HashSet, fmt::Debug};

use crate::StringID;
use crate::il::{
    self, ILBlockData, ILFunction, ILInstruction, ILModule, ILModuleContext, ILOperand, ILPhiNode,
    ILTemp, ILTerminator, ILType,
};
use crate::util::pretty_print::DisplayModuleItem;

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
pub struct ControlFlowGraph {
    cfg: DiGraphMap<CFGNodeType, ()>,
    dominance_frontiers: HashMap<CFGNodeType, HashSet<CFGNodeType>>,
}

// dominance frontier computations
impl ControlFlowGraph {
    /// Get the set of successors of a given node, along with their edge type
    fn get_successors(&self, cfg_node: CFGNodeType) -> impl Iterator<Item = CFGNodeType> {
        self.cfg
            .edges_directed(cfg_node, petgraph::Direction::Outgoing)
            .map(|(_, b, _e)| b)
    }

    /// This method computes the dominance frontier for all CFG nodes in a function.
    ///
    /// This runs in `O(E + N^2)`, with `E` and `N` being the amount of edges and nodes in the CFG.
    fn compute_dfs(&mut self, function_entry_block: CFGNodeType) {
        let domtree = self.domtree(function_entry_block);
        let bottomup = DomTreeDFSPostOrder::new(&domtree, self);
        for x in bottomup {
            // println!("visiting {:?}", Self::fmt_func_block_id_interned(x.0, x.1));
            self.compute_df(x, &domtree);
        }
    }
    /// compute the dominance frontier of a given node and cache it in `self.dominance_frontiers`.
    ///
    /// This method requires all of x's children to have their DF cached in `self.dominance_frontiers`
    /// This is guaranteed by [`self.compute_dfs`] visiting the nodes in DFS postorder.
    ///
    /// this is part of the algorithm described in the paper.
    #[inline]
    fn compute_df(&mut self, x: CFGNodeType, domtree: &Dominators<CFGNodeType>) {
        let mut dominance_frontier = HashSet::new();
        for y in self.get_successors(x) {
            // if x dominates a predeccessor of y (which could be x itself),
            // but does not strictly dominate y, then y in DF(x).
            if domtree.immediate_dominator(y) != Some(x) {
                dominance_frontier.insert(y);
            }
        }
        // x's children in the domtree
        for z in domtree.immediately_dominated_by(x) {
            debug_assert!(
                self.dominance_frontiers.contains_key(&z),
                "postorder invariant violated"
            );
            // each y in the dominance frontier of node z, which is a child of x
            for &y in &self.dominance_frontiers[&z] {
                if domtree.immediate_dominator(y) != Some(x) {
                    dominance_frontier.insert(y);
                }
            }
        }
        // cache the result
        self.dominance_frontiers.insert(x, dominance_frontier);
    }
    fn domtree(
        &self,
        root @ (function, entry_block): CFGNodeType,
    ) -> petgraph::algo::dominators::Dominators<(ILFunction, StringID)> {
        debug_assert!(self.cfg.contains_node((function, entry_block)));
        petgraph::algo::dominators::simple_fast(&self.cfg, root)
    }

    fn nodes(&self) -> petgraph::graphmap::Nodes<'_, CFGNodeType> {
        self.cfg.nodes()
    }
}
impl ControlFlowGraph {
    pub fn empty() -> Self {
        Self {
            cfg: Default::default(),
            dominance_frontiers: HashMap::new(),
        }
    }

    pub fn build(for_module: &ILModule) -> Option<Self> {
        let start = std::time::Instant::now();
        let mut s = CFGGraphBuilder::new(for_module).build();
        let cfg_elapsed = start.elapsed();
        for f in for_module.functions().keys() {
            s.compute_dfs((f, for_module.entry_block_of_func(f).label));
        }
        let dfs_elapsed = start.elapsed();
        trace!(
            "CFG-graph constructed in {} microseconds\nDF's computed in {} microseconds",
            cfg_elapsed.as_micros(),
            dfs_elapsed.as_micros(),
        );
        Some(s)
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
        self.cfg.add_edge((func_id, src), (func_id, dest), ());
    }

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
}
// DOT dumping
impl ControlFlowGraph {
    pub fn dump_cfg_dot(&self, ctx: &ILModuleContext) -> String {
        let mut s = String::from("subgraph cluster_cfg {\nlabel=\"Controlflow graph\"\n");
        for (func, n) in self.nodes() {
            let fmt_block_events = self.fmt_block_events(func, n, ctx);
            s += &format!(
                "    cfg{} [ shape=rect,label = \"{}\n{}\"]\n",
                ControlFlowGraph::fmt_func_block_id_interned(func, n),
                ControlFlowGraph::fmt_func_block_id_uninterned(ctx, func, n),
                fmt_block_events
            );
        }
        for ((f_a, b_a), (f_b, b_b), _) in self.cfg.all_edges() {
            s += &format!(
                "    cfg{} -> cfg{}\n",
                ControlFlowGraph::fmt_func_block_id_interned(f_a, b_a),
                ControlFlowGraph::fmt_func_block_id_interned(f_b, b_b)
            );
        }
        s += "}";
        s
    }
    pub fn dump_domtree_dot(&self, ctx: &ILModuleContext) -> String {
        let mut s = String::from("subgraph cluster_domtree {\n");
        DomTreeDOT::new(ctx, &self).dump_into(&mut s);
        s.push_str("}\n");
        s
    }

    fn collect_block_events(
        &self,
        func_id: ILFunction,
        n: StringID,
        ctx: &ILModuleContext,
    ) -> Vec<CFGSymbolEvent> {
        let mut events = vec![];
        let b = ctx.get_block_by_label(func_id, n).unwrap();
        b.emit_cfg_events(&mut events);
        events
    }

    fn fmt_block_events(&self, func_id: ILFunction, n: StringID, ctx: &ILModuleContext) -> String {
        let events = self.collect_block_events(func_id, n, ctx);
        let mut s = String::new();
        for e in events {
            s += &format!("{}\n", e.display_module_item(ctx))
        }
        s
    }
}

pub struct CFGGraphBuilder<'module> {
    graph: ControlFlowGraph,
    pub for_module: &'module ILModule,
}

impl<'m> CFGGraphBuilder<'m> {
    pub fn new(module: &'m ILModule) -> Self {
        Self {
            graph: ControlFlowGraph::empty(),
            for_module: module,
        }
    }
    pub fn build(mut self) -> ControlFlowGraph {
        self.populate_blocks();
        self.graph
    }

    fn populate_blocks(&mut self) {
        for (func_id, func) in self.for_module.functions().items() {
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
    }

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

struct DomTreeDFSPostOrder<'d> {
    domtree: &'d Dominators<CFGNodeType>,
    cfg: &'d ControlFlowGraph,
    visiting_queue: DomTreeVisitor,
}

/// This iterator wraps a Vec to provide a stack-like iterator
/// It guarantees for all nodes `n` in the DomTree,
/// that all of `n`'s children are returned from the iterator
/// before `n` itself is, aka postorder traversal.
struct DomTreeVisitor(Vec<CFGNodeType>);

impl DomTreeVisitor {
    fn push(&mut self, v: CFGNodeType) {
        self.0.push(v);
    }
}
impl Iterator for DomTreeVisitor {
    type Item = CFGNodeType;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop()
    }
    fn count(self) -> usize {
        self.0.len()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.0.len(), Some(self.0.len()))
    }
}
impl ExactSizeIterator for DomTreeVisitor {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'d> DomTreeDFSPostOrder<'d> {
    fn new(domtree: &'d Dominators<CFGNodeType>, cfg: &'d ControlFlowGraph) -> DomTreeVisitor {
        Self {
            domtree,
            cfg,
            visiting_queue: DomTreeVisitor(Vec::with_capacity(cfg.cfg.node_count())),
        }
        .iter()
    }

    fn iter(mut self) -> DomTreeVisitor {
        let mut dfs_stack = Vec::with_capacity(self.cfg.cfg.node_count());
        dfs_stack.push(self.domtree.root());

        // instead of push_front and then turning it into a Vec,
        // We could also define a little iterator wrapper that wraos a Vec and acts like a stack,
        // to save the `.into_iter().collect()` call
        self.visiting_queue.push(self.domtree.root());

        while let Some(visit) = dfs_stack.pop() {
            let ds = self.domtree.immediately_dominated_by(visit);
            for d in ds {
                self.visiting_queue.push(d);
                dfs_stack.push(d);
            }
        }
        self.visiting_queue
    }
}
struct DomTreeDOT<'a> {
    ctx: &'a ILModuleContext,
    cfg: &'a ControlFlowGraph,
}

impl<'a> DomTreeDOT<'a> {
    fn new(ctx: &'a ILModuleContext, cfg: &'a ControlFlowGraph) -> Self {
        Self { ctx, cfg }
    }

    fn dump_into(self, sink: &mut String) {
        // represent edges in the domtrees
        let mut dom_relation = Vec::new();

        for f in self.ctx.functions.keys() {
            let entry = self.ctx.entry_block_of_function(f).label;
            let domtree = &self.cfg.domtree((f, entry));
            self.dump_single_function_into(&mut dom_relation, domtree);
        }
        for (ilfunction, string_id) in self.cfg.nodes() {
            let n = ControlFlowGraph::fmt_func_block_id_interned(ilfunction, string_id);
            let label =
                ControlFlowGraph::fmt_func_block_id_uninterned(self.ctx, ilfunction, string_id);
            sink.push_str(&format!("dom{n} [label=\"{label}\"]\n"));
        }
        for (dom, n) in dom_relation {
            let dom = ControlFlowGraph::fmt_func_block_id_interned(dom.0, dom.1);
            let n = ControlFlowGraph::fmt_func_block_id_interned(n.0, n.1);
            sink.push_str(&format!("dom{dom} -> dom{n}\n"));
        }
    }

    fn dump_single_function_into(
        &self,
        dom_relation: &mut Vec<((ILFunction, StringID), (ILFunction, StringID))>,
        domtree: &Dominators<(ILFunction, StringID)>,
    ) {
        // use a bottom up to guarantee each node gets
        // visited exactly once
        let bottomup = DomTreeDFSPostOrder::new(domtree, self.cfg);
        for n in bottomup {
            if let Some(dom) = domtree.immediate_dominator(n) {
                dom_relation.push((dom, n));
            }
        }
    }
}

/// This struct has methods that insert phi-functions into a given module.
///
///
struct PhiNodeInsertion {
    cfg: ControlFlowGraph,
    /// from the paper:
    ///
    /// an array of flags, one for each node, where has_already(x) indicates whether a phi-function
    /// for V has already been inserted at x
    has_already: HashMap<CFGNodeType, usize>,
    /// from the paper:
    ///
    /// an array of flags, one flag for each node, where Work(X) indicates whether X has ever
    /// been added to W during the current iteration of the outer loop.
    node_work: HashMap<CFGNodeType, usize>,
}

impl PhiNodeInsertion {
    fn new(cfg: ControlFlowGraph) -> Self {
        Self {
            cfg,
            has_already: HashMap::new(),
            node_work: HashMap::new(),
        }
    }
    /// from the paper:
    ///
    /// The flags Work(X) and HasAlready(X) are independent.
    /// We need two flags because the property of assigning to V is independent of the property of i
    /// needing a d-function for V.
    /// The flags could have been implemented with just the values true and false,
    /// but this would require additional record keeping to reset any true flags between iterations,
    /// without the expense of looping over all the nodes.
    /// It is simpler to devote an integer to each flag and to test
    /// flags by comparing them with the current iteration count.
    fn insert_phis(&mut self, module: &mut ILModule, function: ILFunction) {
        let typ_phifix = module.ctx.get_type_PhiFix();

        let mut itercount = 0;
        for node in self.cfg.nodes() {
            self.has_already.insert(node, 0);
            self.node_work.insert(node, 0);
        }

        // (b, v) means block b assigns to variable v
        let mut assigns = HashSet::new();
        let mut all_vars = HashSet::new();
        self.fill_relations(module, function, &mut assigns, &mut all_vars);

        // from the paper:
        //
        // the worklist of CFG nodes being processed.
        // In each iteration of this algorithm, W is initialized to the set &(V) of nodes that
        // contain assignments to V. Each node X in the worklist ensures that each node Y in DF( X)
        // receives a phi-function. Each iteration terminates when the worklist becomes empty.
        let mut worklist = VecDeque::new();

        for &var in &all_vars {
            let var_assigns = assigns.iter().filter(|(_node, v, _typ)| *v == var);
            let ILTemp::NonSSA(var) = var else {
                todo!("should we allow partially SSAified code?");
            };
            itercount += 1;
            for (node, _v, _typ) in var_assigns {
                self.node_work.insert(*node, itercount);
                if !worklist.contains(node) {
                    worklist.push_back(*node);
                }
            }
            while let Some(x) = worklist.pop_front() {
                for y in &self.cfg.dominance_frontiers[&x] {
                    if self.has_already[y] < itercount {
                        let block = module.get_block_mut(*y);
                        push_var_into_phinode(typ_phifix, var, y, block);
                        self.has_already.insert(*y, itercount);
                        if self.node_work[y] < itercount {
                            self.node_work.insert(*y, itercount);
                            if !worklist.contains(&y) {
                                worklist.push_back(*y);
                            }
                        }
                    }
                }
            }
        }
    }

    fn fill_relations(
        &self,
        module: &mut ILModule,
        function: ILFunction,
        assigns: &mut HashSet<(CFGNodeType, ILTemp, ILType)>,
        all_vars: &mut HashSet<ILTemp>,
    ) {
        let blocks = &module.get_function(function).blocks;
        for block in blocks {
            let node = (function, block.label);
            for item in &block.items {
                match item {
                    il::ILBlockItem::Call(il::ILCall {
                        assignee: Some((var, typ)),
                        args,
                        ..
                    }) => {
                        assigns.insert((node, *var, *typ));
                        all_vars.insert(*var);
                        for (_, arg) in args {
                            self.add_temp_operand_to_all_vars(all_vars, arg);
                        }
                    }
                    il::ILBlockItem::AssignInstr(var, typ, instr) => {
                        assigns.insert((node, *var, *typ));
                        match instr {
                            ILInstruction::Load(_, operand)
                            | ILInstruction::Copy(operand)
                            | ILInstruction::Unop(_, operand) => {
                                self.add_temp_operand_to_all_vars(all_vars, operand);
                            }
                            ILInstruction::Cmp(_, _, a, b) | ILInstruction::Binop(_, a, b) => {
                                self.add_temp_operand_to_all_vars(all_vars, a);
                                self.add_temp_operand_to_all_vars(all_vars, b);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            match block.terminator {
                ILTerminator::BranchIf(operand, ..) | ILTerminator::ReturnVal(operand) => {
                    self.add_temp_operand_to_all_vars(all_vars, &operand);
                }
                _ => {}
            }
        }
    }
    fn add_temp_operand_to_all_vars(&self, all_vars: &mut HashSet<ILTemp>, operand: &ILOperand) {
        if let ILOperand::Temp(t) = operand {
            all_vars.insert(*t);
        }
    }
}

fn push_var_into_phinode(
    typ_phifix: ILType,
    var: StringID,
    y: &(ILFunction, StringID),
    block: &mut ILBlockData,
) {
    let mut phi_idx = None;
    for phimut in block.phis.iter_mut() {
        let phi_assign = phimut.0;
        if phi_assign.name() == var {
            phi_idx = Some(phimut);
        }
    }
    let (_, _, ilphi_node) = match phi_idx {
        Some(idx) => idx,
        None => {
            let new_phi = (
                ILTemp::NonSSA(var),
                typ_phifix,
                ILPhiNode { incoming: vec![] },
            );
            block.phis.push_mut(new_phi)
        }
    };
    ilphi_node
        .incoming
        .push((y.1, ILOperand::Temp(ILTemp::NonSSA(var))));
}
