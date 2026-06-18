use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, LITERAL_STRING, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_LEAVE, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn run(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
) -> Result<(), OptimizerError> {
    let references = VariableReferencesCounter::count_references(request, request.root_block_id)?;
    let root_statement_ids = request.blocks[request.root_block_id as usize]
        .statement_ids
        .clone();

    let mut used_parameters_and_returns = BTreeMap::new();
    for statement_id in &root_statement_ids {
        let statement = &request.statements[*statement_id as usize];
        if statement.kind != STATEMENT_FUNCTION_DEFINITION {
            continue;
        }

        if too_simple_to_be_pruned(request, statement) {
            continue;
        }

        let parameter_mask = use_mask(request, &statement.parameter_ids, &references);
        let return_mask = use_mask(request, &statement.return_variable_ids, &references);
        if parameter_mask
            .iter()
            .chain(return_mask.iter())
            .all(|used| *used)
        {
            continue;
        }

        used_parameters_and_returns.insert(
            statement.name_id,
            UsedParametersAndReturns {
                parameters: parameter_mask,
                returns: return_mask,
            },
        );
    }

    if used_parameters_and_returns.is_empty() {
        return Ok(());
    }

    let names_to_free = used_parameters_and_returns.keys().copied().collect();
    let root_block_id = request.root_block_id;
    let translations =
        NameDisplacer::new(request, name_dispenser, names_to_free).run(root_block_id)?;
    let new_to_original = translations
        .iter()
        .map(|(original, replacement)| (*replacement, *original))
        .collect::<BTreeMap<_, _>>();

    let root_statement_ids = request.blocks[request.root_block_id as usize]
        .statement_ids
        .clone();
    let mut rewritten = Vec::with_capacity(root_statement_ids.len());
    for statement_id in root_statement_ids {
        let statement = request.statements[statement_id as usize].clone();
        let Some(original_name) = (statement.kind == STATEMENT_FUNCTION_DEFINITION)
            .then_some(statement.name_id)
            .and_then(|name| new_to_original.get(&name).copied())
        else {
            rewritten.push(statement_id);
            continue;
        };

        let used = used_parameters_and_returns[&original_name].clone();
        let linking_function_id = create_linking_function(
            request,
            name_dispenser,
            &statement,
            &used,
            original_name,
            statement.name_id,
        );

        {
            let original_function = &mut request.statements[statement_id as usize];
            original_function.name_id = original_name;
            original_function.parameter_ids =
                filter_ids(&original_function.parameter_ids, &used.parameters);
            original_function.return_variable_ids =
                filter_ids(&original_function.return_variable_ids, &used.returns);
        }

        rewritten.push(statement_id);
        rewritten.push(linking_function_id);
    }
    request.blocks[request.root_block_id as usize].statement_ids = rewritten;

    Ok(())
}

#[derive(Clone)]
struct UsedParametersAndReturns {
    parameters: Vec<bool>,
    returns: Vec<bool>,
}

struct VariableReferencesCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    references: BTreeMap<u64, usize>,
}

impl<'a> VariableReferencesCounter<'a> {
    fn count_references(
        request: &'a ffi::WireYulOptimizerRequest,
        block_id: u64,
    ) -> Result<BTreeMap<u64, usize>, OptimizerError> {
        let mut counter = Self {
            request,
            references: BTreeMap::new(),
        };
        counter.visit_block(block_id)?;
        Ok(counter.references)
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
            STATEMENT_ASSIGNMENT => {
                for variable_id in statement.variable_ids {
                    let name_id = self.request.identifiers[variable_id as usize].name_id;
                    *self.references.entry(name_id).or_default() += 1;
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
                for case_id in statement.case_ids {
                    let switch_case = &self.request.cases[case_id as usize];
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
        let expression = self.request.expressions[expression_id as usize].clone();
        match expression.kind {
            EXPRESSION_IDENTIFIER => {
                *self.references.entry(expression.name_id).or_default() += 1;
            }
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }
            }
            EXPRESSION_LITERAL => {}
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid expression kind: {}",
                    expression.kind
                )));
            }
        }
        Ok(())
    }
}

struct NameDisplacer<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    name_dispenser: &'b mut super::NameDispenser,
    names_to_free: BTreeSet<u64>,
    translations: BTreeMap<u64, u64>,
}

impl<'a, 'b> NameDisplacer<'a, 'b> {
    fn new(
        request: &'a mut ffi::WireYulOptimizerRequest,
        name_dispenser: &'b mut super::NameDispenser,
        names_to_free: BTreeSet<u64>,
    ) -> Self {
        Self {
            request,
            name_dispenser,
            names_to_free,
            translations: BTreeMap::new(),
        }
    }

