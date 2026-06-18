use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, LITERAL_STRING,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

const ROOT_CALLSITE: u64 = u64::MAX;

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlinePass {
    Tiny,
    Rest,
}

pub fn run(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
) -> Result<(), OptimizerError> {
    let functions = collect_root_functions(request);
    let references = count_references(request)?;
    let no_inline_functions = collect_no_inline_functions(request, &functions)?;
    let recursive_functions = collect_recursive_functions(request, &functions)?;
    let function_sizes = collect_function_sizes(request, &functions)?;
    let constants = collect_constants(request)?;
    let has_memory_guard = has_memory_guard_call(request)?;
    let single_use_functions = functions
        .keys()
        .copied()
        .filter(|function| references.get(function).copied().unwrap_or(0) == 1)
        .collect();

    let mut inliner = FullInliner {
        request,
        name_dispenser,
        functions,
        no_inline_functions,
        recursive_functions,
        single_use_functions,
        constants,
        function_sizes,
        has_memory_guard,
    };

    inliner.run_pass(InlinePass::Tiny)?;
    inliner.run_pass(InlinePass::Rest)
}

struct FullInliner<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    name_dispenser: &'b mut super::NameDispenser,
    functions: BTreeMap<u64, u64>,
    no_inline_functions: BTreeSet<u64>,
    recursive_functions: BTreeSet<u64>,
    single_use_functions: BTreeSet<u64>,
    constants: BTreeSet<u64>,
    function_sizes: BTreeMap<u64, usize>,
    has_memory_guard: bool,
}

impl FullInliner<'_, '_> {
    fn run_pass(&mut self, pass: InlinePass) -> Result<(), OptimizerError> {
        let root_statement_ids = self.request.blocks[self.request.root_block_id as usize]
            .statement_ids
            .clone();

        let depths = self.call_depths()?;
        let mut functions = Vec::new();
        for statement_id in root_statement_ids.iter().copied() {
            let statement = self.request.statements[statement_id as usize].clone();
            if statement.kind == STATEMENT_FUNCTION_DEFINITION {
                functions.push((statement_id, statement.name_id, statement.body_block_id));
            }
        }
        functions.sort_by_key(|(_, name, _)| depths.get(name).copied().unwrap_or(usize::MAX));

        for (_, name, body_block_id) in functions {
            self.handle_block(Some(name), body_block_id, pass)?;
            let size = self.code_size_block(body_block_id)?;
            self.function_sizes.insert(name, size);
        }

        for statement_id in root_statement_ids {
            let statement = self.request.statements[statement_id as usize].clone();
            if statement.kind == STATEMENT_BLOCK {
                self.handle_block(None, statement.block_id, pass)?;
            }
        }

        Ok(())
    }

