use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, LITERAL_STRING, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let inlinable_functions = find_inlinable_functions(request)?;
    ExpressionInliner {
        request,
        inlinable_functions,
    }
    .visit_block_root()
}

struct ExpressionInliner<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    inlinable_functions: BTreeMap<u64, u64>,
}

impl ExpressionInliner<'_> {
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

        self.try_inline_expression(expression_id)
    }

    fn try_inline_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_IDENTIFIER
        {
            return Ok(());
        }

        let Some(function_statement_id) = self
            .inlinable_functions
            .get(&expression.function_name_name_id)
            .copied()
        else {
            return Ok(());
        };

        let function = self.request.statements[function_statement_id as usize].clone();
        if function.parameter_ids.len() != expression.argument_expression_ids.len() {
            return Ok(());
        }

        let mut substitutions = BTreeMap::new();
        for (index, argument_id) in expression.argument_expression_ids.iter().enumerate() {
            let parameter_name = self.request.names[function.parameter_ids[index] as usize].name_id;
            if !self.expression_movable(*argument_id)? {
                return Ok(());
            }

            let references =
                self.count_references_in_block(function.body_block_id, parameter_name)?;
            let cost = self.code_cost(*argument_id)?;
            if references > 1 && cost > 1 {
                return Ok(());
            }

            substitutions.insert(parameter_name, *argument_id);
        }

        let assignment_id = self.request.blocks[function.body_block_id as usize].statement_ids[0];
        let value_expression_id =
            self.request.statements[assignment_id as usize].value_expression_id;
        let replacement_id =
            self.clone_expression_with_substitution(value_expression_id, &substitutions, false)?;
        self.request.expressions[expression_id as usize] =
            self.request.expressions[replacement_id as usize].clone();
        Ok(())
    }

    fn clone_expression_with_substitution(
        &mut self,
        expression_id: u64,
        substitutions: &BTreeMap<u64, u64>,
        substitution_disabled: bool,
    ) -> Result<u64, OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if !substitution_disabled && expression.kind == EXPRESSION_IDENTIFIER {
            if let Some(argument_id) = substitutions.get(&expression.name_id) {
                return self.clone_expression_with_substitution(*argument_id, substitutions, true);
            }
        }

        let mut cloned = expression.clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            cloned.argument_expression_ids = expression
                .argument_expression_ids
                .iter()
                .map(|argument_id| {
                    self.clone_expression_with_substitution(
                        *argument_id,
                        substitutions,
                        substitution_disabled,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
        }

        let cloned_id = self.request.expressions.len() as u64;
        self.request.expressions.push(cloned);
        Ok(cloned_id)
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

    fn count_references_in_block(
        &self,
        block_id: u64,
        name_id: u64,
    ) -> Result<usize, OptimizerError> {
        let mut counter = ReferenceCounter {
            request: self.request,
            name_id,
            count: 0,
        };
        counter.visit_block(block_id)?;
        Ok(counter.count)
    }

    fn code_cost(&self, expression_id: u64) -> Result<usize, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_IDENTIFIER => Ok(1),
            EXPRESSION_LITERAL => Ok(self.literal_cost(expression)),
            EXPRESSION_FUNCTION_CALL => {
                let mut cost = 1isize;
                for argument_id in &expression.argument_expression_ids {
                    cost += self.code_cost(*argument_id)? as isize;
                }

                if expression.function_name_kind == FUNCTION_NAME_BUILTIN {
                    if let Some(builtin) = self.request.builtins.iter().find(|builtin| {
                        builtin.handle_id == expression.function_name_builtin_handle
                    }) {
                        if builtin.has_evm_opcode {
                            cost += evm_instruction_cost_adjustment(builtin.evm_opcode);
                        } else {
                            cost += 49;
                        }
                    } else {
                        cost += 49;
                    }
                } else {
                    cost += 49;
                }
                Ok(cost.max(0) as usize)
            }
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }

    fn literal_cost(&self, expression: &ffi::WireExpression) -> usize {
        if expression.literal_kind == LITERAL_STRING {
            if expression.literal_unlimited {
                1 + self.request.strings[expression.literal_string_id as usize]
                    .bytes
                    .len()
            } else if expression.has_literal_hint {
                1 + self.request.strings[expression.literal_hint_id as usize]
                    .bytes
                    .len()
            } else {
                1 + decimal_len_u256(&expression.literal_value)
            }
        } else {
            let significant_bytes = expression
                .literal_value
                .iter()
                .position(|byte| *byte != 0)
                .map(|position| expression.literal_value.len() - position)
                .unwrap_or(0);
            significant_bytes.max(1)
        }
    }
}