    fn run(mut self, block_id: u64) -> Result<BTreeMap<u64, u64>, OptimizerError> {
        self.visit_block(block_id)?;
        Ok(self.translations)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();

        for statement_id in &statement_ids {
            if self.request.statements[*statement_id as usize].kind == STATEMENT_FUNCTION_DEFINITION
            {
                let old_name = self.request.statements[*statement_id as usize].name_id;
                let new_name = self.check_and_replace_new(old_name);
                self.request.statements[*statement_id as usize].name_id = new_name;
            }
        }

        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => {
                for variable_id in statement.variable_ids {
                    let old_name = self.request.identifiers[variable_id as usize].name_id;
                    self.request.identifiers[variable_id as usize].name_id =
                        self.check_and_replace(old_name);
                }
                self.visit_expression(statement.value_expression_id)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                for variable_id in statement.variable_ids {
                    let old_name = self.request.names[variable_id as usize].name_id;
                    self.request.names[variable_id as usize].name_id =
                        self.check_and_replace_new(old_name);
                }
                if statement.has_value {
                    self.visit_expression(statement.value_expression_id)?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                for parameter_id in statement.parameter_ids {
                    let old_name = self.request.names[parameter_id as usize].name_id;
                    self.request.names[parameter_id as usize].name_id =
                        self.check_and_replace_new(old_name);
                }
                for return_variable_id in statement.return_variable_ids {
                    let old_name = self.request.names[return_variable_id as usize].name_id;
                    self.request.names[return_variable_id as usize].name_id =
                        self.check_and_replace_new(old_name);
                }
                self.visit_block(statement.body_block_id)
            }
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
        match expression.kind {
            EXPRESSION_IDENTIFIER => {
                self.request.expressions[expression_id as usize].name_id =
                    self.check_and_replace(expression.name_id);
            }
            EXPRESSION_FUNCTION_CALL => {
                if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER {
                    self.request.expressions[expression_id as usize].function_name_name_id =
                        self.check_and_replace(expression.function_name_name_id);
                }
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }
            }
            EXPRESSION_LITERAL => {}
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid expression kind: {}",
                    expression.kind
                )));
            }
        }
        Ok(())
    }

    fn check_and_replace_new(&mut self, name_id: u64) -> u64 {
        if !self.names_to_free.contains(&name_id) {
            return name_id;
        }

        if let Some(replacement) = self.translations.get(&name_id) {
            return *replacement;
        }

        let replacement = self.name_dispenser.new_name_like(self.request, name_id);
        self.translations.insert(name_id, replacement);
        replacement
    }

    fn check_and_replace(&self, name_id: u64) -> u64 {
        self.translations.get(&name_id).copied().unwrap_or(name_id)
    }
}

struct CodeSize<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
}

impl CodeSize<'_> {
    fn block_size(&self, block_id: u64) -> usize {
        self.request.blocks[block_id as usize]
            .statement_ids
            .iter()
            .map(|statement_id| self.statement_size(*statement_id))
            .sum()
    }

    fn statement_size(&self, statement_id: u64) -> usize {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_EXPRESSION => self.expression_size(statement.expression_id),
            STATEMENT_ASSIGNMENT => self.expression_size(statement.value_expression_id),
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.expression_size(statement.value_expression_id)
                } else {
                    0
                }
            }
            STATEMENT_FUNCTION_DEFINITION => 0,
            STATEMENT_IF => {
                2 + self.expression_size(statement.condition_expression_id)
                    + self.block_size(statement.body_block_id)
            }
            STATEMENT_SWITCH => {
                1 + statement.case_ids.len() * 2
                    + self.expression_size(statement.switch_expression_id)
                    + statement
                        .case_ids
                        .iter()
                        .map(|case_id| {
                            let switch_case = &self.request.cases[*case_id as usize];
                            self.block_size(switch_case.body_block_id)
                        })
                        .sum::<usize>()
            }
            STATEMENT_FOR_LOOP => {
                3 + self.block_size(statement.pre_block_id)
                    + self.expression_size(statement.condition_expression_id)
                    + self.block_size(statement.body_block_id)
                    + self.block_size(statement.post_block_id)
            }
            STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => 2,
            STATEMENT_BLOCK => self.block_size(statement.block_id),
            _ => 0,
        }
    }

    fn expression_size(&self, expression_id: u64) -> usize {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                1 + expression
                    .argument_expression_ids
                    .iter()
                    .map(|argument_id| self.expression_size(*argument_id))
                    .sum::<usize>()
            }
            EXPRESSION_IDENTIFIER => 0,
            EXPRESSION_LITERAL => {
                if expression.literal_kind != LITERAL_STRING
                    && !expression.literal_unlimited
                    && expression.literal_value.iter().all(|byte| *byte == 0)
                {
                    0
                } else {
                    1
                }
            }
            _ => 0,
        }
    }
}

fn too_simple_to_be_pruned(
    request: &ffi::WireYulOptimizerRequest,
    function: &ffi::WireStatement,
) -> bool {
    request.blocks[function.body_block_id as usize]
        .statement_ids
        .len()
        <= 1
        && CodeSize { request }.block_size(function.body_block_id) <= 1
}