    fn handle_block(
        &mut self,
        current_function: Option<u64>,
        block_id: u64,
        pass: InlinePass,
    ) -> Result<(), OptimizerError> {
        let original_statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten_statement_ids = Vec::with_capacity(original_statement_ids.len());

        for statement_id in original_statement_ids {
            self.visit_statement(current_function, statement_id, pass)?;
            if let Some(inlined) =
                self.try_inline_statement(current_function, statement_id, pass)?
            {
                rewritten_statement_ids.extend(inlined);
            } else {
                rewritten_statement_ids.push(statement_id);
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten_statement_ids;
        Ok(())
    }

    fn visit_statement(
        &mut self,
        current_function: Option<u64>,
        statement_id: u64,
        pass: InlinePass,
    ) -> Result<(), OptimizerError> {
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
            STATEMENT_FUNCTION_DEFINITION => {
                self.handle_block(Some(statement.name_id), statement.body_block_id, pass)
            }
            STATEMENT_IF => {
                self.visit_expression(statement.condition_expression_id)?;
                self.handle_block(current_function, statement.body_block_id, pass)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(statement.switch_expression_id)?;
                for case_id in statement.case_ids {
                    let switch_case = self.request.cases[case_id as usize].clone();
                    if switch_case.has_value {
                        self.visit_expression(switch_case.value_expression_id)?;
                    }
                    self.handle_block(current_function, switch_case.body_block_id, pass)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.handle_block(current_function, statement.pre_block_id, pass)?;
                self.visit_expression(statement.condition_expression_id)?;
                self.handle_block(current_function, statement.post_block_id, pass)?;
                self.handle_block(current_function, statement.body_block_id, pass)
            }
            STATEMENT_BLOCK => self.handle_block(current_function, statement.block_id, pass),
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

    fn try_inline_statement(
        &mut self,
        current_function: Option<u64>,
        statement_id: u64,
        pass: InlinePass,
    ) -> Result<Option<Vec<u64>>, OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        let expression_id = match statement.kind {
            STATEMENT_EXPRESSION => statement.expression_id,
            STATEMENT_ASSIGNMENT => statement.value_expression_id,
            STATEMENT_VARIABLE_DECLARATION if statement.has_value => statement.value_expression_id,
            _ => return Ok(None),
        };

        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_IDENTIFIER
            || !self.shall_inline(&expression, current_function, pass)?
        {
            return Ok(None);
        }

        Ok(Some(self.perform_inline(
            current_function,
            statement_id,
            expression_id,
            expression.function_name_name_id,
        )?))
    }

    fn shall_inline(
        &self,
        function_call: &ffi::WireExpression,
        current_function: Option<u64>,
        pass: InlinePass,
    ) -> Result<bool, OptimizerError> {
        let function_name = function_call.function_name_name_id;
        if current_function == Some(function_name) {
            return Ok(false);
        }

        if !self.functions.contains_key(&function_name)
            || self.no_inline_functions.contains(&function_name)
            || self.function_directly_recursive(function_name)?
        {
            return Ok(false);
        }

        for argument_id in &function_call.argument_expression_ids {
            let argument = &self.request.expressions[*argument_id as usize];
            if !matches!(argument.kind, EXPRESSION_LITERAL | EXPRESSION_IDENTIFIER) {
                return Ok(false);
            }
        }

        let size = self
            .function_sizes
            .get(&function_name)
            .copied()
            .unwrap_or(usize::MAX);
        if size <= 1 {
            return Ok(true);
        }
        if pass == InlinePass::Tiny {
            return Ok(false);
        }

        let callsite = current_function.unwrap_or(ROOT_CALLSITE);
        let mut aggressive_inlining =
            self.request.dialect.provides_object_access && self.request.dialect.evm_version > 0;

        if !self.has_memory_guard || self.recursive_functions.contains(&callsite) {
            aggressive_inlining = false;
        }

        if !aggressive_inlining
            && self
                .function_sizes
                .get(&callsite)
                .copied()
                .unwrap_or(usize::MAX)
                > 45
        {
            return Ok(false);
        }

        if self.single_use_functions.contains(&function_name) {
            return Ok(true);
        }

        let constant_arg = function_call
            .argument_expression_ids
            .iter()
            .any(|argument_id| {
                let argument = &self.request.expressions[*argument_id as usize];
                argument.kind == EXPRESSION_LITERAL
                    || (argument.kind == EXPRESSION_IDENTIFIER
                        && self.constants.contains(&argument.name_id))
            });

        Ok(size < if aggressive_inlining { 8 } else { 6 }
            || (constant_arg && size < if aggressive_inlining { 16 } else { 12 }))
    }

    fn function_directly_recursive(&self, function_name: u64) -> Result<bool, OptimizerError> {
        let Some(statement_id) = self.functions.get(&function_name) else {
            return Ok(false);
        };
        let body_block_id = self.request.statements[*statement_id as usize].body_block_id;
        block_contains_call_to(self.request, body_block_id, function_name)
    }

    fn perform_inline(
        &mut self,
        current_function: Option<u64>,
        statement_id: u64,
        function_call_id: u64,
        function_name: u64,
    ) -> Result<Vec<u64>, OptimizerError> {
        let call = self.request.expressions[function_call_id as usize].clone();
        let statement = self.request.statements[statement_id as usize].clone();
        let function_statement_id = self.functions[&function_name];
        let function = self.request.statements[function_statement_id as usize].clone();
        let return_count = function.return_variable_ids.len();
        let callsite = current_function.unwrap_or(ROOT_CALLSITE);
        let called_size = self
            .function_sizes
            .get(&function_name)
            .copied()
            .unwrap_or(0);
        *self.function_sizes.entry(callsite).or_default() += called_size;

        if statement.kind == STATEMENT_ASSIGNMENT && statement.variable_ids.len() != return_count {
            return Ok(vec![statement_id]);
        }
        if statement.kind == STATEMENT_VARIABLE_DECLARATION
            && statement.variable_ids.len() != return_count
        {
            return Ok(vec![statement_id]);
        }

        let mut new_statements = Vec::new();
        let mut replacements = BTreeMap::new();

        for (parameter_id, argument_id) in function
            .parameter_ids
            .iter()
            .zip(&call.argument_expression_ids)
            .rev()
        {
            let argument_value_id =
                self.clone_expression_with_replacements(*argument_id, &BTreeMap::new())?;
            self.new_variable_for_existing(
                *parameter_id,
                call.debug_data_id,
                Some(argument_value_id),
                &mut replacements,
                &mut new_statements,
            );
        }

        for return_variable_id in &function.return_variable_ids {
            let zero_id = self.push_zero_literal(u64::MAX);
            self.new_variable_for_existing(
                *return_variable_id,
                call.debug_data_id,
                Some(zero_id),
                &mut replacements,
                &mut new_statements,
            );
        }

        let body_statement_ids = self.request.blocks[function.body_block_id as usize]
            .statement_ids
            .clone();
        for body_statement_id in body_statement_ids {
            let cloned_statement =
                self.clone_statement_with_replacements(body_statement_id, &mut replacements)?;
            new_statements.push(cloned_statement);
        }

        match statement.kind {
            STATEMENT_ASSIGNMENT => {
                for (index, target_id) in statement.variable_ids.iter().enumerate() {
                    let return_name =
                        self.request.names[function.return_variable_ids[index] as usize].name_id;
                    let value_name = replacements[&return_name];
                    let value_id =
                        self.push_identifier_expression(statement.debug_data_id, value_name);
                    let variable_id = self.clone_identifier(*target_id, &BTreeMap::new());
                    let assignment_id = self.request.statements.len() as u64;
                    self.request.statements.push(assignment_statement(
                        statement.debug_data_id,
                        vec![variable_id],
                        value_id,
                    ));
                    new_statements.push(assignment_id);
                }
            }
            STATEMENT_VARIABLE_DECLARATION => {
                for (index, target_id) in statement.variable_ids.iter().enumerate() {
                    let return_name =
                        self.request.names[function.return_variable_ids[index] as usize].name_id;
                    let value_name = replacements[&return_name];
                    let value_id =
                        self.push_identifier_expression(statement.debug_data_id, value_name);
                    let variable_id = self.clone_name_with_debug_data(*target_id, &BTreeMap::new());
                    let declaration_id = self.request.statements.len() as u64;
                    self.request.statements.push(variable_declaration_statement(
                        statement.debug_data_id,
                        vec![variable_id],
                        Some(value_id),
                    ));
                    new_statements.push(declaration_id);
                }
            }
            _ => {}
        }

        Ok(new_statements)
    }

    fn new_variable_for_existing(
        &mut self,
        existing_variable_id: u64,
        debug_data_id: u64,
        value_expression_id: Option<u64>,
        replacements: &mut BTreeMap<u64, u64>,
        new_statements: &mut Vec<u64>,
    ) {
        let existing_name = self.request.names[existing_variable_id as usize].name_id;
        let new_name = self.new_name_like(existing_name);
        replacements.insert(existing_name, new_name);

        let variable_id = self.request.names.len() as u64;
        self.request.names.push(ffi::WireNameWithDebugData {
            debug_data_id,
            name_id: new_name,
        });

        let statement_id = self.request.statements.len() as u64;
        self.request.statements.push(variable_declaration_statement(
            debug_data_id,
            vec![variable_id],
            value_expression_id,
        ));
        new_statements.push(statement_id);
    }

    fn clone_statement_with_replacements(
        &mut self,
        statement_id: u64,
        replacements: &mut BTreeMap<u64, u64>,
    ) -> Result<u64, OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        let cloned = match statement.kind {
            STATEMENT_EXPRESSION => {
                let expression_id =
                    self.clone_expression_with_replacements(statement.expression_id, replacements)?;
                expression_statement(statement.debug_data_id, expression_id)
            }
            STATEMENT_ASSIGNMENT => {
                let variables = statement
                    .variable_ids
                    .iter()
                    .map(|identifier_id| self.clone_identifier(*identifier_id, replacements))
                    .collect();
                let value_id = self.clone_expression_with_replacements(
                    statement.value_expression_id,
                    replacements,
                )?;
                assignment_statement(statement.debug_data_id, variables, value_id)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                let mut variables = Vec::with_capacity(statement.variable_ids.len());
                for variable_id in &statement.variable_ids {
                    let old_name = self.request.names[*variable_id as usize].name_id;
                    let new_name = self.new_name_like(old_name);
                    replacements.insert(old_name, new_name);
                    let cloned_variable_id = self.request.names.len() as u64;
                    self.request.names.push(ffi::WireNameWithDebugData {
                        debug_data_id: self.request.names[*variable_id as usize].debug_data_id,
                        name_id: new_name,
                    });
                    variables.push(cloned_variable_id);
                }
                let value_id = if statement.has_value {
                    Some(self.clone_expression_with_replacements(
                        statement.value_expression_id,
                        replacements,
                    )?)
                } else {
                    None
                };
                variable_declaration_statement(statement.debug_data_id, variables, value_id)
            }
            STATEMENT_FUNCTION_DEFINITION => {
                return Err(OptimizerError::InvalidWire(
                    "FullInliner requires FunctionHoister before inlining.".to_string(),
                ));
            }
            STATEMENT_IF => {
                let condition_id = self.clone_expression_with_replacements(
                    statement.condition_expression_id,
                    replacements,
                )?;
                let body_id =
                    self.clone_block_with_replacements(statement.body_block_id, replacements)?;
                if_statement(statement.debug_data_id, condition_id, body_id)
            }
            STATEMENT_SWITCH => {
                let switch_expression_id = self.clone_expression_with_replacements(
                    statement.switch_expression_id,
                    replacements,
                )?;
                let mut case_ids = Vec::with_capacity(statement.case_ids.len());
                for case_id in statement.case_ids {
                    case_ids.push(self.clone_case_with_replacements(case_id, replacements)?);
                }
                switch_statement(statement.debug_data_id, switch_expression_id, case_ids)
            }
            STATEMENT_FOR_LOOP => {
                let pre_block_id =
                    self.clone_block_with_replacements(statement.pre_block_id, replacements)?;
                let condition_expression_id = self.clone_expression_with_replacements(
                    statement.condition_expression_id,
                    replacements,
                )?;
                let post_block_id =
                    self.clone_block_with_replacements(statement.post_block_id, replacements)?;
                let body_block_id =
                    self.clone_block_with_replacements(statement.body_block_id, replacements)?;
                for_loop_statement(
                    statement.debug_data_id,
                    pre_block_id,
                    condition_expression_id,
                    post_block_id,
                    body_block_id,
                )
            }
            STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => statement,
            STATEMENT_BLOCK => {
                let block_id =
                    self.clone_block_with_replacements(statement.block_id, replacements)?;
                block_statement(statement.debug_data_id, block_id)
            }
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid statement kind: {}",
                    statement.kind
                )));
            }
        };

        let cloned_id = self.request.statements.len() as u64;
        self.request.statements.push(cloned);
        Ok(cloned_id)
    }

    fn clone_block_with_replacements(
        &mut self,
        block_id: u64,
        replacements: &mut BTreeMap<u64, u64>,
    ) -> Result<u64, OptimizerError> {
        let block = self.request.blocks[block_id as usize].clone();
        let mut cloned_statement_ids = Vec::with_capacity(block.statement_ids.len());
        for statement_id in block.statement_ids {
            cloned_statement_ids
                .push(self.clone_statement_with_replacements(statement_id, replacements)?);
        }

        let cloned_block_id = self.request.blocks.len() as u64;
        self.request.blocks.push(ffi::WireBlock {
            debug_data_id: block.debug_data_id,
            statement_ids: cloned_statement_ids,
        });
        Ok(cloned_block_id)
    }

    fn clone_case_with_replacements(
        &mut self,
        case_id: u64,
        replacements: &mut BTreeMap<u64, u64>,
    ) -> Result<u64, OptimizerError> {
        let switch_case = self.request.cases[case_id as usize].clone();
        let value_expression_id = if switch_case.has_value {
            self.clone_expression_with_replacements(switch_case.value_expression_id, replacements)?
        } else {
            0
        };
        let body_block_id =
            self.clone_block_with_replacements(switch_case.body_block_id, replacements)?;
        let cloned_case_id = self.request.cases.len() as u64;
        self.request.cases.push(ffi::WireCase {
            debug_data_id: switch_case.debug_data_id,
            has_value: switch_case.has_value,
            value_expression_id,
            body_block_id,
        });
        Ok(cloned_case_id)
    }

    fn clone_expression_with_replacements(
        &mut self,
        expression_id: u64,
        replacements: &BTreeMap<u64, u64>,
    ) -> Result<u64, OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        let mut cloned = expression.clone();
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER {
                    cloned.function_name_name_id = replacements
                        .get(&expression.function_name_name_id)
                        .copied()
                        .unwrap_or(expression.function_name_name_id);
                }
                let mut arguments = Vec::with_capacity(expression.argument_expression_ids.len());
                for argument_id in expression.argument_expression_ids {
                    arguments
                        .push(self.clone_expression_with_replacements(argument_id, replacements)?);
                }
                cloned.argument_expression_ids = arguments;
            }
            EXPRESSION_IDENTIFIER => {
                cloned.name_id = replacements
                    .get(&expression.name_id)
                    .copied()
                    .unwrap_or(expression.name_id);
            }
            EXPRESSION_LITERAL => {}
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid expression kind: {}",
                    expression.kind
                )));
            }
        }

        let cloned_id = self.request.expressions.len() as u64;
        self.request.expressions.push(cloned);
        Ok(cloned_id)
    }

    fn clone_identifier(&mut self, identifier_id: u64, replacements: &BTreeMap<u64, u64>) -> u64 {
        let identifier = self.request.identifiers[identifier_id as usize].clone();
        let cloned_id = self.request.identifiers.len() as u64;
        self.request.identifiers.push(ffi::WireIdentifier {
            debug_data_id: identifier.debug_data_id,
            name_id: replacements
                .get(&identifier.name_id)
                .copied()
                .unwrap_or(identifier.name_id),
        });
        cloned_id
    }

    fn clone_name_with_debug_data(
        &mut self,
        name_id: u64,
        replacements: &BTreeMap<u64, u64>,
    ) -> u64 {
        let name = self.request.names[name_id as usize].clone();
        let cloned_id = self.request.names.len() as u64;
        self.request.names.push(ffi::WireNameWithDebugData {
            debug_data_id: name.debug_data_id,
            name_id: replacements
                .get(&name.name_id)
                .copied()
                .unwrap_or(name.name_id),
        });
        cloned_id
    }

    fn push_identifier_expression(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let expression_id = self.request.expressions.len() as u64;
        self.request
            .expressions
            .push(identifier_expression(debug_data_id, name_id));
        expression_id
    }

    fn push_zero_literal(&mut self, debug_data_id: u64) -> u64 {
        let expression_id = self.request.expressions.len() as u64;
        self.request.expressions.push(zero_literal(debug_data_id));
        expression_id
    }

    fn new_name_like(&mut self, name_id: u64) -> u64 {
        self.name_dispenser.new_name_like(self.request, name_id)
    }

    fn code_size_block(&self, block_id: u64) -> Result<usize, OptimizerError> {
        let mut size = 0;
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            size += self.code_size_statement(*statement_id)?;
        }
        Ok(size)
    }

    fn code_size_statement(&self, statement_id: u64) -> Result<usize, OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        let size = match statement.kind {
            STATEMENT_EXPRESSION => self.code_size_expression(statement.expression_id)?,
            STATEMENT_ASSIGNMENT => self.code_size_expression(statement.value_expression_id)?,
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.code_size_expression(statement.value_expression_id)?
                } else {
                    0
                }
            }
            STATEMENT_FUNCTION_DEFINITION => 0,
            STATEMENT_IF => {
                2 + self.code_size_expression(statement.condition_expression_id)?
                    + self.code_size_block(statement.body_block_id)?
            }
            STATEMENT_SWITCH => {
                let mut size = 1
                    + 2 * statement.case_ids.len()
                    + self.code_size_expression(statement.switch_expression_id)?;
                for case_id in &statement.case_ids {
                    let switch_case = &self.request.cases[*case_id as usize];
                    size += self.code_size_block(switch_case.body_block_id)?;
                }
                size
            }
            STATEMENT_FOR_LOOP => {
                3 + self.code_size_block(statement.pre_block_id)?
                    + self.code_size_expression(statement.condition_expression_id)?
                    + self.code_size_block(statement.post_block_id)?
                    + self.code_size_block(statement.body_block_id)?
            }
            STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => 2,
            STATEMENT_BLOCK => self.code_size_block(statement.block_id)?,
            _ => 0,
        };
        Ok(size)
    }

    fn code_size_expression(&self, expression_id: u64) -> Result<usize, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                let mut size = 1;
                for argument_id in &expression.argument_expression_ids {
                    size += self.code_size_expression(*argument_id)?;
                }
                Ok(size)
            }
            EXPRESSION_IDENTIFIER => Ok(0),
            EXPRESSION_LITERAL => {
                if literal_is_non_string_zero(expression) {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }

    fn call_depths(&self) -> Result<BTreeMap<u64, usize>, OptimizerError> {
        let mut graph = collect_function_call_graph(self.request, &self.functions)?;
        for callees in graph.values_mut() {
            callees.retain(|callee| self.functions.contains_key(callee));
        }

        let mut depths = BTreeMap::new();
        let mut current_depth = 0;

        loop {
            let removed: Vec<u64> = graph
                .iter()
                .filter_map(|(function, callees)| callees.is_empty().then_some(*function))
                .collect();

            if removed.is_empty() {
                break;
            }

            for function in &removed {
                graph.remove(function);
                depths.insert(*function, current_depth);
            }

            for callees in graph.values_mut() {
                for removed_function in &removed {
                    callees.remove(removed_function);
                }
            }

            current_depth += 1;
        }

        for function in graph.keys() {
            depths.insert(*function, current_depth);
        }

        Ok(depths)
    }
}

