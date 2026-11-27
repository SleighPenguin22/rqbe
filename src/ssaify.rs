//! This module contains the code necessary to convert a non-SSA IR into an SSA
//! one
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
//! We must somehow handle this in the CFG, as we cannot identify a block by its label
//! alone, we must instead also keep track of its enclosing function.
//!
//! The current CFG implementation has its node-type be a `(func: ILFunction, label:
//! StringKey)`.
//!
//!
//! The CFG and Phi-insertion algorithm used is described in a paper by Cytron et al.
//! called "Efficiently Computing Static Single Assignment Form and the Control Dependence
//! Graph" that I found at [this link](https://www.cs.utexas.edu/~pingali/CS380C/2010/papers/ssaCytron.pdf)
//!
//! The placement of phi-nodes is scoped to functions, i.e. a Phi node's origin can never
//! be a label in another function's. As such, there is never a need to place phi nodes
//! for jumps between functions, as the only way to jump between function is through
//! calls, which start a new scope.
//!
//! The paper describes an algorithm to compute the dominance frontier of all the nodes in
//! a CFG by traversing its DomTree bottom-up (that is, depth-first postorder).
//!
//! This makes the complexity of the DF algorithm `O(E + N^2)`, with `E` and `N` being the
//! number of edges and nodes in the CFG, respectively.
//!
//! After that, we insert Phi nodes according to another algorithm in the paper, At first,
//! we place phi-nodes without worrying about correclty naming variables, as that is done
//! in a pass right after.
//!
//! I do not yet fully understand the phi-placement algorithm, but that will come with
//! time.
//!
//! After the phi-placement and renaming, we can uniquely identify any temporary with
//! `(func: ILFunction, name: StringKey, number: u32)`
//!
//! There is currently also a discrepancy in the 'key' for [`ILBlockData`], as the IR
//! guarantees uniqueness of a block's label in a given function

use keyvec::InternKey;
use log::trace;
use petgraph::algo::dominators::Dominators;
use petgraph::graphmap::DiGraphMap;
use std::collections::{HashMap, VecDeque};
use std::{collections::HashSet, fmt::Debug};