fn use_mask(
    request: &ffi::WireYulOptimizerRequest,
    name_ids: &[u64],
    references: &BTreeMap<u64, usize>,
) -> Vec<bool> {
    name_ids
        .iter()
        .map(|name_id| references.contains_key(&request.names[*name_id as usize].name_id))
        .collect()
}

fn filter_ids(ids: &[u64], mask: &[bool]) -> Vec<u64> {
    ids.iter()
        .zip(mask.iter())
        .filter_map(|(id, used)| used.then_some(*id))
        .collect()
}

fn create_linking_function(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
    original: &ffi::WireStatement,
    used: &UsedParametersAndReturns,
    original_function_name: u64,
    linking_function_name: u64,
) -> u64 {
    let linking_parameters = generate_typed_names(request, name_dispenser, &original.parameter_ids);
    let linking_returns =
        generate_typed_names(request, name_dispenser, &original.return_variable_ids);

    let mut arguments = Vec::new();
    for (parameter_id, is_used) in linking_parameters.iter().zip(used.parameters.iter()) {
        if *is_used {
            let name_id = request.names[*parameter_id as usize].name_id;
            arguments.push(push_expression(
                request,
                identifier_expression(original.debug_data_id, name_id),
            ));
        }
    }

    let call = push_expression(
        request,
        function_call_expression(original.debug_data_id, original_function_name, arguments),
    );

    let mut assignment_variables = Vec::new();
    for (return_id, is_used) in linking_returns.iter().zip(used.returns.iter()) {
        if *is_used {
            let name_id = request.names[*return_id as usize].name_id;
            assignment_variables.push(push_identifier(
                request,
                ffi::WireIdentifier {
                    debug_data_id: original.debug_data_id,
                    name_id,
                },
            ));
        }
    }

    let call_statement = if assignment_variables.is_empty() {
        expression_statement(original.debug_data_id, call)
    } else {
        assignment_statement(original.debug_data_id, assignment_variables, call)
    };
    let call_statement_id = push_statement(request, call_statement);
    let body_block_id = push_block(
        request,
        ffi::WireBlock {
            debug_data_id: original.debug_data_id,
            statement_ids: vec![call_statement_id],
        },
    );

    push_statement(
        request,
        function_definition_statement(
            original.debug_data_id,
            linking_function_name,
            linking_parameters,
            linking_returns,
            body_block_id,
        ),
    )
}

fn generate_typed_names(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
    name_ids: &[u64],
) -> Vec<u64> {
    name_ids
        .iter()
        .map(|name_id| {
            let name = request.names[*name_id as usize].clone();
            let new_name = name_dispenser.new_name_like(request, name.name_id);
            push_name(
                request,
                ffi::WireNameWithDebugData {
                    debug_data_id: name.debug_data_id,
                    name_id: new_name,
                },
            )
        })
        .collect()
}

fn push_name(request: &mut ffi::WireYulOptimizerRequest, name: ffi::WireNameWithDebugData) -> u64 {
    let id = request.names.len() as u64;
    request.names.push(name);
    id
}

fn push_identifier(
    request: &mut ffi::WireYulOptimizerRequest,
    identifier: ffi::WireIdentifier,
) -> u64 {
    let id = request.identifiers.len() as u64;
    request.identifiers.push(identifier);
    id
}

fn push_expression(
    request: &mut ffi::WireYulOptimizerRequest,
    expression: ffi::WireExpression,
) -> u64 {
    let id = request.expressions.len() as u64;
    request.expressions.push(expression);
    id
}

fn push_statement(
    request: &mut ffi::WireYulOptimizerRequest,
    statement: ffi::WireStatement,
) -> u64 {
    let id = request.statements.len() as u64;
    request.statements.push(statement);
    id
}

fn push_block(request: &mut ffi::WireYulOptimizerRequest, block: ffi::WireBlock) -> u64 {
    let id = request.blocks.len() as u64;
    request.blocks.push(block);
    id
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

fn function_call_expression(
    debug_data_id: u64,
    function_name: u64,
    arguments: Vec<u64>,
) -> ffi::WireExpression {
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
        function_name_kind: FUNCTION_NAME_IDENTIFIER,
        function_name_debug_data_id: debug_data_id,
        function_name_name_id: function_name,
        function_name_builtin_handle: 0,
        argument_expression_ids: arguments,
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

fn function_definition_statement(
    debug_data_id: u64,
    name_id: u64,
    parameter_ids: Vec<u64>,
    return_variable_ids: Vec<u64>,
    body_block_id: u64,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_FUNCTION_DEFINITION,
        debug_data_id,
        expression_id: 0,
        has_value: false,
        value_expression_id: 0,
        variable_ids: Vec::new(),
        name_id,
        parameter_ids,
        return_variable_ids,
        body_block_id,
        pre_block_id: 0,
        post_block_id: 0,
        condition_expression_id: 0,
        switch_expression_id: 0,
        case_ids: Vec::new(),
        block_id: 0,
    }
}