fn collect_root_functions(request: &ffi::WireYulOptimizerRequest) -> BTreeMap<u64, u64> {
    let mut functions = BTreeMap::new();
    for statement_id in &request.blocks[request.root_block_id as usize].statement_ids {
        let statement = &request.statements[*statement_id as usize];
        if statement.kind == STATEMENT_FUNCTION_DEFINITION {
            functions.insert(statement.name_id, *statement_id);
        }
    }
    functions
}

fn collect_no_inline_functions(
    request: &ffi::WireYulOptimizerRequest,
    functions: &BTreeMap<u64, u64>,
) -> Result<BTreeSet<u64>, OptimizerError> {
    let mut output = BTreeSet::new();
    for (function_name, statement_id) in functions {
        let body_block_id = request.statements[*statement_id as usize].body_block_id;
        if contains_leave(request, body_block_id)? {
            output.insert(*function_name);
        }
    }
    Ok(output)
}

fn collect_recursive_functions(
    request: &ffi::WireYulOptimizerRequest,
    functions: &BTreeMap<u64, u64>,
) -> Result<BTreeSet<u64>, OptimizerError> {
    let graph = collect_function_call_graph(request, functions)?;
    let mut output = BTreeSet::new();
    for function_name in functions.keys() {
        if reaches_function(&graph, *function_name, *function_name, &mut BTreeSet::new()) {
            output.insert(*function_name);
        }
    }
    Ok(output)
}

