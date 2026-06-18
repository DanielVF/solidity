use super::conditional_simplifier::{function_side_effects, ControlFlowSideEffects};
use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK,
    STATEMENT_BREAK, STATEMENT_CONTINUE, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_LEAVE, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
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
    ConditionalUnsimplifier {
        request,
        function_side_effects,
    }
    .visit_block_root()
}

struct ConditionalUnsimplifier<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    function_side_effects: BTreeMap<u64, ControlFlowSideEffects>,
}

impl ConditionalUnsimplifier<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in &statement_ids {
            self.visit_statement(*statement_id)?;
        }

        let mut rewritten_statement_ids = Vec::with_capacity(statement_ids.len());
        let mut index = 0;
        while index < statement_ids.len() {
            if index + 1 < statement_ids.len()
                && self.can_remove_following_condition_assignment(
                    statement_ids[index],
                    statement_ids[index + 1],
                )?
            {
                rewritten_statement_ids.push(statement_ids[index]);
                index += 2;
            } else {
                rewritten_statement_ids.push(statement_ids[index]);
                index += 1;
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten_statement_ids;
        Ok(())
    }

    fn can_remove_following_condition_assignment(
        &self,
        if_statement_id: u64,
        assignment_statement_id: u64,
    ) -> Result<bool, OptimizerError> {
        let if_statement = &self.request.statements[if_statement_id as usize];
        if if_statement.kind != STATEMENT_IF {
            return Ok(false);
        }

        let condition = &self.request.expressions[if_statement.condition_expression_id as usize];
        if condition.kind != EXPRESSION_IDENTIFIER
            || self.request.blocks[if_statement.body_block_id as usize]
                .statement_ids
                .is_empty()
        {
            return Ok(false);
        }

        let assignment = &self.request.statements[assignment_statement_id as usize];
        if assignment.kind != STATEMENT_ASSIGNMENT
            || assignment.variable_ids.len() != 1
            || self.request.identifiers[assignment.variable_ids[0] as usize].name_id
                != condition.name_id
        {
            return Ok(false);
        }

        let last_body_statement = *self.request.blocks[if_statement.body_block_id as usize]
            .statement_ids
            .last()
            .expect("non-empty checked above");
        if self.control_flow_kind(last_body_statement)? == ControlFlow::FlowOut {
            return Ok(false);
        }

        Ok(self.expression_is_numeric_zero_literal(assignment.value_expression_id))
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
            STATEMENT_SWITCH => self.visit_switch(statement_id),
            STATEMENT_FOR_LOOP => {
                self.visit_block(statement.pre_block_id)?;
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_switch(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        self.visit_expression(statement.switch_expression_id)?;

        if self.request.expressions[statement.switch_expression_id as usize].kind
            != EXPRESSION_IDENTIFIER
        {
            self.visit_expression(statement.switch_expression_id)?;
            for case_id in statement.case_ids {
                let switch_case = self.request.cases[case_id as usize].clone();
                if switch_case.has_value {
                    self.visit_expression(switch_case.value_expression_id)?;
                }
                self.visit_block(switch_case.body_block_id)?;
            }
            return Ok(());
        }

        let switch_expression_name =
            self.request.expressions[statement.switch_expression_id as usize].name_id;

        for case_id in statement.case_ids {
            let switch_case = self.request.cases[case_id as usize].clone();
            if switch_case.has_value {
                self.visit_expression(switch_case.value_expression_id)?;
                self.remove_matching_case_assignment(
                    switch_case.body_block_id,
                    switch_expression_name,
                    switch_case.value_expression_id,
                );
            }
            self.visit_block(switch_case.body_block_id)?;
        }

        Ok(())
    }

    fn remove_matching_case_assignment(
        &mut self,
        body_block_id: u64,
        switch_expression_name: u64,
        case_value_expression_id: u64,
    ) {
        let Some(first_statement_id) = self.request.blocks[body_block_id as usize]
            .statement_ids
            .first()
            .copied()
        else {
            return;
        };

        let assignment = &self.request.statements[first_statement_id as usize];
        if assignment.kind == STATEMENT_ASSIGNMENT
            && assignment.variable_ids.len() == 1
            && self.request.identifiers[assignment.variable_ids[0] as usize].name_id
                == switch_expression_name
            && self.literal_values_equal(assignment.value_expression_id, case_value_expression_id)
        {
            self.request.blocks[body_block_id as usize]
                .statement_ids
                .remove(0);
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

    fn expression_is_numeric_zero_literal(&self, expression_id: u64) -> bool {
        let expression = &self.request.expressions[expression_id as usize];
        expression.kind == EXPRESSION_LITERAL
            && !expression.literal_unlimited
            && expression.literal_value.iter().all(|byte| *byte == 0)
    }

    fn literal_values_equal(&self, left_expression_id: u64, right_expression_id: u64) -> bool {
        let left = &self.request.expressions[left_expression_id as usize];
        let right = &self.request.expressions[right_expression_id as usize];
        if left.kind != EXPRESSION_LITERAL
            || right.kind != EXPRESSION_LITERAL
            || left.literal_unlimited != right.literal_unlimited
        {
            return false;
        }

        if left.literal_unlimited {
            self.request.strings[left.literal_string_id as usize].bytes
                == self.request.strings[right.literal_string_id as usize].bytes
        } else {
            left.literal_value == right.literal_value
        }
    }
}
