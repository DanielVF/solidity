use super::conditional_simplifier::{function_side_effects, ControlFlowSideEffects};
use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlFlow {
    FlowOut,
    Break,
    Continue,
    Terminate,
    Leave,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let function_side_effects = function_side_effects(request)?;
    DeadCodeEliminator {
        request,
        function_side_effects,
    }
    .visit_block_root()
}

struct DeadCodeEliminator<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    function_side_effects: BTreeMap<u64, ControlFlowSideEffects>,
}

impl DeadCodeEliminator<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        if let Some(index) = self.first_unconditional_control_flow_change(block_id)? {
            let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
            self.request.blocks[block_id as usize].statement_ids = statement_ids
                .into_iter()
                .enumerate()
                .filter_map(|(statement_index, statement_id)| {
                    (statement_index <= index
                        || self.request.statements[statement_id as usize].kind
                            == STATEMENT_FUNCTION_DEFINITION)
                        .then_some(statement_id)
                })
                .collect();
        }

        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
        }
        Ok(())
    }

    fn first_unconditional_control_flow_change(
        &self,
        block_id: u64,
    ) -> Result<Option<usize>, OptimizerError> {
        for (index, statement_id) in self.request.blocks[block_id as usize]
            .statement_ids
            .iter()
            .enumerate()
        {
            if self.control_flow_kind(*statement_id)? != ControlFlow::FlowOut {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => self.visit_expression(statement.value_expression_id),
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.visit_expression(statement.value_expression_id)?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id),
            STATEMENT_IF => {
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(statement.switch_expression_id)?;
                for case_id in statement.case_ids {
                    let switch_case = self.request.cases[case_id as usize].clone();
                    if switch_case.has_value {
                        self.visit_expression(switch_case.value_expression_id)?;
                    }
                    self.visit_block(switch_case.body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                if !self.request.blocks[statement.pre_block_id as usize]
                    .statement_ids
                    .is_empty()
                {
                    return Err(OptimizerError::InvalidWire(
                        "DeadCodeEliminator requires ForLoopInitRewriter.".to_string(),
                    ));
                }
                self.visit_block(statement.pre_block_id)?;
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }

    fn control_flow_kind(&self, statement_id: u64) -> Result<ControlFlow, OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_VARIABLE_DECLARATION if statement.has_value => {
                if self.contains_non_continuing_function_call(statement.value_expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_ASSIGNMENT => {
                if self.contains_non_continuing_function_call(statement.value_expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_EXPRESSION => {
                if self.contains_non_continuing_function_call(statement.expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_BREAK => Ok(ControlFlow::Break),
            STATEMENT_CONTINUE => Ok(ControlFlow::Continue),
            STATEMENT_LEAVE => Ok(ControlFlow::Leave),
            _ => Ok(ControlFlow::FlowOut),
        }
    }

    fn contains_non_continuing_function_call(
        &self,
        expression_id: u64,
    ) -> Result<bool, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL {
            return Ok(false);
        }

        for argument_id in &expression.argument_expression_ids {
            if self.contains_non_continuing_function_call(*argument_id)? {
                return Ok(true);
            }
        }

        match expression.function_name_kind {
            FUNCTION_NAME_BUILTIN => Ok(self
                .request
                .builtins
                .iter()
                .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
                .is_some_and(|builtin| !builtin.control_flow_can_continue)),
            FUNCTION_NAME_IDENTIFIER => Ok(self
                .function_side_effects
                .get(&expression.function_name_name_id)
                .is_some_and(|side_effects| !side_effects.can_continue)),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid function name kind: {}",
                expression.function_name_kind
            ))),
        }
    }
}
