use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    ForLoopConditionOutOfBody { request }.visit_block_root()
}

struct ForLoopConditionOutOfBody<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
}

impl ForLoopConditionOutOfBody<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
        }
        Ok(())
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
            STATEMENT_FOR_LOOP => self.visit_for_loop(statement_id),
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_for_loop(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        self.visit_block(statement.pre_block_id)?;
        self.visit_expression(statement.condition_expression_id)?;
        self.visit_block(statement.post_block_id)?;
        self.visit_block(statement.body_block_id)?;
        self.move_condition_out_of_body(statement_id)
    }

    fn move_condition_out_of_body(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        if !self.request.special_handles.has_boolean_negation {
            return Ok(());
        }

        let statement = self.request.statements[statement_id as usize].clone();
        if !self.literal_is_truthy(statement.condition_expression_id) {
            return Ok(());
        }

        let body_statement_ids = self.request.blocks[statement.body_block_id as usize]
            .statement_ids
            .clone();
        let Some(first_statement_id) = body_statement_ids.first().copied() else {
            return Ok(());
        };

        let first_statement = self.request.statements[first_statement_id as usize].clone();
        if first_statement.kind != STATEMENT_IF {
            return Ok(());
        }

        let if_body_statement_ids = self.request.blocks[first_statement.body_block_id as usize]
            .statement_ids
            .clone();
        let Some(if_first_statement_id) = if_body_statement_ids.first().copied() else {
            return Ok(());
        };
        if self.request.statements[if_first_statement_id as usize].kind != STATEMENT_BREAK {
            return Ok(());
        }

        if !self.expression_movable(first_statement.condition_expression_id)? {
            return Ok(());
        }

        if self.is_boolean_negation_call(first_statement.condition_expression_id) {
            let condition =
                self.request.expressions[first_statement.condition_expression_id as usize].clone();
            let Some(argument_id) = condition.argument_expression_ids.first().copied() else {
                return Ok(());
            };
            self.replace_expression_with_clone_of(statement.condition_expression_id, argument_id)?;
        } else {
            let debug_data_id = self.request.expressions
                [first_statement.condition_expression_id as usize]
                .debug_data_id;
            let cloned_condition_id =
                self.clone_expression_tree(first_statement.condition_expression_id)?;
            self.request.expressions[statement.condition_expression_id as usize] =
                boolean_negation_call(
                    debug_data_id,
                    self.request.special_handles.boolean_negation,
                    cloned_condition_id,
                );
        }

        self.request.blocks[statement.body_block_id as usize]
            .statement_ids
            .remove(0);
        Ok(())
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

    fn expression_movable(&self, expression_id: u64) -> Result<bool, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in &expression.argument_expression_ids {
                    if !self.expression_movable(*argument_id)? {
                        return Ok(false);
                    }
                }

                match expression.function_name_kind {
                    FUNCTION_NAME_BUILTIN => Ok(self
                        .request
                        .builtins
                        .iter()
                        .find(|builtin| {
                            builtin.handle_id == expression.function_name_builtin_handle
                        })
                        .is_some_and(|builtin| builtin.movable)),
                    FUNCTION_NAME_IDENTIFIER => Ok(false),
                    _ => Err(OptimizerError::InvalidWire(format!(
                        "invalid function name kind: {}",
                        expression.function_name_kind
                    ))),
                }
            }
            EXPRESSION_IDENTIFIER | EXPRESSION_LITERAL => Ok(true),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }

    fn literal_is_truthy(&self, expression_id: u64) -> bool {
        let expression = &self.request.expressions[expression_id as usize];
        expression.kind == EXPRESSION_LITERAL
            && !expression.literal_unlimited
            && expression.literal_value.iter().any(|byte| *byte != 0)
    }

    fn is_boolean_negation_call(&self, expression_id: u64) -> bool {
        let expression = &self.request.expressions[expression_id as usize];
        expression.kind == EXPRESSION_FUNCTION_CALL
            && expression.function_name_kind == FUNCTION_NAME_BUILTIN
            && expression.function_name_builtin_handle
                == self.request.special_handles.boolean_negation
    }

    fn clone_expression_tree(&mut self, expression_id: u64) -> Result<u64, OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        let mut cloned = expression.clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            let mut cloned_arguments = Vec::with_capacity(expression.argument_expression_ids.len());
            for argument_id in expression.argument_expression_ids {
                cloned_arguments.push(self.clone_expression_tree(argument_id)?);
            }
            cloned.argument_expression_ids = cloned_arguments;
        }

        let cloned_id = self.request.expressions.len() as u64;
        self.request.expressions.push(cloned);
        Ok(cloned_id)
    }

    fn replace_expression_with_clone_of(
        &mut self,
        expression_id: u64,
        source_id: u64,
    ) -> Result<(), OptimizerError> {
        let cloned_id = self.clone_expression_tree(source_id)?;
        self.request.expressions[expression_id as usize] =
            self.request.expressions[cloned_id as usize].clone();
        Ok(())
    }
}

fn boolean_negation_call(debug_data_id: u64, handle: u64, argument_id: u64) -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_FUNCTION_CALL,
        debug_data_id,
        literal_kind: LITERAL_NUMBER,
        literal_unlimited: false,
        literal_value: vec![0; 32],
        literal_string_id: 0,
        has_literal_hint: false,
        literal_hint_id: 0,
        name_id: 0,
        function_name_kind: FUNCTION_NAME_BUILTIN,
        function_name_debug_data_id: debug_data_id,
        function_name_name_id: 0,
        function_name_builtin_handle: handle,
        argument_expression_ids: vec![argument_id],
    }
}
