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
use petgraph::graphmap::{DiGraphMap, GraphMap};
use std::collections::{HashMap, VecDeque};
use std::{collections::HashSet, fmt::Debug};

use crate::StringKey;
use crate::il::{
    self, ILBlockData, ILFunction, ILInstruction, ILModule, ILModuleContext, ILOperand, ILPhiNode,
    ILTemp, ILTerminator, ILType,
};
use crate::util::pretty_print::DisplayModuleItem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CFGSymbolEvent {
    /// A temp whose value is a phi-node: `%t <- phi(...)`
    PhiDef(StringKey),
    /// A temp-operand to a phi function `... <- phi(@b %t, ...)`
    PhiUseLocal(StringKey),
    /// Definition of a temp: `$t <- ...`
    DefTemp(StringKey),
    /// Usage of a temporary: `... <- %t`
    UseTemp(StringKey),
    /// Usage of a global: `... <- $g`
    UseGlobal(StringKey),
    /// A call to a function `... <- $f(...)`
    Call(StringKey),
    PhiUseGlobal(StringKey),
}

impl CFGSymbolEvent {
    const fn into_string_key(self) -> StringKey {
        match self {
            CFGSymbolEvent::PhiDef(string_key)
            | CFGSymbolEvent::PhiUseLocal(string_key)
            | CFGSymbolEvent::Call(string_key)
            | CFGSymbolEvent::DefTemp(string_key)
            | CFGSymbolEvent::UseGlobal(string_key)
            | CFGSymbolEvent::PhiUseGlobal(string_key)
            | CFGSymbolEvent::UseTemp(string_key) => string_key,
        }
    }
}
impl DisplayModuleItem for CFGSymbolEvent {
    fn display_module_item(&self, ctx: &ILModuleContext) -> String {
        let s = self.into_string_key().unintern();
        match self {
            CFGSymbolEvent::DefTemp(_) => {
                format!("%{s} <-")
            }
            CFGSymbolEvent::UseTemp(_) => {
                format!("<- %{s}")
            }
            CFGSymbolEvent::Call(_) => format!("<- ${s}()"),
            CFGSymbolEvent::PhiDef(_) => format!("%{s} <- phi"),
            CFGSymbolEvent::PhiUseLocal(_) => format!("<- phi %{s}"),
            CFGSymbolEvent::PhiUseGlobal(_) => format!("<- phi ${s}"),
            CFGSymbolEvent::UseGlobal(_) => format!("<- ${s}"),
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
            il::ILOperand::Temp(temp) => sink.push(CFGSymbolEvent::UseTemp(temp.name())),
            il::ILOperand::Global(string_key) => sink.push(CFGSymbolEvent::UseGlobal(*string_key)),
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
                sink.push(CFGSymbolEvent::DefTemp(iltemp.name()));
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
        for ILPhiNode {
            assignee,
            typ: _,
            incoming,
        } in &self.phis
        {
            for (_origin, value) in incoming {
                match value {
                    il::ILOperand::Temp(iltemp) => {
                        sink.push(CFGSymbolEvent::PhiUseLocal(iltemp.name()))
                    }
                    il::ILOperand::Global(string_id) => {
                        sink.push(CFGSymbolEvent::PhiUseGlobal(*string_id))
                    }
                    _ => {}
                }
            }
            sink.push(CFGSymbolEvent::DefTemp(assignee.name()));
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
            sink.push(CFGSymbolEvent::DefTemp(iltemp.name()));
        }
    }
}
type CFGNodeType = (ILFunction, StringKey);

/// A Control-Flow graph
/// that marks its edges a strictly-dominating edge
/// or as a join edge.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    cfg: DiGraphMap<CFGNodeType, ()>,
    dominance_frontiers: HashMap<CFGNodeType, HashSet<CFGNodeType>>,
    usages: HashMap<CFGNodeType, Vec<CFGSymbolEvent>>,
}