fn collect_function_sizes(
    request: &ffi::WireYulOptimizerRequest,
    functions: &BTreeMap<u64, u64>,
) -> Result<BTreeMap<u64, usize>, OptimizerError> {
    let mut output = BTreeMap::new();
    let counter = CodeSizeCounter { request };
    output.insert(ROOT_CALLSITE, counter.block(request.root_block_id)?);
    for (function_name, statement_id) in functions {
        let body_block_id = request.statements[*statement_id as usize].body_block_id;
        output.insert(*function_name, counter.block(body_block_id)?);
    }
    Ok(output)
}

fn collect_constants(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeSet<u64>, OptimizerError> {
    let mut tracker = SsaValueTracker {
        request,
        values: BTreeMap::new(),
    };
    tracker.visit_block(request.root_block_id)?;

    Ok(tracker
        .values
        .into_iter()
        .filter_map(|(name, value)| match value {
            SsaValue::Zero => Some(name),
            SsaValue::Expression(expression_id)
                if request.expressions[expression_id as usize].kind == EXPRESSION_LITERAL =>
            {
                Some(name)
            }
            _ => None,
        })
        .collect())
}

fn has_memory_guard_call(request: &ffi::WireYulOptimizerRequest) -> Result<bool, OptimizerError> {
    if !request.special_handles.has_memoryguard {
        return Ok(false);
    }
    let mut finder = BuiltinCallFinder {
        request,
        handle: request.special_handles.memoryguard,
        found: false,
    };
    finder.visit_block(request.root_block_id)?;
    Ok(finder.found)
}

fn collect_function_call_graph(
    request: &ffi::WireYulOptimizerRequest,
    functions: &BTreeMap<u64, u64>,
) -> Result<BTreeMap<u64, BTreeSet<u64>>, OptimizerError> {
    let mut graph = BTreeMap::new();
    for (function_name, statement_id) in functions {
        let mut collector = FunctionCallCollector {
            request,
            calls: BTreeSet::new(),
        };
        collector.visit_block(request.statements[*statement_id as usize].body_block_id)?;
        graph.insert(*function_name, collector.calls);
    }
    Ok(graph)
}

fn reaches_function(
    graph: &BTreeMap<u64, BTreeSet<u64>>,
    current: u64,
    target: u64,
    visited: &mut BTreeSet<u64>,
) -> bool {
    let Some(callees) = graph.get(&current) else {
        return false;
    };

    for callee in callees {
        if *callee == target {
            return true;
        }
        if visited.insert(*callee) && reaches_function(graph, *callee, target, visited) {
            return true;
        }
    }
    false
}

fn count_references(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, usize>, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: BTreeMap::new(),
    };
    counter.visit_block(request.root_block_id)?;
    Ok(counter.references)
}

