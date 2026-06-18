use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_IDENTIFIER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK,
    STATEMENT_CONTINUE, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Specialization {
    new_name_id: u64,
    literal_arguments: Vec<Option<u64>>,
}

pub fn run(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
) -> Result<(), OptimizerError> {
    let functions = collect_root_functions(request);
    let recursive_functions = collect_recursive_functions(request, &functions)?;

    let mut specializer = FunctionSpecializer {
        request,
        name_dispenser,
        recursive_functions,
        old_to_new: BTreeMap::new(),
    };
    specializer.visit_block(specializer.request.root_block_id)?;
    specializer.rewrite_function_definitions()
}

struct FunctionSpecializer<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    name_dispenser: &'b mut super::NameDispenser,
    recursive_functions: BTreeSet<u64>,
    old_to_new: BTreeMap<u64, Vec<Specialization>>,
}

impl FunctionSpecializer<'_, '_> {
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
        if expression.kind != EXPRESSION_FUNCTION_CALL {
            return Ok(());
        }

        for argument_id in expression.argument_expression_ids.iter().rev() {
            self.visit_expression(*argument_id)?;
        }

        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.function_name_kind != FUNCTION_NAME_IDENTIFIER
            || self
                .recursive_functions
                .contains(&expression.function_name_name_id)
        {
            return Ok(());
        }

        let mut literal_arguments = Vec::with_capacity(expression.argument_expression_ids.len());
        let mut retained_arguments = Vec::new();
        let mut has_literal = false;

        for argument_id in &expression.argument_expression_ids {
            if self.request.expressions[*argument_id as usize].kind == EXPRESSION_LITERAL {
                literal_arguments.push(Some(*argument_id));
                has_literal = true;
            } else {
                literal_arguments.push(None);
                retained_arguments.push(*argument_id);
            }
        }

        if !has_literal {
            return Ok(());
        }

        let old_name_id = expression.function_name_name_id;
        let new_name_id = self.new_name_like(old_name_id);
        self.old_to_new
            .entry(old_name_id)
            .or_default()
            .push(Specialization {
                new_name_id,
                literal_arguments,
            });