// dominance frontier computations
impl ControlFlowGraph {
    /// Get the set of successors of a given node, along with their edge type
    fn get_successors(&self, cfg_node: CFGNodeType) -> impl Iterator<Item = CFGNodeType> {
        self.cfg
            .edges_directed(cfg_node, petgraph::Direction::Outgoing)
            .map(|(_, b, _e)| b)
    }
    fn get_predecessors(&self, node: CFGNodeType) -> impl Iterator<Item = CFGNodeType> + '_ {
        self.cfg
            .edges_directed(node, petgraph::Direction::Incoming)
            .map(|(src, _, _)| src)
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
    ) -> petgraph::algo::dominators::Dominators<(ILFunction, StringKey)> {
        debug_assert!(self.cfg.contains_node((function, entry_block)));
        petgraph::algo::dominators::simple_fast::<&DiGraphMap<CFGNodeType, ()>>(&self.cfg, root)
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
            usages: HashMap::new(),
        }
    }

    pub fn build(for_module: &ILModule) -> Self {
        let start = std::time::Instant::now();
        let mut s = CFGGraphBuilder::new(for_module).construct_cfg();
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
        s
    }

    pub fn clear(&mut self) {
        self.cfg.clear();
    }

    /// Perform `add_predecessor(dest,src)` and `add_successor(src,dest)`
    fn add_link(&mut self, func_id: ILFunction, src: StringKey, dest: StringKey) {
        self.add_node(func_id, src);
        self.add_node(func_id, dest);
        self.add_intra_functional_cfg_edge(func_id, src, dest);
    }

    /// introduce an edge into the graph within one function,
    /// calling this function multiple times for the same pair of nodes has no effect
    fn add_intra_functional_cfg_edge(
        &mut self,
        func_id: ILFunction,
        src: StringKey,
        dest: StringKey,
    ) {
        self.cfg.add_edge((func_id, src), (func_id, dest), ());
    }

    /// introduce a node into the graph,
    /// calling this function multiple times for the same node
    /// has no effect
    fn add_node(&mut self, func_id: ILFunction, id: StringKey) {
        self.cfg.add_node((func_id, id));
    }

    fn fmt_func_block_id_uninterned(
        ctx: &ILModuleContext,
        func_id: ILFunction,
        block: StringKey,
    ) -> String {
        format!(
            "{}::{}",
            ctx.get_function(func_id).name.unintern(),
            block.display_module_item(ctx)
        )
    }
    fn fmt_func_block_id_interned(func_id: ILFunction, block: StringKey) -> String {
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

    fn fmt_block_events(&self, func_id: ILFunction, n: StringKey, ctx: &ILModuleContext) -> String {
        let events = &self.usages[&(func_id, n)];
        let mut s = String::new();
        for e in events {
            s += &format!("{}\n", e.display_module_item(ctx))
        }
        s
    }
}

