use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, FUNCTION_NAME_IDENTIFIER,
    LITERAL_NUMBER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION,
    STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::BTreeMap;

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let assignment_counts = count_assignments(request)?;
    SSAReverser {
        request,
        assignment_counts,
    }
    .visit_block_root()
}

struct SSAReverser<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    assignment_counts: BTreeMap<u64, usize>,
}

impl SSAReverser<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in &statement_ids {
            self.visit_statement(*statement_id)?;
        }

        let mut rewritten = Vec::with_capacity(statement_ids.len());
        let mut index = 0;
        while index < statement_ids.len() {
            if index + 1 < statement_ids.len()
                && self.try_rewrite_pair(statement_ids[index], statement_ids[index + 1])?
            {
                rewritten.push(statement_ids[index]);
                if self.request.statements[statement_ids[index + 1] as usize].kind != 0xff {
                    rewritten.push(statement_ids[index + 1]);
                }
                index += 2;
            } else {
                rewritten.push(statement_ids[index]);
                index += 1;
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten;
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

    fn try_rewrite_pair(&mut self, first_id: u64, second_id: u64) -> Result<bool, OptimizerError> {
        let first = self.request.statements[first_id as usize].clone();
        if first.kind != STATEMENT_VARIABLE_DECLARATION
            || first.variable_ids.len() != 1
            || !first.has_value
        {
            return Ok(false);
        }
        let first_name = self.request.names[first.variable_ids[0] as usize].name_id;

        let second = self.request.statements[second_id as usize].clone();
        if second.kind == STATEMENT_ASSIGNMENT {
            if second.variable_ids.len() != 1 {
                return Ok(false);
            }
            let Some(value_name) = self.identifier_value_name(second.value_expression_id) else {
                return Ok(false);
            };
            if value_name != first_name {
                return Ok(false);
            }

            let target_identifier =
                self.request.identifiers[second.variable_ids[0] as usize].clone();
            if target_identifier.name_id == value_name {
                self.request.statements[second_id as usize].kind = 0xff;
                return Ok(true);
            }

            let new_assignment = assignment_statement(
                second.debug_data_id,
                second.variable_ids,
                first.value_expression_id,
            );
            let new_value_id = self.push_identifier_expression(
                target_identifier.debug_data_id,
                target_identifier.name_id,
            );
            let new_declaration = variable_declaration_statement(
                first.debug_data_id,
                first.variable_ids,
                Some(new_value_id),
            );
            self.request.statements[first_id as usize] = new_assignment;
            self.request.statements[second_id as usize] = new_declaration;
            return Ok(true);
        }

        if second.kind == STATEMENT_VARIABLE_DECLARATION
            && second.variable_ids.len() == 1
            && second.has_value
        {
            let Some(value_name) = self.identifier_value_name(second.value_expression_id) else {
                return Ok(false);
            };
            if value_name != first_name {
                return Ok(false);
            }
            let second_name = self.request.names[second.variable_ids[0] as usize].name_id;
            if self.assignment_count(second_name) <= self.assignment_count(first_name) {
                return Ok(false);
            }

            let second_variable = self.request.names[second.variable_ids[0] as usize].clone();
            let new_first = variable_declaration_statement(
                second.debug_data_id,
                second.variable_ids,
                Some(first.value_expression_id),
            );
            let identifier_id = self
                .push_identifier_expression(second_variable.debug_data_id, second_variable.name_id);
            let new_second = variable_declaration_statement(
                first.debug_data_id,
                first.variable_ids,
                Some(identifier_id),
            );
            self.request.statements[first_id as usize] = new_first;
            self.request.statements[second_id as usize] = new_second;
            return Ok(true);
        }

        Ok(false)
    }

    fn identifier_value_name(&self, expression_id: u64) -> Option<u64> {
        let expression = &self.request.expressions[expression_id as usize];
        (expression.kind == EXPRESSION_IDENTIFIER).then_some(expression.name_id)
    }

    fn assignment_count(&self, name_id: u64) -> usize {
        self.assignment_counts.get(&name_id).copied().unwrap_or(0)
    }

    fn push_identifier_expression(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let expression_id = self.request.expressions.len() as u64;
        self.request
            .expressions
            .push(identifier_expression(debug_data_id, name_id));
        expression_id
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
}

fn count_assignments(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, usize>, OptimizerError> {
    let mut counter = AssignmentCounter {
        request,
        counts: BTreeMap::new(),
    };
    counter.visit_block(request.root_block_id)?;
    Ok(counter.counts)
}

struct AssignmentCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    counts: BTreeMap<u64, usize>,
}

impl AssignmentCounter<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_ASSIGNMENT => {
                for variable_id in &statement.variable_ids {
                    let name = self.request.identifiers[*variable_id as usize].name_id;
                    *self.counts.entry(name).or_default() += 1;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id),
            STATEMENT_IF => self.visit_block(statement.body_block_id),
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    self.visit_block(self.request.cases[*case_id as usize].body_block_id)?;
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
}

fn assignment_statement(
    debug_data_id: u64,
    variable_ids: Vec<u64>,
    value_expression_id: u64,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_ASSIGNMENT,
        debug_data_id,
        expression_id: 0,
        has_value: true,
        value_expression_id,
        variable_ids,
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

fn variable_declaration_statement(
    debug_data_id: u64,
    variable_ids: Vec<u64>,
    value_expression_id: Option<u64>,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_VARIABLE_DECLARATION,
        debug_data_id,
        expression_id: 0,
        has_value: value_expression_id.is_some(),
        value_expression_id: value_expression_id.unwrap_or(0),
        variable_ids,
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
        function_name_kind: FUNCTION_NAME_IDENTIFIER,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}