        let expression = &mut self.request.expressions[expression_id as usize];
        expression.function_name_name_id = new_name_id;
        expression.argument_expression_ids = retained_arguments;
        Ok(())
    }

    fn rewrite_function_definitions(&mut self) -> Result<(), OptimizerError> {
        let root_block_id = self.request.root_block_id;
        let statement_ids = self.request.blocks[root_block_id as usize]
            .statement_ids
            .clone();
        let mut rewritten_statement_ids = Vec::with_capacity(statement_ids.len());

        for statement_id in statement_ids {
            let statement = self.request.statements[statement_id as usize].clone();
            if statement.kind == STATEMENT_FUNCTION_DEFINITION {
                if let Some(specializations) = self.old_to_new.get(&statement.name_id).cloned() {
                    for specialization in specializations {
                        let specialized_id =
                            self.specialize_function(statement_id, specialization)?;
                        rewritten_statement_ids.push(specialized_id);
                    }
                }
            }
            rewritten_statement_ids.push(statement_id);
        }

        self.request.blocks[root_block_id as usize].statement_ids = rewritten_statement_ids;
        Ok(())
    }

    fn specialize_function(
        &mut self,
        function_statement_id: u64,
        specialization: Specialization,
    ) -> Result<u64, OptimizerError> {
        let function = self.request.statements[function_statement_id as usize].clone();
        if specialization.literal_arguments.len() != function.parameter_ids.len() {
            return Ok(function_statement_id);
        }

        let mut variable_names = BTreeSet::new();
        collect_function_variables(self.request, function_statement_id, &mut variable_names)?;

        let mut replacements = BTreeMap::new();
        for name_id in variable_names {
            replacements.insert(name_id, self.new_name_like(name_id));
        }

        let cloned_function_id =
            self.clone_statement_with_replacements(function_statement_id, &replacements)?;
        let mut cloned_function = self.request.statements[cloned_function_id as usize].clone();
        cloned_function.name_id = specialization.new_name_id;

        let mut literal_declarations = Vec::new();
        let mut retained_parameters = Vec::new();
        for (index, literal_argument) in specialization.literal_arguments.iter().enumerate() {
            let parameter_id = cloned_function.parameter_ids[index];
            if let Some(literal_expression_id) = literal_argument {
                let value_id = self
                    .clone_expression_with_replacements(*literal_expression_id, &BTreeMap::new())?;
                let declaration_id = self.request.statements.len() as u64;
                self.request.statements.push(variable_declaration_statement(
                    function.debug_data_id,
                    vec![parameter_id],
                    Some(value_id),
                ));
                literal_declarations.push(declaration_id);
            } else {
                retained_parameters.push(parameter_id);
            }
        }
        cloned_function.parameter_ids = retained_parameters;

        let body_block_id = cloned_function.body_block_id;
        let original_body = self.request.blocks[body_block_id as usize]
            .statement_ids
            .clone();
        let mut rewritten_body = literal_declarations;
        rewritten_body.extend(original_body);
        self.request.blocks[body_block_id as usize].statement_ids = rewritten_body;
        self.request.statements[cloned_function_id as usize] = cloned_function;

        Ok(cloned_function_id)
    }

    fn clone_statement_with_replacements(
        &mut self,
        statement_id: u64,
        replacements: &BTreeMap<u64, u64>,
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
                let variables = statement
                    .variable_ids
                    .iter()
                    .map(|name_id| self.clone_name_with_debug_data(*name_id, replacements))
                    .collect();
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
                let parameters = statement
                    .parameter_ids
                    .iter()
                    .map(|name_id| self.clone_name_with_debug_data(*name_id, replacements))
                    .collect();
                let return_variables = statement
                    .return_variable_ids
                    .iter()
                    .map(|name_id| self.clone_name_with_debug_data(*name_id, replacements))
                    .collect();
                let body_block_id =
                    self.clone_block_with_replacements(statement.body_block_id, replacements)?;
                function_definition_statement(
                    statement.debug_data_id,
                    statement.name_id,
                    parameters,
                    return_variables,
                    body_block_id,
                )
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
        replacements: &BTreeMap<u64, u64>,
    ) -> Result<u64, OptimizerError> {
        let block = self.request.blocks[block_id as usize].clone();
        let mut statement_ids = Vec::with_capacity(block.statement_ids.len());
        for statement_id in block.statement_ids {
            statement_ids.push(self.clone_statement_with_replacements(statement_id, replacements)?);
        }
        let cloned_id = self.request.blocks.len() as u64;
        self.request.blocks.push(ffi::WireBlock {
            debug_data_id: block.debug_data_id,
            statement_ids,
        });
        Ok(cloned_id)
    }

    fn clone_case_with_replacements(
        &mut self,
        case_id: u64,
        replacements: &BTreeMap<u64, u64>,
    ) -> Result<u64, OptimizerError> {
        let switch_case = self.request.cases[case_id as usize].clone();
        let value_expression_id = if switch_case.has_value {
            self.clone_expression_with_replacements(switch_case.value_expression_id, replacements)?
        } else {
            0
        };
        let body_block_id =
            self.clone_block_with_replacements(switch_case.body_block_id, replacements)?;
        let cloned_id = self.request.cases.len() as u64;
        self.request.cases.push(ffi::WireCase {
            debug_data_id: switch_case.debug_data_id,
            has_value: switch_case.has_value,
            value_expression_id,
            body_block_id,
        });
        Ok(cloned_id)
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

    fn new_name_like(&mut self, name_id: u64) -> u64 {
        self.name_dispenser.new_name_like(self.request, name_id)
    }
}

fn collect_function_variables(
    request: &ffi::WireYulOptimizerRequest,
    function_statement_id: u64,
    output: &mut BTreeSet<u64>,
) -> Result<(), OptimizerError> {
    let function = &request.statements[function_statement_id as usize];
    for parameter_id in &function.parameter_ids {
        output.insert(request.names[*parameter_id as usize].name_id);
    }
    for return_variable_id in &function.return_variable_ids {
        output.insert(request.names[*return_variable_id as usize].name_id);
    }
    collect_declared_variables_in_block(request, function.body_block_id, output)
}

fn collect_declared_variables_in_block(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    output: &mut BTreeSet<u64>,
) -> Result<(), OptimizerError> {
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        let statement = &request.statements[*statement_id as usize];
        match statement.kind {
            STATEMENT_VARIABLE_DECLARATION => {
                for variable_id in &statement.variable_ids {
                    output.insert(request.names[*variable_id as usize].name_id);
                }
            }
            STATEMENT_FUNCTION_DEFINITION => {
                collect_function_variables(request, *statement_id, output)?;
            }
            STATEMENT_IF => {
                collect_declared_variables_in_block(request, statement.body_block_id, output)?
            }
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    collect_declared_variables_in_block(
                        request,
                        request.cases[*case_id as usize].body_block_id,
                        output,
                    )?;
                }
            }
            STATEMENT_FOR_LOOP => {
                collect_declared_variables_in_block(request, statement.pre_block_id, output)?;
                collect_declared_variables_in_block(request, statement.post_block_id, output)?;
                collect_declared_variables_in_block(request, statement.body_block_id, output)?;
            }
            STATEMENT_BLOCK => {
                collect_declared_variables_in_block(request, statement.block_id, output)?
            }
            _ => {}
        }
    }
    Ok(())
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