struct CFGGraphBuilder<'module> {
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
    pub fn construct_cfg(mut self) -> ControlFlowGraph {
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
        next_block_label: Option<StringKey>,
    ) {
        let mut events = Vec::with_capacity(block.items.len() + block.phis.len());
        block.emit_cfg_events(&mut events);
        let cur_block = block.label;
        self.graph.usages.insert((func_id, cur_block), events);
        self.cfgify_block_items(func_id, cur_block, block);
        self.cfigify_terminator(func_id, cur_block, block, next_block_label);
    }

    fn cfgify_block_items(
        &mut self,
        func_id: ILFunction,
        cur_block: StringKey,
        block: &ILBlockData,
    ) {
        self.graph.add_node(func_id, cur_block);
        for p in block.phis.iter() {
            for (src_block, _) in p.incoming.iter().copied() {
                self.graph
                    .add_intra_functional_cfg_edge(func_id, src_block, cur_block);
            }
        }
    }

    fn cfigify_terminator(
        &mut self,
        func_id: ILFunction,
        cur_block: StringKey,
        block: &ILBlockData,
        next_block_label: Option<StringKey>,
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
        dom_relation: &mut Vec<((ILFunction, StringKey), (ILFunction, StringKey))>,
        domtree: &Dominators<(ILFunction, StringKey)>,
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
pub struct PhiNodeInsertion {
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
    pub fn new(cfg: ControlFlowGraph) -> Self {
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
    pub fn insert_phis(&mut self, module: &mut ILModule, function: ILFunction) {
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
        println!(
            "assigns: {:?}",
            assigns
                .iter()
                .map(|(a, b, _c)| (
                    ControlFlowGraph::fmt_func_block_id_uninterned(&module.ctx, a.0, a.1),
                    b.name().unintern()
                ))
                .collect::<Vec<_>>()
        );
        println!(
            "all vars: {:?}",
            all_vars
                .iter()
                .map(|v| v.display_module_item(&module.ctx))
                .collect::<Vec<_>>()
        );
        // from the paper:
        //
        // the worklist of CFG nodes being processed.
        // In each iteration of this algorithm, W is initialized to the set &(V) of nodes that
        // contain assignments to V. Each node X in the worklist ensures that each node Y in DF( X)
        // receives a phi-function. Each iteration terminates when the worklist becomes empty.
        let mut worklist = VecDeque::new();

        for &var in &all_vars {
            println!("working on {}", var.display_module_item(&module.ctx));
            let blocks_assigning_to_var = assigns.iter().filter(|(_node, v, _typ)| *v == var);
            let ILTemp::NonSSA(var) = var else {
                todo!("should we allow partially SSAified code?");
            };
            itercount += 1;
            for (node, _v, _typ) in blocks_assigning_to_var {
                self.node_work.insert(*node, itercount);
                if !worklist.contains(node) {
                    worklist.push_back(*node);
                }
            }
            while let Some(x) = worklist.pop_front() {
                println!(
                    "working on {}",
                    ControlFlowGraph::fmt_func_block_id_uninterned(&module.ctx, x.0, x.1)
                );
                for y in &self.cfg.dominance_frontiers[&x] {
                    println!(
                        "y = {} in DF({})",
                        ControlFlowGraph::fmt_func_block_id_uninterned(&module.ctx, y.0, y.1),
                        ControlFlowGraph::fmt_func_block_id_uninterned(&module.ctx, x.0, x.1)
                    );
                    if self.has_already[y] < itercount {
                        let block = module.get_block_by_cfg_node_mut(*y);
                        println!("pushing into phi...");
                        let preds = self.cfg.get_predecessors(*y);
                        push_var_into_phinode(typ_phifix, var, preds, block);
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
            println!("done working on {}", var.display_module_item(&module.ctx));
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
    var: StringKey,
    predecessors: impl Iterator<Item = (ILFunction, StringKey)>,
    block: &mut ILBlockData,
) {
    let mut phi_idx = None;
    for phimut in block.phis.iter_mut() {
        let phi_assign = phimut.assignee;
        if phi_assign.name() == var {
            phi_idx = Some(phimut);
        }
    }
    let p = match phi_idx {
        Some(idx) => idx,
        None => {
            let new_phi = ILPhiNode {
                assignee: ILTemp::NonSSA(var),
                typ: typ_phifix,
                incoming: vec![],
            };
            block.phis.push_mut(new_phi)
        }
    };
    for pred in predecessors {
        p.incoming
            .push((pred.1, ILOperand::Temp(ILTemp::NonSSA(var))));
    }
    println!(
        "{} = {:?}",
        block.label.unintern(),
        p.incoming
            .iter()
            .map(|(a, b)| (a.unintern(), format!("{b:?}")))
            .collect::<Vec<_>>()
    );
}
#[cfg(test)]
mod tests {

    use crate::builder_dsl;

    use super::*;

    #[test]
    fn simple_branch_phi() {
        let mut build = crate::il::builder::ILModuleBuilder::start();
        let type_i8_interned = build.get_type_I8();
        let type_i8 = il::ILTypeData::I8;
        let builder = &mut build;

        builder_dsl!(func builder;
        function foo(type_i8_interned) -> type_i8_interned {
            |start_block| @start
            |cond| cond =type_i8 (1);
            !(jmpz cond, @bt, @bf)

            |bt_block| @bt
            |bt_a| a =type_i8 (3);
            !(jmp @merge)

            |bf_block| @bf
            |bf_a| a =type_i8 (3);
            !(jmp @merge)

            |merge_block| @merge
            |merge_a| a =type_i8 (a);
            !(ret a)
        }

        );

        let mut m = build.finish();

        let cfg = ControlFlowGraph::build(&m);
        println!("|DF| = {}", cfg.dominance_frontiers.len());
        for (a, b) in &cfg.dominance_frontiers {
            println!(
                "DF({}) := {:?}",
                a.1.unintern(),
                b.iter()
                    .map(|e| ControlFlowGraph::fmt_func_block_id_uninterned(&m.ctx, e.0, e.1))
                    .collect::<HashSet<_>>()
            );
        }
        println!("\nbefore phi insertion:\n{}", m.display_module());
        PhiNodeInsertion::new(cfg).insert_phis(&mut m, foo);
        println!("\nafter phi-insertion:\n{}", m.display_module());
        assert_eq!(m.get_block(merge_block).phis[0].incoming.len(), 2);
    }
}
