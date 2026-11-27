use std::collections::{HashMap, VecDeque};

use crate::{
    StringKey,
    il::{ILBlockIdentifier, ILFunction, ILFunctionData, ILModule, ILOperand, ILTemp, ILType},
    ssaify::ControlFlowGraph,
};

type UniqueTemp = (ILFunction, StringKey, u32);

pub enum TypeCheckError {
    /// The assignment to a regular temporary is invalid.
    InvalidAssignment {
        func_name: StringKey,
        unique_temp: UniqueTemp,
        expected: ILType,
        got: ILType,
    },
    InvalidReturnType {
        func_name: StringKey,
        return_val: ILOperand,
        expected: ILType,
        received: ILType,
    },
    MisMatchedReturnValues {
        func_name: StringKey,
        expected: ILType,
        received: ILType,
    },
    StrayNonSSATemp {
        func_name: StringKey,
        temp: StringKey,
    },
}

pub struct TypeCheck<'m> {
    ctx: &'m ILModule,
    cfg: &'m ControlFlowGraph,
    query_cache: HashMap<UniqueTemp, ILType>,
}

impl<'m> TypeCheck<'m> {
    pub fn new(ctx: &'m ILModule, cfg: &'m ControlFlowGraph) -> Self {
        Self {
            ctx,
            query_cache: HashMap::new(),
            cfg,
        }
    }

    fn visit_module(&mut self) -> Result<(), TypeCheckError> {
        todo!()
    }

    fn visit_function(
        &mut self,
        func: ILFunction,
        data: &ILFunctionData,
    ) -> Result<(), TypeCheckError> {
        let func_name = self.ctx.get_func_name(func);
        let params = data
            .signature
            .param_types
            .iter()
            .zip(&data.signature.param_temporaries);
        for (iltype_data, iltemp) in params {
            match *iltemp {
                ILTemp::SSA(name, number) => {
                    self.query_cache.insert((func, name, number), *iltype_data);
                }
                ILTemp::NonSSA(name) => {
                    return Err(TypeCheckError::StrayNonSSATemp {
                        func_name: data.name,
                        temp: name,
                    });
                }
            }
        }
        let entry_label = self.ctx.entry_identifier_of_func(func);
        let domtree = self.cfg.domtree(entry_label);
        let mut visiting_queue = VecDeque::new();
        visiting_queue.push_back(entry_label);
        while let Some(visiting) = visiting_queue.pop_front() {
            let block = data.find_block(visiting.label());
            for item in &block.items {
                let () = self.check_block_item(func, func_name, visiting, item)?;
            }
            for phi in &block.phis {
                self.check_phi(func, phi, visiting)?;
            }
        }

        Ok(())
    }

    fn query_symbol_type(&self, temp: UniqueTemp) -> ILType {
        self.query_cache.get(&temp).copied().unwrap()
    }

    fn compare_typs(
        &self,
        func_name: StringKey,
        unique_temp: UniqueTemp,
        expected: ILType,
        got: ILType,
    ) -> Result<(), TypeCheckError> {
        if expected == got {
            Ok(())
        } else {
            Err(TypeCheckError::InvalidAssignment {
                func_name,
                unique_temp,
                expected,
                got,
            })
        }
    }

    fn check_phi(
        &self,
        func: ILFunction,
        phi: &crate::il::ILPhiNode,
        cur_block: ILBlockIdentifier,
    ) -> Result<(), TypeCheckError> {
        let preds: Vec<_> = self.cfg.get_predecessors(cur_block).collect();
        for (label, then) in &phi.incoming {
            for ILBlockIdentifier(_, pred_label) in &preds {
                if pred_label == label {
                    todo!()
                }
            }
        }

        Ok(())
    }

    fn check_block_item(
        &mut self,
        func: ILFunction,
        func_name: StringKey,
        _visiting: ILBlockIdentifier,
        item: &crate::il::ILBlockItem,
    ) -> Result<(), TypeCheckError> {
        match item {
            crate::il::ILBlockItem::Call(ilcall) => {
                if let Some((iltemp, iltype_data)) = ilcall.assignee {
                    let unique = iltemp
                        .as_unique(func)
                        .map_err(|temp| TypeCheckError::StrayNonSSATemp { func_name, temp })?;
                    self.query_cache.insert(unique, iltype_data);
                }
                Ok(())
            }
            crate::il::ILBlockItem::AssignInstr(iltemp, expected_type, ilinstruction) => {
                let unique = iltemp
                    .as_unique(func)
                    .map_err(|temp| TypeCheckError::StrayNonSSATemp { func_name, temp })?;

                self.query_cache.insert(unique, *expected_type);
                match ilinstruction {
                    crate::il::ILInstruction::Load(ilabityp, _) => {
                        self.compare_typs(func_name, unique, *expected_type, ilabityp.base)?;

                        Ok(())
                    }
                    crate::il::ILInstruction::Immi64(_) => todo!(),
                    crate::il::ILInstruction::Immf64(_) => todo!(),
                    crate::il::ILInstruction::Copy(iloperand) => {
                        if let ILOperand::Temp(copier) = *iloperand {
                            let copier_unique = copier.as_unique(func).map_err(|temp| {
                                TypeCheckError::StrayNonSSATemp { func_name, temp }
                            })?;
                            let copier_type = self.query_cache[&copier_unique];
                            self.compare_typs(func_name, unique, *expected_type, copier_type)?;
                            todo!()
                        }
                        todo!()
                    }
                    crate::il::ILInstruction::Cmp(cmp_op, iltype_data, iloperand, iloperand1) => {
                        todo!()
                    }
                    crate::il::ILInstruction::Binop(bin_op, iloperand, iloperand1) => todo!(),
                    crate::il::ILInstruction::Unop(un_op, iloperand) => todo!(),
                }
            }
            crate::il::ILBlockItem::Store(iltype_data, iloperand, iloperand1) => todo!(),
        }
    }
}
