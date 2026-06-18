use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
enum ValueRef {
    Expression(u64),
    Zero,
}

#[derive(Clone, Debug, Default)]
struct DataFlowState {
    value: BTreeMap<u64, ValueRef>,
    sorted_references: BTreeMap<u64, Vec<u64>>,
}

#[derive(Clone, Debug)]
struct Scope {
    variables: BTreeSet<u64>,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    LiteralRematerialiser {
        request,
        state: DataFlowState::default(),
        loop_depth: 0,
        variable_scopes: Vec::new(),
    }
    .visit_block_root()
}

struct LiteralRematerialiser<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    state: DataFlowState,
    loop_depth: usize,
    variable_scopes: Vec<Scope>,
}

impl LiteralRematerialiser<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let scope_count = self.variable_scopes.len();
        self.push_scope(false);

        let result = (|| {
            let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
            for statement_id in statement_ids {
                self.visit_statement(statement_id)?;
            }
            Ok(())
        })();

        self.pop_scope();
        if self.variable_scopes.len() != scope_count {
            return Err(OptimizerError::InvalidWire(
                "scope stack mismatch in LiteralRematerialiser".to_string(),
            ));
        }
        result
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => {
                let names = self.identifier_names(&statement.variable_ids);
                self.visit_expression(statement.value_expression_id)?;
                self.handle_assignment(names, Some(statement.value_expression_id), false)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                let names = self.name_with_debug_data_names(&statement.variable_ids);
                if let Some(scope) = self.variable_scopes.last_mut() {
                    scope.variables.extend(names.iter().copied());
                }
                if statement.has_value {
                    self.visit_expression(statement.value_expression_id)?;
                    self.handle_assignment(names, Some(statement.value_expression_id), true)
                } else {
                    self.handle_assignment(names, None, true)
                }
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_function_definition(&statement),
            STATEMENT_IF => {
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)?;
                let assigned = self.assigned_variable_names(statement.body_block_id)?;
                self.clear_values(&assigned);
                Ok(())
            }
            STATEMENT_SWITCH => {
                self.visit_expression(statement.switch_expression_id)?;
                let mut assigned_variables = BTreeSet::new();
                for case_id in statement.case_ids {
                    let switch_case = self.request.cases[case_id as usize].clone();
                    if switch_case.has_value {
                        self.visit_expression(switch_case.value_expression_id)?;
                    }
                    self.visit_block(switch_case.body_block_id)?;
                    let variables = self.assigned_variable_names(switch_case.body_block_id)?;
                    assigned_variables.extend(variables.iter().copied());
                    self.clear_values(&variables);
                }
                self.clear_values(&assigned_variables);
                Ok(())
            }
            STATEMENT_FOR_LOOP => self.visit_for_loop(&statement),
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_function_definition(
        &mut self,
        statement: &ffi::WireStatement,
    ) -> Result<(), OptimizerError> {
        let saved_state = std::mem::take(&mut self.state);
        let saved_loop_depth = self.loop_depth;
        self.loop_depth = 0;
        self.push_scope(true);

        let result = (|| {
            let parameter_names = self.name_with_debug_data_names(&statement.parameter_ids);
            if let Some(scope) = self.variable_scopes.last_mut() {
                scope.variables.extend(parameter_names);
            }

            for variable_id in &statement.return_variable_ids {
                let variable = self.request.names[*variable_id as usize].name_id;
                if let Some(scope) = self.variable_scopes.last_mut() {
                    scope.variables.insert(variable);
                }
                self.handle_assignment(BTreeSet::from([variable]), None, true)?;
            }

            self.visit_block(statement.body_block_id)
        })();

        self.pop_scope();
        self.state = saved_state;
        self.loop_depth = saved_loop_depth;
        result
    }

    fn visit_for_loop(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "LiteralRematerialiser requires ForLoopInitRewriter.".to_string(),
            ));
        }

        self.loop_depth += 1;
        let result = (|| {
            let assignments_since_continue =
                self.assignments_since_continue(statement.body_block_id)?;
            let mut assigned_variables = self.assigned_variable_names(statement.body_block_id)?;
            assigned_variables.extend(self.assigned_variable_names(statement.post_block_id)?);
            self.clear_values(&assigned_variables);

            self.visit_expression(statement.condition_expression_id)?;
            self.visit_block(statement.body_block_id)?;
            self.clear_values(&assignments_since_continue);
            self.visit_block(statement.post_block_id)?;
            self.clear_values(&assigned_variables);
            Ok(())
        })();
        self.loop_depth -= 1;
        result
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        self.try_replace_identifier_with_literal(expression_id);

        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }

    fn try_replace_identifier_with_literal(&mut self, expression_id: u64) {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_IDENTIFIER {
            return;
        }

        let Some(value) = self.state.value.get(&expression.name_id).copied() else {
            return;
        };

        match value {
            ValueRef::Expression(value_expression_id) => {
                let value_expression =
                    self.request.expressions[value_expression_id as usize].clone();
                if value_expression.kind == EXPRESSION_LITERAL {
                    self.request.expressions[expression_id as usize] = value_expression;
                }
            }
            ValueRef::Zero => {
                self.request.expressions[expression_id as usize] = zero_literal(u64::MAX);
            }
        }
    }

    fn handle_assignment(
        &mut self,
        names: BTreeSet<u64>,
        value: Option<u64>,
        is_declaration: bool,
    ) -> Result<(), OptimizerError> {
        if !is_declaration {
            self.clear_values(&names);
        }

        if let Some(value_id) = value {
            let value_expression = &self.request.expressions[value_id as usize];
            if names.len() == 1 && value_expression.kind == EXPRESSION_LITERAL {
                let name = *names.iter().next().expect("single item");
                self.assign_value(name, ValueRef::Expression(value_id));
            }
        } else {
            for name in &names {
                self.assign_value(*name, ValueRef::Zero);
            }
        }

        for name in names {
            self.state.sorted_references.insert(name, Vec::new());
        }

        Ok(())
    }

    fn assign_value(&mut self, variable: u64, value: ValueRef) {
        self.state.value.insert(variable, value);
    }

    fn clear_values(&mut self, variables_to_clear: &BTreeSet<u64>) {
        let mut referencing_variables_to_clear = Vec::new();
        for (referencing_variable, referenced_variables) in &self.state.sorted_references {
            if referenced_variables
                .iter()
                .any(|referenced| variables_to_clear.contains(referenced))
            {
                referencing_variables_to_clear.push(*referencing_variable);
            }
        }

        for name in variables_to_clear {
            self.state.value.remove(name);
            self.state.sorted_references.remove(name);
        }
        for name in referencing_variables_to_clear {
            self.state.value.remove(&name);
            self.state.sorted_references.remove(&name);
        }
    }

    fn identifier_names(&self, identifier_ids: &[u64]) -> BTreeSet<u64> {
        identifier_ids
            .iter()
            .map(|identifier_id| self.request.identifiers[*identifier_id as usize].name_id)
            .collect()
    }

    fn name_with_debug_data_names(&self, name_ids: &[u64]) -> BTreeSet<u64> {
        name_ids
            .iter()
            .map(|name_id| self.request.names[*name_id as usize].name_id)
            .collect()
    }

    fn push_scope(&mut self, _is_function: bool) {
        self.variable_scopes.push(Scope {
            variables: BTreeSet::new(),
        });
    }

    fn pop_scope(&mut self) {
        if let Some(scope) = self.variable_scopes.pop() {
            for name in scope.variables {
                self.state.value.remove(&name);
                self.state.sorted_references.remove(&name);
            }
        }
    }

    fn assigned_variable_names(&self, block_id: u64) -> Result<BTreeSet<u64>, OptimizerError> {
        let mut collector = AssignedVariableCollector {
            request: self.request,
            names: BTreeSet::new(),
        };
        collector.visit_block(block_id)?;
        Ok(collector.names)
    }

    fn assignments_since_continue(&self, block_id: u64) -> Result<BTreeSet<u64>, OptimizerError> {
        let mut collector = AssignmentsSinceContinueCollector {
            request: self.request,
            names: BTreeSet::new(),
            for_loop_depth: 0,
            continue_found: false,
        };
        collector.visit_block(block_id)?;
        Ok(collector.names)
    }
}