struct ReferenceCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    references: BTreeMap<u64, usize>,
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
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER {
                *self
                    .references
                    .entry(expression.function_name_name_id)
                    .or_default() += 1;
            }
            for argument_id in &expression.argument_expression_ids {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }
}

struct FunctionCallCollector<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    calls: BTreeSet<u64>,
}

impl FunctionCallCollector<'_> {
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
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER {
                self.calls.insert(expression.function_name_name_id);
            }
            for argument_id in &expression.argument_expression_ids {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum SsaValue {
    Expression(u64),
    Zero,
}

struct SsaValueTracker<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    values: BTreeMap<u64, SsaValue>,
}

impl SsaValueTracker<'_> {
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
                    self.values.remove(&name);
                }
                Ok(())
            }
            STATEMENT_VARIABLE_DECLARATION => {
                if !statement.has_value {
                    for variable_id in &statement.variable_ids {
                        let name = self.request.names[*variable_id as usize].name_id;
                        self.values.insert(name, SsaValue::Zero);
                    }
                } else if statement.variable_ids.len() == 1 {
                    let name = self.request.names[statement.variable_ids[0] as usize].name_id;
                    self.values
                        .insert(name, SsaValue::Expression(statement.value_expression_id));
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                for variable_id in &statement.return_variable_ids {
                    let name = self.request.names[*variable_id as usize].name_id;
                    self.values.insert(name, SsaValue::Zero);
                }
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
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }
}

