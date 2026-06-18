use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, FUNCTION_NAME_BUILTIN,
    LITERAL_NUMBER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION,
    STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
};

const LITERAL_ARGUMENT_UNRESTRICTED: u8 = 0xff;

pub fn run(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
) -> Result<(), OptimizerError> {
    let mut splitter = ExpressionSplitter {
        request,
        name_dispenser,
        statements_to_prefix: Vec::new(),
    };
    splitter.visit_block(splitter.request.root_block_id)
}

struct ExpressionSplitter<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    name_dispenser: &'b mut super::NameDispenser,
    statements_to_prefix: Vec<u64>,
}

impl ExpressionSplitter<'_, '_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let saved_prefix = std::mem::take(&mut self.statements_to_prefix);
        let original_statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut replacement_statement_ids = Vec::with_capacity(original_statement_ids.len());

        for statement_id in original_statement_ids {
            self.statements_to_prefix.clear();
            self.visit_statement(statement_id)?;

            if self.statements_to_prefix.is_empty() {
                replacement_statement_ids.push(statement_id);
            } else {
                self.statements_to_prefix.push(statement_id);
                replacement_statement_ids.append(&mut self.statements_to_prefix);
            }
        }

        self.request.blocks[block_id as usize].statement_ids = replacement_statement_ids;
        self.statements_to_prefix = saved_prefix;
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
                self.outline_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_SWITCH => {
                self.outline_expression(statement.switch_expression_id)?;
                for case_id in statement.case_ids {
                    let case_body_id = self.request.cases[case_id as usize].body_block_id;
                    self.visit_block(case_body_id)?;
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

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_FUNCTION_CALL {
            return Ok(());
        }

        let literal_argument_kinds = if expression.function_name_kind == FUNCTION_NAME_BUILTIN {
            self.builtin(expression.function_name_builtin_handle)
                .map(|builtin| builtin.literal_argument_kinds.clone())
        } else {
            None
        };

        for index in (0..expression.argument_expression_ids.len()).rev() {
            if literal_argument_kinds
                .as_ref()
                .is_some_and(|kinds| builtin_argument_must_stay_literal(kinds, index))
            {
                continue;
            }

            self.outline_expression(expression.argument_expression_ids[index])?;
        }

        Ok(())
    }

    fn outline_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        if self.request.expressions[expression_id as usize].kind == EXPRESSION_IDENTIFIER {
            return Ok(());
        }

        self.visit_expression(expression_id)?;

        let debug_data_id = self.request.expressions[expression_id as usize].debug_data_id;
        let value_expression_id = self.request.expressions.len() as u64;
        self.request
            .expressions
            .push(self.request.expressions[expression_id as usize].clone());

        let name_id = self.name_dispenser.new_name(self.request);
        let variable_id = self.request.names.len() as u64;
        self.request.names.push(ffi::WireNameWithDebugData {
            debug_data_id,
            name_id,
        });

        let statement_id = self.request.statements.len() as u64;
        self.request.statements.push(variable_declaration(
            debug_data_id,
            variable_id,
            value_expression_id,
        ));
        self.statements_to_prefix.push(statement_id);

        self.request.expressions[expression_id as usize] =
            identifier_expression(debug_data_id, name_id);
        Ok(())
    }
    fn builtin(&self, handle: u64) -> Option<&ffi::WireBuiltinFunction> {
        self.request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == handle)
    }
}

fn builtin_argument_must_stay_literal(literal_argument_kinds: &[u8], index: usize) -> bool {
    literal_argument_kinds
        .get(index)
        .is_some_and(|kind| *kind != LITERAL_ARGUMENT_UNRESTRICTED)
}

fn variable_declaration(
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

fn identifier_expression(debug_data_id: u64, name_id: u64) -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_IDENTIFIER,
        debug_data_id,
        literal_kind: LITERAL_NUMBER,
        literal_unlimited: false,
        literal_value: vec![0; 32],
        literal_string_id: 0,
        has_literal_hint: false,
        literal_hint_id: 0,
        name_id,
        function_name_kind: FUNCTION_NAME_BUILTIN,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}