fn evm_instruction_cost_adjustment(opcode: u16) -> isize {
    match opcode {
        0x00
        | 0x30
        | 0x32..=0x34
        | 0x36
        | 0x38
        | 0x3a
        | 0x3d
        | 0x41..=0x46
        | 0x48
        | 0x4a
        | 0x50
        | 0x58..=0x5a
        | 0x5f
        | 0xf3
        | 0xfd
        | 0xfe => -1,
        0x0a
        | 0x20
        | 0x31
        | 0x3b
        | 0x3c
        | 0x3f
        | 0x40
        | 0x54
        | 0x55
        | 0x57
        | 0x5b..=0x5d
        | 0xa0..=0xa4
        | 0xf0..=0xf2
        | 0xf4
        | 0xf5
        | 0xfa
        | 0xff => 49,
        _ => 1,
    }
}

fn decimal_len_u256(value: &[u8]) -> usize {
    let Some(mut work) = u256_bytes(value) else {
        return 1;
    };
    if work.iter().all(|byte| *byte == 0) {
        return 1;
    }

    let mut digits = 0;
    while work.iter().any(|byte| *byte != 0) {
        let mut remainder = 0u16;
        for byte in &mut work {
            let current = remainder * 256 + *byte as u16;
            *byte = (current / 10) as u8;
            remainder = current % 10;
        }
        digits += 1;
    }
    digits
}

fn u256_bytes(value: &[u8]) -> Option<[u8; 32]> {
    if value.len() != 32 {
        return None;
    }
    let mut output = [0; 32];
    output.copy_from_slice(value);
    Some(output)
}

fn find_inlinable_functions(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, u64>, OptimizerError> {
    let mut finder = InlinableExpressionFunctionFinder {
        request,
        inlinable_functions: BTreeMap::new(),
    };
    finder.visit_block(request.root_block_id)?;
    Ok(finder.inlinable_functions)
}

struct InlinableExpressionFunctionFinder<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    inlinable_functions: BTreeMap<u64, u64>,
}

impl InlinableExpressionFunctionFinder<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => {
                self.check_function(statement_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_IF => self.visit_block(statement.body_block_id),
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    self.visit_block(self.request.cases[*case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(statement.pre_block_id)?;
                self.visit_block(statement.body_block_id)?;
                self.visit_block(statement.post_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn check_function(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let function = &self.request.statements[statement_id as usize];
        if function.return_variable_ids.len() != 1
            || self.request.blocks[function.body_block_id as usize]
                .statement_ids
                .len()
                != 1
        {
            return Ok(());
        }

        let return_variable = self.request.names[function.return_variable_ids[0] as usize].name_id;
        let body_statement_id =
            self.request.blocks[function.body_block_id as usize].statement_ids[0];
        let body_statement = &self.request.statements[body_statement_id as usize];
        if body_statement.kind != STATEMENT_ASSIGNMENT || body_statement.variable_ids.len() != 1 {
            return Ok(());
        }

        let assigned_variable =
            self.request.identifiers[body_statement.variable_ids[0] as usize].name_id;
        if assigned_variable != return_variable {
            return Ok(());
        }

        let disallowed = BTreeSet::from([return_variable, function.name_id]);
        if !self.expression_contains_disallowed_identifier(
            body_statement.value_expression_id,
            &disallowed,
        )? {
            self.inlinable_functions
                .insert(function.name_id, statement_id);
        }

        Ok(())
    }

    fn expression_contains_disallowed_identifier(
        &self,
        expression_id: u64,
        disallowed: &BTreeSet<u64>,
    ) -> Result<bool, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_IDENTIFIER => Ok(disallowed.contains(&expression.name_id)),
            EXPRESSION_FUNCTION_CALL => {
                if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER
                    && disallowed.contains(&expression.function_name_name_id)
                {
                    return Ok(true);
                }

                for argument_id in &expression.argument_expression_ids {
                    if self.expression_contains_disallowed_identifier(*argument_id, disallowed)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            EXPRESSION_LITERAL => Ok(false),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }
}

struct ReferenceCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    name_id: u64,
    count: usize,
}

impl ReferenceCounter<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => {
                for variable_id in &statement.variable_ids {
                    if self.request.identifiers[*variable_id as usize].name_id == self.name_id {
                        self.count += 1;
                    }
                }
                self.visit_expression(statement.value_expression_id)
            }
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
                for case_id in &statement.case_ids {
                    let switch_case = &self.request.cases[*case_id as usize];
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
                self.visit_block(statement.body_block_id)?;
                self.visit_block(statement.post_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_IDENTIFIER => {
                if expression.name_id == self.name_id {
                    self.count += 1;
                }
                Ok(())
            }
            EXPRESSION_FUNCTION_CALL => {
                if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER
                    && expression.function_name_name_id == self.name_id
                {
                    self.count += 1;
                }
                for argument_id in &expression.argument_expression_ids {
                    self.visit_expression(*argument_id)?;
                }
                Ok(())
            }
            EXPRESSION_LITERAL => Ok(()),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }
}
