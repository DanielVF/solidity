use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, LITERAL_BOOLEAN, LITERAL_NUMBER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK,
    STATEMENT_BREAK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    ForLoopConditionIntoBody { request }.visit_block_root()
}

struct ForLoopConditionIntoBody<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
}

impl ForLoopConditionIntoBody<'_> {
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
        self.move_condition_into_body(statement_id)?;

        let statement = self.request.statements[statement_id as usize].clone();
        self.visit_block(statement.pre_block_id)?;
        self.visit_expression(statement.condition_expression_id)?;
        self.visit_block(statement.post_block_id)?;
        self.visit_block(statement.body_block_id)
    }

    fn move_condition_into_body(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        if !self.request.special_handles.has_boolean_negation {
            return Ok(());
        }

        let statement = self.request.statements[statement_id as usize].clone();
        let condition =
            self.request.expressions[statement.condition_expression_id as usize].clone();
        if matches!(condition.kind, EXPRESSION_LITERAL | EXPRESSION_IDENTIFIER) {
            return Ok(());
        }

        let debug_data_id = condition.debug_data_id;
        let cloned_condition_id = self.clone_expression_tree(statement.condition_expression_id)?;
        let negated_condition_id = self.request.expressions.len() as u64;
        self.request.expressions.push(boolean_negation_call(
            debug_data_id,
            self.request.special_handles.boolean_negation,
            cloned_condition_id,
        ));

        let break_statement_id = self.request.statements.len() as u64;
        self.request.statements.push(break_statement(u64::MAX));

        let break_body_id = self.request.blocks.len() as u64;
        self.request.blocks.push(ffi::WireBlock {
            debug_data_id,
            statement_ids: vec![break_statement_id],
        });

        let if_statement_id = self.request.statements.len() as u64;
        self.request.statements.push(if_statement(
            debug_data_id,
            negated_condition_id,
            break_body_id,
        ));

        self.request.blocks[statement.body_block_id as usize]
            .statement_ids
            .insert(0, if_statement_id);
        self.request.expressions[statement.condition_expression_id as usize] =
            boolean_literal(debug_data_id, true);

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

fn boolean_literal(debug_data_id: u64, value: bool) -> ffi::WireExpression {
    let mut literal_value = vec![0; 32];
    literal_value[31] = u8::from(value);
    ffi::WireExpression {
        kind: EXPRESSION_LITERAL,
        debug_data_id,
        literal_kind: LITERAL_BOOLEAN,
        literal_unlimited: false,
        literal_value,
        literal_string_id: 0,
        has_literal_hint: false,
        literal_hint_id: 0,
        name_id: 0,
        function_name_kind: FUNCTION_NAME_BUILTIN,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}

fn break_statement(debug_data_id: u64) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_BREAK,
        debug_data_id,
        expression_id: 0,
        has_value: false,
        value_expression_id: 0,
        variable_ids: Vec::new(),
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

fn if_statement(
    debug_data_id: u64,
    condition_expression_id: u64,
    body_block_id: u64,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_IF,
        debug_data_id,
        expression_id: 0,
        has_value: false,
        value_expression_id: 0,
        variable_ids: Vec::new(),
        name_id: 0,
        parameter_ids: Vec::new(),
        return_variable_ids: Vec::new(),
        body_block_id,
        pre_block_id: 0,
        post_block_id: 0,
        condition_expression_id,
        switch_expression_id: 0,
        case_ids: Vec::new(),
        block_id: 0,
    }
}