use crate::StringKey;
use crate::il::{
    self, ILBlockData, ILBlockIdentifier, ILBlockItem, ILCall, ILFunction, ILInstruction, ILModule,
    ILModuleContext, ILOperand, ILPhiNode, ILTemp, ILTerminator, ILType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CFGSymbolEvent {
    /// A temp whose value is a phi-node: `%t <- phi(...)`
    DefPhi(StringKey),
    /// A temp-operand to a phi function `... <- phi(@b %t, ...)`
    PhiUseLocal(StringKey),
    /// Definition of a temp: `%t <- ...`
    DefTemp(StringKey),
    /// Usage of a temporary: `... <- %t`
    UseTemp(StringKey),
    /// Usage of a global: `... <- $g`
    UseGlobal(StringKey),
    /// A call to a function `... <- $f(...)`
    Call(StringKey),
    PhiUseGlobal(StringKey),
    LivenessUseTemp(StringKey),
}

impl CFGSymbolEvent {
    const fn into_string_key(self) -> StringKey {
        match self {
            CFGSymbolEvent::DefPhi(string_key)
            | CFGSymbolEvent::PhiUseLocal(string_key)
            | CFGSymbolEvent::Call(string_key)
            | CFGSymbolEvent::DefTemp(string_key)
            | CFGSymbolEvent::UseGlobal(string_key)
            | CFGSymbolEvent::PhiUseGlobal(string_key)
            | CFGSymbolEvent::LivenessUseTemp(string_key)
            | CFGSymbolEvent::UseTemp(string_key) => string_key,
        }
    }
}
impl std::fmt::Display for CFGSymbolEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.into_string_key().unintern();
        let prefix = match self {
            CFGSymbolEvent::DefTemp(_) => "%",
            CFGSymbolEvent::LivenessUseTemp(_) | CFGSymbolEvent::UseTemp(_) => "<- %",
            CFGSymbolEvent::Call(_) => "<- $",
            CFGSymbolEvent::DefPhi(_) => "%",
            CFGSymbolEvent::PhiUseLocal(_) => "<- phi %",
            CFGSymbolEvent::PhiUseGlobal(_) => "<- phi $",
            CFGSymbolEvent::UseGlobal(_) => "<- $",
        };
        let suffix = match self {
            CFGSymbolEvent::DefTemp(_) => " <-",
            CFGSymbolEvent::Call(_) => "()",
            CFGSymbolEvent::DefPhi(_) => " <- phi",
            CFGSymbolEvent::UseTemp(_)
            | CFGSymbolEvent::PhiUseLocal(_)
            | CFGSymbolEvent::PhiUseGlobal(_)
            | CFGSymbolEvent::UseGlobal(_)
            | CFGSymbolEvent::LivenessUseTemp(_) => "",
        };
        write!(f, "{prefix}{s}{suffix}")
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
            il::ILOperand::Temp(temp) => {
                if !sink.contains(&CFGSymbolEvent::DefTemp(temp.name())) {
                    sink.push(CFGSymbolEvent::LivenessUseTemp(temp.name()));
                }
                sink.push(CFGSymbolEvent::UseTemp(temp.name()))
            }
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
                    il::ILOperand::Temp(t) => {
                        if !(sink.contains(&CFGSymbolEvent::DefTemp(t.name()))
                            && sink.contains(&CFGSymbolEvent::DefPhi(t.name())))
                        {
                            sink.push(CFGSymbolEvent::LivenessUseTemp(t.name()));
                        }
                        sink.push(CFGSymbolEvent::PhiUseLocal(t.name()))
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

impl EmitCFGEvents for il::ILFunctionData {
    fn emit_cfg_events(&self, sink: &mut Vec<CFGSymbolEvent>) {
        for param in &self.signature.param_temporaries {
            sink.push(CFGSymbolEvent::DefTemp(param.name()));
        }
        for block in &self.blocks {
            block.emit_cfg_events(sink);
        }
    }
}

/// The main structure that keeps track of metadata related to the SSA-ification of an
/// [`ILModule`]
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    cfg: DiGraphMap<ILBlockIdentifier, ()>,
    domfrontiers: HashMap<ILBlockIdentifier, HashSet<ILBlockIdentifier>>,
    def_uses: HashMap<ILBlockIdentifier, Vec<CFGSymbolEvent>>,
}

// dominance frontier computations
impl ControlFlowGraph {
    /// Get the set of successors of a given node
    fn get_successors(
        &self,
        cfg_node: ILBlockIdentifier,
    ) -> impl Iterator<Item = ILBlockIdentifier> {
        self.cfg
            .edges_directed(cfg_node, petgraph::Direction::Outgoing)
            .map(|(_cfg_node, dest, _)| dest)
    }

    /// Get the set of predecessors of a given node
    pub(crate) fn get_predecessors(
        &self,
        cfg_node: ILBlockIdentifier,
    ) -> impl Iterator<Item = ILBlockIdentifier> + '_ {
        self.cfg
            .edges_directed(cfg_node, petgraph::Direction::Incoming)
            .map(|(src, _cfg_node, _)| src)
    }

    /// This method computes the dominance frontier for all CFG nodes in a
    /// function.
    ///
    /// This runs in `O(E + N^2)`, with `E` and `N` being the amount of edges
    /// and nodes in the CFG.
    fn compute_domfrontiers(
        &mut self,
        function_entry_block: ILBlockIdentifier,
        ctx: &ILModuleContext,
    ) -> Result<(), TempUsageError> {
        let domtree = self.domtree(function_entry_block);
        let bottomup = DomTreeDFSPostOrder::build(&domtree, self);
        for x in bottomup {
            self.verify_usages(x, &domtree, ctx)?;
            self.compute_single_domfrontier(x, &domtree);
        }
        Ok(())
    }
    /// compute the dominance frontier of a given node and cache it in
    /// `self.dominance_frontiers`.
    ///
    /// This method requires all of x's children to have their DF cached in
    /// `self.dominance_frontiers` This is guaranteed by [`self.compute_dfs`] visiting the
    /// nodes in DFS postorder.
    ///
    /// this is part of the algorithm described in the paper.
    #[inline]
    fn compute_single_domfrontier(
        &mut self,
        x: ILBlockIdentifier,
        domtree: &Dominators<ILBlockIdentifier>,
    ) {
        let mut domfrontier = HashSet::new();
        for y in self.get_successors(x) {
            // if x dominates a predeccessor of y (which could be x itself),
            // but does not strictly dominate y, then y in DF(x).
            if domtree.immediate_dominator(y) != Some(x) {
                domfrontier.insert(y);
            }
        }
        // x's children in the domtree
        for z in domtree.immediately_dominated_by(x) {
            debug_assert!(
                self.domfrontiers.contains_key(&z),
                "postorder invariant violated"
            );
            // each y in the dominance frontier of node z, which is a child of x
            for &y in &self.domfrontiers[&z] {
                if domtree.immediate_dominator(y) != Some(x) {
                    domfrontier.insert(y);
                }
            }
        }
        // cache the result
        self.domfrontiers.insert(x, domfrontier);
    }
    pub fn domtree(
        &self,
        root: ILBlockIdentifier,
    ) -> petgraph::algo::dominators::Dominators<ILBlockIdentifier> {
        debug_assert!(self.cfg.contains_node(root));
        petgraph::algo::dominators::simple_fast(&self.cfg, root)
    }

    fn nodes(&self) -> petgraph::graphmap::Nodes<'_, ILBlockIdentifier> {
        self.cfg.nodes()
    }

    fn verify_usages(
        &self,
        x @ ILBlockIdentifier(func, _label): ILBlockIdentifier,
        domtree: &Dominators<ILBlockIdentifier>,
        ctx: &ILModuleContext,
    ) -> Result<(), TempUsageError> {
        let def_uses = self.def_uses[&x].as_slice();

        for event in def_uses {
            match event {
                CFGSymbolEvent::UseTemp(temp) | CFGSymbolEvent::PhiUseLocal(temp) => {
                    if let TempDefinitionKind::NeverDefined =
                        self.search_definition(*temp, x, domtree)
                    {
                        return Err(TempUsageError::UseBeforeDefinition {
                            culprit: *temp,
                            used_in: x,
                            func_name: ctx.get_func_name(func),
                        });
                    } else if let TempDefinitionKind::DefinedInNonDominator { nearest_def } =
                        self.search_definition(*temp, x, domtree)
                    {
                        return Err(TempUsageError::MaybeUsedBeforeDefinition {
                            culprit: *temp,
                            used_in: x,
                            nearest_def,
                            func_name: ctx.get_func_name(func),
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn search_definition(
        &self,
        usage: StringKey,
        x: ILBlockIdentifier,
        domtree: &Dominators<ILBlockIdentifier>,
    ) -> TempDefinitionKind {
        let mut candidate_block = x;
        loop {
            if let res @ TempDefinitionKind::Defined =
                self.search_definition_in_events(usage, candidate_block)
            {
                return res;
            }

            if let Some(new_candidate) = domtree.immediate_dominator(candidate_block) {
                candidate_block = new_candidate;
            } else {
                break;
            }
        }
        for pred in self.get_predecessors(x) {
            if let TempDefinitionKind::Defined = self.search_definition_in_events(usage, pred) {
                return TempDefinitionKind::DefinedInNonDominator { nearest_def: pred };
            }
        }
        TempDefinitionKind::NeverDefined
    }

    fn search_definition_in_events(
        &self,
        usage: StringKey,
        candidate: ILBlockIdentifier,
    ) -> TempDefinitionKind {
        for event in self.def_uses[&candidate].iter().rev() {
            if let CFGSymbolEvent::DefTemp(def) | CFGSymbolEvent::DefPhi(def) = event
                && *def == usage
            {
                return TempDefinitionKind::Defined;
            }
        }
        TempDefinitionKind::NeverDefined
    }

    fn defs_in(&self, block: ILBlockIdentifier) -> impl Iterator<Item = StringKey> {
        self.def_uses[&block]
            .iter()
            .filter_map(|event| match event {
                CFGSymbolEvent::DefPhi(t) | CFGSymbolEvent::DefTemp(t) => Some(t),
                _ => None,
            })
            .copied()
    }
    fn uses_in(&self, block: ILBlockIdentifier) -> impl Iterator<Item = StringKey> {
        self.def_uses[&block]
            .iter()
            .filter_map(|event| match event {
                CFGSymbolEvent::LivenessUseTemp(t) | CFGSymbolEvent::PhiUseLocal(t) => Some(t),
                _ => None,
            })
            .copied()
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TempDefinitionKind {
    /// A dominator of the node defines the usage
    Defined,
    /// A predecessor that is not a dominator defines the usage
    DefinedInNonDominator { nearest_def: ILBlockIdentifier },
    /// No dominator or predecessor defines the usage
    NeverDefined,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, thiserror::Error)]
pub enum TempUsageError {
    UseBeforeDefinition {
        culprit: StringKey,
        used_in: ILBlockIdentifier,
        func_name: StringKey,
    },
    MaybeUsedBeforeDefinition {
        culprit: StringKey,
        used_in: ILBlockIdentifier,
        nearest_def: ILBlockIdentifier,
        func_name: StringKey,
    },
}
impl std::fmt::Display for TempUsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TempUsageError::UseBeforeDefinition {
                culprit,
                used_in,
                func_name,
            } => write!(
                f,
                "temporary %{0} used at @{1}::{2} where it is not defined",
                culprit.unintern(),
                func_name.unintern(),
                used_in.1.unintern(),
            ),
            TempUsageError::MaybeUsedBeforeDefinition {
                culprit,
                used_in,
                nearest_def,
                func_name,
            } => write!(
                f,
                "temporary %{0} used in a place where it might note be defined (used in @{1}::{2})\nthe nearest definition is in @{1}::{3}",
                culprit.unintern(),
                func_name.unintern(),
                used_in.1.unintern(),
                nearest_def.1.unintern(),
            ),
        }
    }
}

impl ControlFlowGraph {
    pub fn empty() -> Self {
        Self {
            cfg: Default::default(),
            domfrontiers: HashMap::new(),
            def_uses: HashMap::new(),
        }
    }

    pub fn build(for_module: &ILModule) -> Result<Self, TempUsageError> {
        let start = std::time::Instant::now();
        let mut s = CFGGraphBuilder::new(for_module).construct_cfg();
        let cfg_elapsed = start.elapsed();
        for f in for_module.functions().keys() {
            s.compute_domfrontiers(
                ILBlockIdentifier(f, for_module.entry_block_of_func(f).label),
                &for_module.ctx,
            )?;
        }
        let dfs_elapsed = start.elapsed();
        trace!(
            "CFG-graph constructed in {} microseconds\nDF's computed in {} microseconds",
            cfg_elapsed.as_micros(),
            dfs_elapsed.as_micros(),
        );
        Ok(s)
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
    /// calling this function multiple times for the same pair of nodes has no
    /// effect
    fn add_intra_functional_cfg_edge(
        &mut self,
        func_id: ILFunction,
        src: StringKey,
        dest: StringKey,
    ) {
        self.cfg.add_edge(
            ILBlockIdentifier(func_id, src),
            ILBlockIdentifier(func_id, dest),
            (),
        );
    }

    /// introduce a node into the graph,
    /// calling this function multiple times for the same node
    /// has no effect
    fn add_node(&mut self, func_id: ILFunction, id: StringKey) {
        self.cfg.add_node(ILBlockIdentifier(func_id, id));
    }

    fn fmt_func_block_id_uninterned(ctx: &ILModuleContext, node: ILBlockIdentifier) -> String {
        format!(
            "{}::{}",
            ctx.get_func_name(node.func()).unintern(),
            node.label().unintern()
        )
    }
    fn fmt_func_block_id_interned(node: ILBlockIdentifier) -> String {
        format!("{}b{}", node.func().into_usize(), node.label().into_usize())
    }
}

impl ILOperand {
    fn rename_use(&mut self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        if let ILOperand::Temp(t) = self {
            trace!("usage of {}", t.name().unintern());
            t.rename_rhs_use(stacks);
        }
    }
}

impl ILTemp {
    fn rename_rhs_use(&mut self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        // dbg!(&stacks);
        let name = self.name();
        let _ = stacks.entry(self.name()).or_insert((1, vec![0]));

        let top = match stacks
            .get(&name)
            .unwrap_or_else(|| {
                panic!(
                    "no entry for {}, stack: {:?}",
                    name.unintern(),
                    stacks
                        .iter()
                        .map(|(a, b)| (a.unintern(), b))
                        .collect::<Vec<_>>()
                )
            })
            .1
            .last()
        {
            Some(top) => top,
            None => {
                panic!(
                    "{:?}",
                    stacks
                        .iter()
                        .map(|(a, b)| format!("{}, {b:?}", a.unintern()))
                        .collect::<Vec<_>>()
                )
            }
        };
        *self = ILTemp::SSA(name, *top)
    }
    fn number(&mut self, number: u32) {
        match self {
            ILTemp::NonSSA(string_key) => *self = ILTemp::SSA(*string_key, number),
            ILTemp::SSA(_, old) => *old = number,
        }
    }

    fn rename_lhs_def(&mut self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        let _ = stacks.entry(self.name()).or_insert((1, vec![0]));
        // dbg!(&stacks, self.name().unintern());
        let count = stacks[&self.name()].0;
        self.number(count);
        let (old_count, v) = stacks
            .get_mut(&self.name())
            .unwrap_or_else(|| panic!("no entry for {}", self.name().unintern()));
        v.push(count);
        *old_count += 1;
    }
    fn pop_old_def(&self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        trace!("pop old def of {}", self.name().unintern());
        stacks.get_mut(&self.name()).unwrap().1.pop().unwrap();
    }
}
impl ILBlockItem {
    fn rename_uses(&mut self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        match self {
            ILBlockItem::Call(ILCall { args, .. }) => {
                for (_iltype, arg) in args.iter_mut() {
                    arg.rename_use(stacks);
                }
            }
            ILBlockItem::AssignInstr(_, _, ilinstruction) => ilinstruction.number_usage(stacks),
            ILBlockItem::Store(_, val, addr) => {
                val.rename_use(stacks);
                addr.rename_use(stacks);
            }
        }
    }
    fn rename_defs(&mut self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        match self {
            ILBlockItem::Call(ILCall {
                assignee: Some(assignee),
                ..
            }) => {
                assignee.0.rename_lhs_def(stacks);
            }
            ILBlockItem::AssignInstr(assignee, _, _) => {
                trace!("def of {}", assignee.name().unintern());
                assignee.rename_lhs_def(stacks);
            }
            _ => {}
        }
    }
    fn pop_old_defs(&self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        match self {
            ILBlockItem::Call(ILCall {
                assignee: Some((t, _)),
                ..
            }) => {
                t.pop_old_def(stacks);
            }
            ILBlockItem::AssignInstr(t, ..) => t.pop_old_def(stacks),
            _ => {}
        }
    }
}
impl ILInstruction {
    fn number_usage(&mut self, stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>) {
        match self {
            ILInstruction::Unop(_, arg)
            | ILInstruction::Copy(arg)
            | ILInstruction::Load(_, arg) => arg.rename_use(stacks),

            ILInstruction::Cmp(_, _, a, b) | ILInstruction::Binop(_, a, b) => {
                a.rename_use(stacks);
                b.rename_use(stacks);
            }
            _ => {}
        }
    }
}

// DOT dumping
impl ControlFlowGraph {
    pub fn dump_cfg_dot(&self, ctx: &ILModuleContext) -> String {
        let mut s = String::from("subgraph cluster_cfg {\nlabel=\"Controlflow graph\"\n");
        for node in self.nodes() {
            let fmt_block_events = self.fmt_block_events(node);
            s += &format!(
                "    cfg{} [ shape=rect,label = \"{}\n{}\"]\n",
                ControlFlowGraph::fmt_func_block_id_interned(node),
                ControlFlowGraph::fmt_func_block_id_uninterned(ctx, node),
                fmt_block_events
            );
        }
        for (a, b, _) in self.cfg.all_edges() {
            s += &format!(
                "    cfg{} -> cfg{}\n",
                ControlFlowGraph::fmt_func_block_id_interned(a),
                ControlFlowGraph::fmt_func_block_id_interned(b)
            );
        }
        s += "}";
        s
    }
    pub fn dump_domtree_dot(&self, ctx: &ILModuleContext) -> String {
        let mut s = String::from("subgraph cluster_domtree {\n");
        DomTreeDOT::new(ctx, self).dump_into(&mut s);
        s.push_str("}\n");
        s
    }

    fn fmt_block_events(&self, node: ILBlockIdentifier) -> String {
        let events = &self.def_uses[&node];
        let mut s = String::new();
        for e in events {
            s += &format!("{e}\n")
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

        let first_block_node = ILBlockIdentifier(func_id, blocks[0].1.label);
        self.graph
            .def_uses
            .entry(first_block_node)
            .or_default()
            .extend(
                func.signature
                    .param_temporaries
                    .iter()
                    .map(|t| CFGSymbolEvent::DefTemp(t.name())),
            );

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
        self.graph
            .def_uses
            .insert(ILBlockIdentifier(func_id, cur_block), events);
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
    domtree: &'d Dominators<ILBlockIdentifier>,
    cfg: &'d ControlFlowGraph,
    visiting_queue: DomTreeVisitor,
}

/// This iterator wraps a Vec to provide a stack-like iterator
/// It guarantees for all nodes `n` in the DomTree,
/// that all of `n`'s children are returned from the iterator
/// before `n` itself is, aka postorder traversal.
struct DomTreeVisitor(Vec<ILBlockIdentifier>);

impl DomTreeVisitor {
    fn push(&mut self, v: ILBlockIdentifier) {
        self.0.push(v);
    }
}
impl Iterator for DomTreeVisitor {
    type Item = ILBlockIdentifier;
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
    fn build(
        domtree: &'d Dominators<ILBlockIdentifier>,
        cfg: &'d ControlFlowGraph,
    ) -> DomTreeVisitor {
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
        // We could also define a little iterator wrapper that wraos a Vec and
        // acts like a stack, to save the `.into_iter().collect()` call
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
            let entry = self.ctx.function_entry_block(f).label;
            let domtree = &self.cfg.domtree(ILBlockIdentifier(f, entry));
            self.dump_single_function_into(&mut dom_relation, domtree);
        }
        for node in self.cfg.nodes() {
            let n = ControlFlowGraph::fmt_func_block_id_interned(node);
            let label = ControlFlowGraph::fmt_func_block_id_uninterned(self.ctx, node);
            sink.push_str(&format!("dom{n} [label=\"{label}\"]\n"));
        }
        for (dom, n) in dom_relation {
            let dom = ControlFlowGraph::fmt_func_block_id_interned(dom);
            let n = ControlFlowGraph::fmt_func_block_id_interned(n);
            sink.push_str(&format!("dom{dom} -> dom{n}\n"));
        }
    }

    fn dump_single_function_into(
        &self,
        dom_relation: &mut Vec<(ILBlockIdentifier, ILBlockIdentifier)>,
        domtree: &Dominators<ILBlockIdentifier>,
    ) {
        // use a bottom up to guarantee each node gets
        // visited exactly once
        let bottomup = DomTreeDFSPostOrder::build(domtree, self.cfg);
        for n in bottomup {
            if let Some(dom) = domtree.immediate_dominator(n) {
                dom_relation.push((dom, n));
            }
        }
    }
}

/// This struct has methods that insert phi-functions into a given module.
pub struct PhiNodeInsertion {
    cfg: ControlFlowGraph,
}

#[derive(Default)]
pub struct PhiNodeInsertionMetaData {
    /// from the paper:
    ///
    /// an array of flags, one for each node, where has_already(x) indicates
    /// whether a phi-function for V has already been inserted at x
    has_already: u32,
    /// from the paper:
    ///
    /// an array of flags, one flag for each node, where Work(X) indicates
    /// whether X has ever been added to W during the current iteration of the
    /// outer loop.
    node_work: u32,
    live_in: HashSet<StringKey>,
    live_out: HashSet<StringKey>,
}
impl PhiNodeInsertionMetaData {
    fn clear(&mut self) {
        *self = PhiNodeInsertionMetaData::default();
    }
}

type Meta = HashMap<ILBlockIdentifier, PhiNodeInsertionMetaData>;

impl<'m> PhiNodeInsertion {
    pub fn meta_of(
        &self,
        meta: &'m mut Meta,
        node: ILBlockIdentifier,
    ) -> &'m mut PhiNodeInsertionMetaData {
        meta.entry(node).or_default()
    }
    pub fn set_node_work(&self, meta: &'m mut Meta, node: ILBlockIdentifier, val: u32) {
        self.meta_of(meta, node).node_work = val;
    }
    pub fn set_has_already(&self, meta: &'m mut Meta, node: ILBlockIdentifier, val: u32) {
        self.meta_of(meta, node).has_already = val;
    }
    pub fn add_temp_live_in(
        &self,
        meta: &'m mut Meta,
        node: ILBlockIdentifier,
        temp: StringKey,
    ) -> bool {
        self.meta_of(meta, node).live_in.insert(temp)
    }
    pub fn add_temp_live_out(
        &self,
        meta: &'m mut Meta,
        node: ILBlockIdentifier,
        temp: StringKey,
    ) -> bool {
        self.meta_of(meta, node).live_out.insert(temp)
    }
}

impl PhiNodeInsertion {
    pub fn new(cfg: ControlFlowGraph) -> Self {
        Self { cfg }
    }

    /// from the paper:
    ///
    /// The flags Work(X) and HasAlready(X) are independent.
    /// We need two flags because the property of assigning to V is independent
    /// of the property of i needing a d-function for V.
    /// The flags could have been implemented with just the values true and
    /// false, but this would require additional record keeping to reset any
    /// true flags between iterations, without the expense of looping over all
    /// the nodes. It is simpler to devote an integer to each flag and to test
    /// flags by comparing them with the current iteration count.
    pub fn insert_phis_pruned(
        &mut self,
        module: &mut ILModule,
        function: ILFunction,
        meta: &mut Meta,
    ) {
        let mut itercount = 0;

        // (b, v) means block b assigns to variable v
        let mut assigns = HashSet::new();
        let mut all_vars = HashSet::new();
        self.fill_relations(module, function, &mut assigns, &mut all_vars);
        // from the paper:
        //
        // the worklist of CFG nodes being processed.
        // In each iteration of this algorithm, W is initialized to the set &(V)
        // of nodes that contain assignments to V. Each node X in the worklist
        // ensures that each node Y in DF( X) receives a phi-function. Each
        // iteration terminates when the worklist becomes empty.
        let mut worklist = VecDeque::new();

        for &var in &all_vars {
            let mut assigned_typ = None;
            let blocks_assigning_to_var = assigns.iter().filter(|(_node, v, _typ)| v.name() == var);
            itercount += 1;
            for (defining_block, _v, typ) in blocks_assigning_to_var {
                assigned_typ = Some(*typ);
                self.set_node_work(meta, *defining_block, itercount);
                if worklist.contains(defining_block) {
                    continue;
                }
                worklist.push_back(*defining_block);
            }
            while let Some(defining_block) = worklist.pop_front() {
                let assigned_typ = assigned_typ.unwrap();
                let df_x = self.cfg.domfrontiers[&defining_block].clone();
                for frontier_block in df_x {
                    // have we processed this block in this iteration already?
                    if self.meta_of(meta, frontier_block).has_already >= itercount {
                        continue;
                    }
                    let block = module.get_block_mut(frontier_block);

                    if meta[&frontier_block].live_in.contains(&var) {
                        let preds = self.cfg.get_predecessors(frontier_block);
                        push_var_into_phinode(assigned_typ, var, preds, block);
                    } else {
                        trace!(
                            "var `{}` dead after {}",
                            var.unintern(),
                            frontier_block.label().unintern()
                        );
                    }
                    self.set_has_already(meta, frontier_block, itercount);
                    if meta[&frontier_block].node_work >= itercount {
                        continue;
                    }
                    self.set_node_work(meta, frontier_block, itercount);
                    if worklist.contains(&frontier_block) {
                        continue;
                    }
                    worklist.push_back(frontier_block);
                }
            }
        }
    }

    fn fill_relations(
        &self,
        module: &mut ILModule,
        function: ILFunction,
        assigns: &mut HashSet<(ILBlockIdentifier, ILTemp, ILType)>,
        all_vars: &mut HashSet<StringKey>,
    ) {
        let blocks = &module.get_function(function).blocks;
        for block in blocks {
            let node = ILBlockIdentifier(function, block.label);
            for item in &block.items {
                match item {
                    il::ILBlockItem::Call(il::ILCall {
                        assignee: Some((var, typ)),
                        args,
                        ..
                    }) => {
                        assigns.insert((node, *var, *typ));
                        all_vars.insert(var.name());
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
    fn add_temp_operand_to_all_vars(&self, all_vars: &mut HashSet<StringKey>, operand: &ILOperand) {
        if let ILOperand::Temp(t) = operand {
            all_vars.insert(t.name());
        }
    }

    pub fn rename_pass(
        &self,
        ctx: &mut ILModule,
        domtree: &Dominators<ILBlockIdentifier>,
        function: ILFunction,
    ) {
        let mut stacks: HashMap<StringKey, (u32, Vec<u32>)> = HashMap::new();
        let entry = ctx.entry_block_of_func(function).label;
        self.rename_block_items(
            domtree,
            ILBlockIdentifier(function, entry),
            ctx,
            &mut stacks,
        );
    }

    fn rename_block_items(
        &self,
        domtree: &Dominators<ILBlockIdentifier>,
        label: ILBlockIdentifier,
        ctx: &mut ILModule,
        stacks: &mut HashMap<StringKey, (u32, Vec<u32>)>,
    ) {
        trace!("renaming in {}", label.label().unintern());
        let cur_block = ctx.get_block_mut(label);
        let cur_label = cur_block.label;
        // we must first process phi-nodes
        for phi in &mut cur_block.phis {
            trace!("phi-defs for {}", phi.assignee.name().unintern());
            phi.assignee.rename_lhs_def(stacks);
        }
        // so that the block can use its definitions in its items
        for item in &mut cur_block.items {
            item.rename_uses(stacks);
            item.rename_defs(stacks);
        }
        for y in self.cfg.get_successors(label) {
            let t_block = ctx.get_block_mut(y);
            for phi in &mut t_block.phis {
                for pred in &mut phi.incoming {
                    if pred.0 == cur_label {
                        pred.1.rename_use(stacks);
                    }
                }
            }
        }
        for y in domtree.immediately_dominated_by(label) {
            self.rename_block_items(domtree, y, ctx, stacks);
        }
        // reborrow the block to avoid borrow checker complains
        let cur_block = ctx.get_block_mut(label);
        for phi in &mut cur_block.phis {
            phi.assignee.pop_old_def(stacks);
        }
        for item in &mut cur_block.items {
            item.pop_old_defs(stacks);
        }
    }

    /// Insert phi-nodes and rename temporaries.
    ///
    /// After this function returns, `module` will be in pruned SSA form.
    pub fn visit_function(&mut self, f: ILFunction, module: &mut ILModule) {
        // block state for the liveness and phi insertion algorithms
        let mut meta = HashMap::new();
        self.compute_liveness(module, f, &mut meta);
        self.insert_phis_pruned(module, f, &mut meta);
        let entry = module.entry_block_of_func(f).label;
        let domtree = &self.cfg.domtree(ILBlockIdentifier(f, entry));
        self.rename_pass(module, domtree, f);
    }

    /// traverse the domtree rooted at `block`, and check if there is any usage of `temp`
    /// after `block`
    pub fn temp_live_after(
        &self,
        temp: StringKey,
        defining_block: ILBlockIdentifier,
        meta: &Meta,
    ) -> bool {
        match meta.get(&defining_block) {
            Some(meta) => meta.live_in.contains(&temp),
            None => false,
        }
    }

    fn compute_liveness(
        &mut self,
        module: &mut ILModule,
        f: ILFunction,

        meta: &mut HashMap<ILBlockIdentifier, PhiNodeInsertionMetaData>,
    ) {
        let nodes: Vec<_> = self.cfg.nodes().collect();
        for node in nodes {
            self.meta_of(meta, node).clear();
        }

        loop {
            let mut changed = false;
            for block in module.iter_block_labels(f) {
                let old_len = self.meta_of(meta, block).live_out.len();
                let mut live_out_closure = HashSet::new();
                for succ in self.cfg.get_successors(block) {
                    if let Some(meta) = meta.get(&succ) {
                        for live_in_succ in meta.live_in.iter() {
                            live_out_closure.insert(*live_in_succ);
                        }
                    }
                }
                self.meta_of(meta, block).live_out = live_out_closure;

                let new_len = self.meta_of(meta, block).live_out.len();
                if old_len != new_len {
                    changed = true;
                }

                let old_len = self.meta_of(meta, block).live_in.len();
                // live_out[block] \ defs[block]
                let rhs: HashSet<StringKey> = self
                    .meta_of(meta, block)
                    .live_out
                    .clone()
                    .difference(&self.cfg.defs_in(block).collect())
                    .copied()
                    .collect();
                let lhs = self.cfg.uses_in(block).collect();
                let live_in_closure: HashSet<_> = rhs.union(&lhs).copied().collect();
                let new_len = live_in_closure.len();
                if old_len != new_len {
                    changed = true;
                }
                self.meta_of(meta, block).live_in = live_in_closure;
            }
            if !changed {
                break;
            }
        }
    }
}

fn push_var_into_phinode(
    typ_phifix: ILType,
    var: StringKey,
    predecessors: impl Iterator<Item = ILBlockIdentifier>,
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
    p.sort_incoming_by_label();
}
#[cfg(test)]
mod tests {

    use crate::builder_dsl;

    macro_rules! phis {
    ($($label:expr => $op:expr),*) => {{
        use $crate::il::ILOperand;
        let mut labels = Vec::new();
        $(
            labels.push(($label.label(), ILOperand::Temp($op)));
        )*
        labels.sort_by_key(|a|a.0);
        labels}
    };
}

    fn cmpify_phinode(node: &[ILPhiNode]) -> Vec<(ILTemp, Vec<(StringKey, ILOperand)>)> {
        let mut phis: Vec<_> = node
            .to_vec()
            .into_iter()
            .map(|phi| (phi.assignee, phi.incoming.clone()))
            .collect();
        phis.sort_by_key(|a| a.0.name());
        phis
    }

    fn pretty_philist(
        phis: Vec<(ILTemp, Vec<(StringKey, ILOperand)>)>,
    ) -> Vec<(Box<str>, Vec<(Box<str>, String)>)> {
        phis.iter()
            .map(|(a, b)| {
                (
                    a.name().unintern(),
                    b.into_iter()
                        .map(|(a, b)| (a.unintern(), b.to_string()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    }

    macro_rules! check_phi_operands {
    ($phis:expr; $len:literal: $($a:expr; {$($label:expr => $op:expr),+ $(,)?}),+) => {
        let phis = cmpify_phinode(&$phis);
        let mut labels = vec![];
        $(labels.push(($a, phis!($($label => $op),+)));)+
        labels.sort_by_key(|a|a.0.name());
        if phis.len() != labels.len() {
            panic!("unexpected phi: {:#?}", pretty_philist(phis));
        }
        for (phi, exp) in phis.iter().zip(labels) {
            if phi.0.name() != exp.0.name() {
                panic!(
                    "phi node assigness unequal: expected `{}` != received `{}`",
                    phi.0.name().unintern(),
                    exp.0.name().unintern()
                );
            }
            for ((label,got), (_,exp)) in phi.1.iter().zip(exp.1) {
                let ILOperand::Temp(got) = got else {unreachable!()};
                let ILOperand::Temp(exp) = exp else {unreachable!()};
                if got.name() != exp.name() {
                    panic!(
                        "@{}: {} != {}",
                        label.unintern(),
                        got.name().unintern(),
                        exp.name().unintern()
                    );
                }
            }
        }
};
    ($phis:expr; {}) => {
        assert!($phis.is_empty(), "unexpected phi-node");

    };
}

    use super::*;

    /// A scenario a dominator-not-predecessor of a node defines a temp
    #[test]
    fn ssa_defined_in_dominator() {
        let mut build = crate::il::builder::ILModuleBuilder::start();
        let type_i8_interned = build.get_type_I8();
        let type_i8 = il::ILType::I8;
        let builder = &mut build;

        builder_dsl!(func builder;
        function foo(type_i8_interned) -> type_i8_interned {
            @start
            |start_a| a =type_i8 (3);
            |cond| cond =type_i8 (1);
            !(jmpz cond, @bt, @bf)

            @bt
            !(jmp @merge)

            @bf
            !(jmp @merge)

            @merge
            |merge_a| a =type_i8 (a);
            !(ret a)
        }

        );

        let mut m = build.finish();
        let cfg = ControlFlowGraph::build(&m).unwrap();
        PhiNodeInsertion::new(cfg).visit_function(foo, &mut m);
        check_phi_operands!(
            m.get_block(merge).phis;
            {}
        );
    }
    /// A scenario where all predecessors of a block define some temp
    #[test]
    fn ssa_defined_in_all_predecessors_simple() {
        let mut build = crate::il::builder::ILModuleBuilder::start();
        let ti8 = il::ILType::I8;
        let builder = &mut build;

        builder_dsl!(func builder;
        function foo(ti8) -> ti8 {
            @start
            |cond| cond =ti8 (1);
            !(jmpz cond, @bt, @bf)

            @bt
            |bt_a| a =ti8 (3);
            !(jmp @merge)

            @bf
            |bf_a| a =ti8 (3);
            !(jmp @merge)

            @merge
            |merge_a| a =ti8 (a);
            !(ret a)
        }

        );

        let mut m = build.finish();
        let cfg = ControlFlowGraph::build(&m).unwrap();
        PhiNodeInsertion::new(cfg).visit_function(foo, &mut m);
        check_phi_operands!(
            &m.get_block(merge).phis; 1:
            merge_a ; {bf => bf_a, bt => bt_a}
        );
    }
    #[test]
    fn ssa_defined_in_all_predecessors_many_tall() {
        let mut build = crate::il::builder::ILModuleBuilder::start();
        let ti8 = il::ILType::I8;
        let builder = &mut build;

        builder_dsl!(func builder;
        function foo(ti8) -> ti8 {
            @start
            |cond| cond =ti8 (1);
            !(jmpz cond, @bt, @bf)

            @bt
            |bt_a| a =ti8 (3);
            |bt_b| b =ti8 (3);
            |bt_c| c =ti8 (3);
            |bt_d| d =ti8 (3);
            !(jmp @merge)

            @bf
            |bf_a| a =ti8 (3);
            |bf_b| b =ti8 (3);
            |bf_c| c =ti8 (3);
            |bf_d| d =ti8 (3);
            !(jmp @merge)

            @merge
            |merge_a| a =ti8 (a);
            |merge_b| b =ti8 (b);
            |merge_c| c =ti8 (c);
            |merge_d| d =ti8 (d);
            !(ret a)
        }

        );

        let mut m = build.finish();
        let cfg = ControlFlowGraph::build(&m).unwrap();
        PhiNodeInsertion::new(cfg).visit_function(foo, &mut m);
        check_phi_operands!(
            &m.get_block(merge).phis; 4:
            merge_a ; {bf => bf_a, bt => bt_a},
            merge_b ; {bf => bf_b, bt => bt_b},
            merge_c ; {bf => bf_c, bt => bt_c},
            merge_d ; {bf => bf_d, bt => bt_d}
        );
    }
    #[test]
    fn ssa_defined_in_predecessors_many_wide() {
        let mut build = crate::il::builder::ILModuleBuilder::start();
        let ti8 = il::ILType::I8;
        let builder = &mut build;

        builder_dsl!(func builder;
        function foo(ti8) -> ti8 {
            @start
            |cond| cond =ti8 (1);
            !(jmpz cond, @ba, @bb)

            @ba
            |ba_cond| ba_cond =ti8 (1);
            !(jmpz ba_cond, @b0, @b1)
            @b0
            |b0_a| a =ti8 (0);
            !(jmp @merge)
            @b1
            |b1_a| a =ti8 (1);
            !(jmp @merge)

            @bb
            |bb_cond| bb_cond =ti8 (1);
            !(jmpz bb_cond, @b2, @b3)
            @b2
            |b2_a| a =ti8 (2);
            !(jmp @merge)
            @b3
            |b3_a| a =ti8 (3);
            !(jmp @merge)

            @merge
            |merge_a| a =ti8 (a);
            |use_a| a =ti8 (a);
            !(ret a)
        }

        );

        let mut m = build.finish();
        let cfg = ControlFlowGraph::build(&m).unwrap();
        PhiNodeInsertion::new(cfg).visit_function(foo, &mut m);
        check_phi_operands!(
            &m.get_block(merge).phis; 1:
            merge_a ; {b0 => b0_a, b1 => b1_a, b2 => b2_a, b3 => b3_a}
        );
    }
}