struct BuiltinCallFinder<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    handle: u64,
    found: bool,
}

impl BuiltinCallFinder<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
            if self.found {
                break;
            }
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
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
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            if expression.function_name_kind == FUNCTION_NAME_BUILTIN
                && expression.function_name_builtin_handle == self.handle
            {
                self.found = true;
                return Ok(());
            }
            for argument_id in &expression.argument_expression_ids {
                self.visit_expression(*argument_id)?;
                if self.found {
                    break;
                }
            }
        }
        Ok(())
    }
}

fn contains_leave(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<bool, OptimizerError> {
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        let statement = &request.statements[*statement_id as usize];
        let contains = match statement.kind {
            STATEMENT_LEAVE => true,
            STATEMENT_FUNCTION_DEFINITION => contains_leave(request, statement.body_block_id)?,
            STATEMENT_IF => contains_leave(request, statement.body_block_id)?,
            STATEMENT_SWITCH => {
                let mut found = false;
                for case_id in &statement.case_ids {
                    found |=
                        contains_leave(request, request.cases[*case_id as usize].body_block_id)?;
                }
                found
            }
            STATEMENT_FOR_LOOP => {
                contains_leave(request, statement.pre_block_id)?
                    || contains_leave(request, statement.post_block_id)?
                    || contains_leave(request, statement.body_block_id)?
            }
            STATEMENT_BLOCK => contains_leave(request, statement.block_id)?,
            _ => false,
        };
        if contains {
            return Ok(true);
        }
    }
    Ok(false)
}