struct AssignedVariableCollector<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    names: BTreeSet<u64>,
}

impl AssignedVariableCollector<'_> {
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
                    self.names
                        .insert(self.request.identifiers[*variable_id as usize].name_id);
                }
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id)?,
            STATEMENT_IF => self.visit_block(statement.body_block_id)?,
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    self.visit_block(self.request.cases[*case_id as usize].body_block_id)?;
                }
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(statement.pre_block_id)?;
                self.visit_block(statement.body_block_id)?;
                self.visit_block(statement.post_block_id)?;
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id)?,
            _ => {}
        }
        Ok(())
    }
}

struct AssignmentsSinceContinueCollector<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    names: BTreeSet<u64>,
    for_loop_depth: usize,
    continue_found: bool,
}

impl AssignmentsSinceContinueCollector<'_> {
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
                if self.continue_found {
                    for variable_id in &statement.variable_ids {
                        self.names
                            .insert(self.request.identifiers[*variable_id as usize].name_id);
                    }
                }
            }
            STATEMENT_FUNCTION_DEFINITION => {
                return Err(OptimizerError::InvalidWire(
                    "AssignmentsSinceContinue encountered a function definition.".to_string(),
                ));
            }
            STATEMENT_IF => self.visit_block(statement.body_block_id)?,
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    self.visit_block(self.request.cases[*case_id as usize].body_block_id)?;
                }
            }
            STATEMENT_FOR_LOOP => {
                self.for_loop_depth += 1;
                self.visit_block(statement.pre_block_id)?;
                self.visit_block(statement.body_block_id)?;
                self.visit_block(statement.post_block_id)?;
                self.for_loop_depth -= 1;
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id)?,
            crate::wire::STATEMENT_CONTINUE => {
                if self.for_loop_depth == 0 {
                    self.continue_found = true;
                }
            }
            _ => {}
        }
        Ok(())
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
        function_name_kind: FUNCTION_NAME_IDENTIFIER,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}
