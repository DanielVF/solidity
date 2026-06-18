use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_LITERAL, LITERAL_NUMBER, STATEMENT_BLOCK, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let root_block_id = request.root_block_id;
    VarDeclInitializer { request }.visit_block(root_block_id)
}

struct VarDeclInitializer<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
}

impl VarDeclInitializer<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in &statement_ids {
            self.visit_statement(*statement_id)?;
        }

        let mut rewritten = Vec::with_capacity(statement_ids.len());
        for statement_id in statement_ids {
            let statement = self.request.statements[statement_id as usize].clone();
            if statement.kind != STATEMENT_VARIABLE_DECLARATION || statement.has_value {
                rewritten.push(statement_id);
                continue;
            }

            if statement.variable_ids.len() <= 1 {
                let value_expression_id = self.push_zero_literal(statement.debug_data_id);
                let statement = &mut self.request.statements[statement_id as usize];
                statement.has_value = true;
                statement.value_expression_id = value_expression_id;
                rewritten.push(statement_id);
            } else {
                for variable_id in statement.variable_ids {
                    let value_expression_id = self.push_zero_literal(statement.debug_data_id);
                    let new_statement_id = self.request.statements.len() as u64;
                    self.request.statements.push(variable_declaration_statement(
                        statement.debug_data_id,
                        variable_id,
                        value_expression_id,
                    ));
                    rewritten.push(new_statement_id);
                }
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten;
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id),
            STATEMENT_IF => self.visit_block(statement.body_block_id),
            STATEMENT_SWITCH => {
                for case_id in statement.case_ids {
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(statement.pre_block_id)?;
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn push_zero_literal(&mut self, debug_data_id: u64) -> u64 {
        let id = self.request.expressions.len() as u64;
        self.request.expressions.push(zero_literal(debug_data_id));
        id
    }
}

fn variable_declaration_statement(
    debug_data_id: u64,
    variable_id: u64,
    value_expression_id: u64,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_VARIABLE_DECLARATION,
        debug_data_id,
        expression_id: 0,
        has_value: true,
        value_expression_id,
        variable_ids: vec![variable_id],
        name_id: 0,
        parameter_ids: Vec::new(),
        return_variable_ids: Vec::new(),
        body_block_id: 0,
        pre_block_id: 0,
        post_block_id: 0,
        condition_expression_id: 0,
        switch_expression_id: 0,
        case_ids: Vec::new(),
        block_id: 0,
    }
}

fn zero_literal(debug_data_id: u64) -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_LITERAL,
        debug_data_id,
        literal_kind: LITERAL_NUMBER,
        literal_unlimited: false,
        literal_value: vec![0; 32],
        literal_string_id: 0,
        has_literal_hint: false,
        literal_hint_id: 0,
        name_id: 0,
        function_name_kind: 0,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}