fn literal_is_non_string_zero(expression: &ffi::WireExpression) -> bool {
    expression.kind == EXPRESSION_LITERAL
        && expression.literal_kind != LITERAL_STRING
        && !expression.literal_unlimited
        && expression.literal_value.iter().all(|byte| *byte == 0)
}

fn block_contains_call_to(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    name_id: u64,
) -> Result<bool, OptimizerError> {
    let mut finder = CallTargetFinder {
        request,
        name_id,
        found: false,
    };
    finder.visit_block(block_id)?;
    Ok(finder.found)
}

struct CallTargetFinder<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    name_id: u64,
    found: bool,
}

impl CallTargetFinder<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
            if self.found {
                break;
            }
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
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
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER
                && expression.function_name_name_id == self.name_id
            {
                self.found = true;
                return Ok(());
            }
            for argument_id in &expression.argument_expression_ids {
                self.visit_expression(*argument_id)?;
                if self.found {
                    break;
                }
            }
        }
        Ok(())
    }
}

struct CodeSizeCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
}

impl CodeSizeCounter<'_> {
    fn block(&self, block_id: u64) -> Result<usize, OptimizerError> {
        let mut size = 0;
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            size += self.statement(*statement_id)?;
        }
        Ok(size)
    }

    fn statement(&self, statement_id: u64) -> Result<usize, OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        let size = match statement.kind {
            STATEMENT_EXPRESSION => self.expression(statement.expression_id)?,
            STATEMENT_ASSIGNMENT => self.expression(statement.value_expression_id)?,
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.expression(statement.value_expression_id)?
                } else {
                    0
                }
            }
            STATEMENT_FUNCTION_DEFINITION => 0,
            STATEMENT_IF => {
                2 + self.expression(statement.condition_expression_id)?
                    + self.block(statement.body_block_id)?
            }
            STATEMENT_SWITCH => {
                let mut size = 1
                    + 2 * statement.case_ids.len()
                    + self.expression(statement.switch_expression_id)?;
                for case_id in &statement.case_ids {
                    let switch_case = &self.request.cases[*case_id as usize];
                    size += self.block(switch_case.body_block_id)?;
                }
                size
            }
            STATEMENT_FOR_LOOP => {
                3 + self.block(statement.pre_block_id)?
                    + self.expression(statement.condition_expression_id)?
                    + self.block(statement.post_block_id)?
                    + self.block(statement.body_block_id)?
            }
            STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => 2,
            STATEMENT_BLOCK => self.block(statement.block_id)?,
            _ => 0,
        };
        Ok(size)
    }

    fn expression(&self, expression_id: u64) -> Result<usize, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                let mut size = 1;
                for argument_id in &expression.argument_expression_ids {
                    size += self.expression(*argument_id)?;
                }
                Ok(size)
            }
            EXPRESSION_IDENTIFIER => Ok(0),
            EXPRESSION_LITERAL => {
                if literal_is_non_string_zero(expression) {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }
}

fn expression_statement(debug_data_id: u64, expression_id: u64) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_EXPRESSION,
        debug_data_id,
        expression_id,
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

fn switch_statement(
    debug_data_id: u64,
    switch_expression_id: u64,
    case_ids: Vec<u64>,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_SWITCH,
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
        switch_expression_id,
        case_ids,
        block_id: 0,
    }
}

fn for_loop_statement(
    debug_data_id: u64,
    pre_block_id: u64,
    condition_expression_id: u64,
    post_block_id: u64,
    body_block_id: u64,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_FOR_LOOP,
        debug_data_id,
        expression_id: 0,
        has_value: false,
        value_expression_id: 0,
        variable_ids: Vec::new(),
        name_id: 0,
        parameter_ids: Vec::new(),
        return_variable_ids: Vec::new(),
        body_block_id,
        pre_block_id,
        post_block_id,
        condition_expression_id,
        switch_expression_id: 0,
        case_ids: Vec::new(),
        block_id: 0,
    }
}

fn block_statement(debug_data_id: u64, block_id: u64) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_BLOCK,
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
        block_id,
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
        function_name_kind: FUNCTION_NAME_BUILTIN,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}
