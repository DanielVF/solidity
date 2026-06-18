use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_LITERAL, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let root_block_id = request.root_block_id;
    StructuralSimplifier { request }.visit_block(root_block_id)
}

struct StructuralSimplifier<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
}

impl StructuralSimplifier<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten = Vec::with_capacity(statement_ids.len());

        for statement_id in statement_ids {
            if let Some(mut replacement) = self.try_simplify_statement(statement_id)? {
                rewritten.append(&mut replacement);
            } else {
                self.visit_statement(statement_id)?;
                rewritten.push(statement_id);
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten;
        Ok(())
    }

    fn try_simplify_statement(
        &mut self,
        statement_id: u64,
    ) -> Result<Option<Vec<u64>>, OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_IF => {
                if self.expression_always_true(statement.condition_expression_id) {
                    self.visit_block(statement.body_block_id)?;
                    Ok(Some(
                        self.request.blocks[statement.body_block_id as usize]
                            .statement_ids
                            .clone(),
                    ))
                } else if self.expression_always_false(statement.condition_expression_id) {
                    Ok(Some(Vec::new()))
                } else {
                    Ok(None)
                }
            }
            STATEMENT_SWITCH => {
                let Some(switch_value) = self.literal_value(statement.switch_expression_id) else {
                    return Ok(None);
                };

                let mut default_body = None;
                for case_id in statement.case_ids {
                    let switch_case = self.request.cases[case_id as usize].clone();
                    if switch_case.has_value {
                        if self
                            .literal_value(switch_case.value_expression_id)
                            .as_deref()
                            == Some(switch_value.as_slice())
                        {
                            self.visit_block(switch_case.body_block_id)?;
                            return Ok(Some(
                                self.request.blocks[switch_case.body_block_id as usize]
                                    .statement_ids
                                    .clone(),
                            ));
                        }
                    } else {
                        default_body = Some(switch_case.body_block_id);
                    }
                }

                if let Some(body_block_id) = default_body {
                    self.visit_block(body_block_id)?;
                    Ok(Some(
                        self.request.blocks[body_block_id as usize]
                            .statement_ids
                            .clone(),
                    ))
                } else {
                    Ok(Some(Vec::new()))
                }
            }
            STATEMENT_FOR_LOOP => {
                if self.expression_always_false(statement.condition_expression_id) {
                    self.visit_block(statement.pre_block_id)?;
                    Ok(Some(
                        self.request.blocks[statement.pre_block_id as usize]
                            .statement_ids
                            .clone(),
                    ))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
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

    fn expression_always_true(&self, expression_id: u64) -> bool {
        self.literal_value(expression_id)
            .is_some_and(|value| value.iter().any(|byte| *byte != 0))
    }

    fn expression_always_false(&self, expression_id: u64) -> bool {
        self.literal_value(expression_id)
            .is_some_and(|value| value.iter().all(|byte| *byte == 0))
    }

    fn literal_value(&self, expression_id: u64) -> Option<Vec<u8>> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_LITERAL && !expression.literal_unlimited {
            Some(expression.literal_value.clone())
        } else {
            None
        }
    }
}
