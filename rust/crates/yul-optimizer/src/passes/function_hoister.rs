use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let mut hoister = FunctionHoister {
        request,
        functions: Vec::new(),
    };
    hoister.visit_block(hoister.request.root_block_id, true)
}

struct FunctionHoister<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    functions: Vec<u64>,
}

impl FunctionHoister<'_> {
    fn visit_block(&mut self, block_id: u64, top_level: bool) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten_statement_ids = Vec::with_capacity(statement_ids.len());

        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
            if self.request.statements[statement_id as usize].kind == STATEMENT_FUNCTION_DEFINITION
            {
                self.functions.push(statement_id);
            } else if !self.is_empty_block_statement(statement_id) {
                rewritten_statement_ids.push(statement_id);
            }
        }

        if top_level {
            rewritten_statement_ids.append(&mut self.functions);
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten_statement_ids;
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
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id, false),
            STATEMENT_IF => {
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id, false)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(statement.switch_expression_id)?;
                for case_id in statement.case_ids {
                    let switch_case = self.request.cases[case_id as usize].clone();
                    if switch_case.has_value {
                        self.visit_expression(switch_case.value_expression_id)?;
                    }
                    self.visit_block(switch_case.body_block_id, false)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(statement.pre_block_id, false)?;
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.post_block_id, false)?;
                self.visit_block(statement.body_block_id, false)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id, false),
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

    fn is_empty_block_statement(&self, statement_id: u64) -> bool {
        let statement = &self.request.statements[statement_id as usize];
        statement.kind == STATEMENT_BLOCK
            && self.request.blocks[statement.block_id as usize]
                .statement_ids
                .is_empty()
    }
}
