use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use primitive_types::U256;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
enum ValueRef {
    Expression(u64),
    Zero,
}

#[derive(Clone, Copy, Debug)]
struct AssignedValue {
    value: ValueRef,
    loop_depth: usize,
}

#[derive(Clone, Debug, Default)]
struct DataFlowState {
    value: BTreeMap<u64, AssignedValue>,
    sorted_references: BTreeMap<u64, Vec<u64>>,
}

#[derive(Clone, Debug)]
struct Scope {
    variables: BTreeSet<u64>,
    is_function: bool,
}

#[derive(Clone, Debug, Default)]
struct MovableInfo {
    movable: bool,
    referenced_variables: BTreeSet<u64>,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let reference_counts = count_variable_references(request)?;
    Rematerialiser {
        request,
        reference_counts,
        state: DataFlowState::default(),
        loop_depth: 0,
        variable_scopes: Vec::new(),
    }
    .visit_block_root()
}

struct Rematerialiser<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    reference_counts: BTreeMap<u64, usize>,
    state: DataFlowState,
    loop_depth: usize,
    variable_scopes: Vec<Scope>,
}

impl Rematerialiser<'_> {
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
                "scope stack mismatch in Rematerialiser".to_string(),
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
                "Rematerialiser requires ForLoopInitRewriter.".to_string(),
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
        self.try_replace_identifier(expression_id)?;

        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }

    fn try_replace_identifier(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_IDENTIFIER {
            return Ok(());
        }

        let name = expression.name_id;
        let Some(assigned_value) = self.state.value.get(&name).copied() else {
            return Ok(());
        };

        let value_expression_id = match assigned_value.value {
            ValueRef::Expression(value_expression_id) => value_expression_id,
            ValueRef::Zero => {
                let zero_id = self.request.expressions.len() as u64;
                self.request.expressions.push(zero_literal(u64::MAX));
                zero_id
            }
        };

        let refs = self.reference_counts.get(&name).copied().unwrap_or(0);
        let cost = self.code_cost_expression(value_expression_id)?;
        let should_replace = (refs <= 1 && assigned_value.loop_depth == self.loop_depth)
            || cost == 0
            || (refs <= 5 && cost <= 1 && self.loop_depth == 0);
        if !should_replace {
            return Ok(());
        }

        if let Some(references) = self.state.sorted_references.get(&name) {
            if references
                .iter()
                .any(|reference| !self.in_scope(*reference))
            {
                return Ok(());
            }
        }

        if let Some(count) = self.reference_counts.get_mut(&name) {
            *count = count.saturating_sub(1);
        }
        for reference in self.expression_references(value_expression_id)? {
            *self.reference_counts.entry(reference).or_default() += 1;
        }

        let cloned = self.clone_expression_value(value_expression_id)?;
        self.request.expressions[expression_id as usize] = cloned;
        Ok(())
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

        let mut referenced_variables = BTreeSet::new();
        if let Some(value_id) = value {
            let movable = self.movable_info(value_id)?;
            referenced_variables = movable.referenced_variables;
            if names.len() == 1 {
                let name = *names.iter().next().expect("single item");
                if movable.movable && !referenced_variables.contains(&name) {
                    self.assign_value(name, ValueRef::Expression(value_id));
                }
            }
        } else {
            for name in &names {
                self.assign_value(*name, ValueRef::Zero);
            }
        }

        let sorted_references: Vec<u64> = referenced_variables.iter().copied().collect();
        for name in names {
            self.state
                .sorted_references
                .insert(name, sorted_references.clone());
        }

        Ok(())
    }

    fn assign_value(&mut self, variable: u64, value: ValueRef) {
        self.state.value.insert(
            variable,
            AssignedValue {
                value,
                loop_depth: self.loop_depth,
            },
        );
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

    fn movable_info(&self, expression_id: u64) -> Result<MovableInfo, OptimizerError> {
        let mut info = MovableInfo {
            movable: true,
            referenced_variables: BTreeSet::new(),
        };
        self.collect_movable_info(expression_id, &mut info)?;
        Ok(info)
    }

    fn collect_movable_info(
        &self,
        expression_id: u64,
        info: &mut MovableInfo,
    ) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.collect_movable_info(*argument_id, info)?;
                }
                match expression.function_name_kind {
                    FUNCTION_NAME_BUILTIN => {
                        let movable = self
                            .request
                            .builtins
                            .iter()
                            .find(|builtin| {
                                builtin.handle_id == expression.function_name_builtin_handle
                            })
                            .is_some_and(|builtin| builtin.movable);
                        info.movable &= movable;
                    }
                    FUNCTION_NAME_IDENTIFIER => {
                        info.movable = false;
                    }
                    _ => {
                        return Err(OptimizerError::InvalidWire(format!(
                            "invalid function name kind: {}",
                            expression.function_name_kind
                        )));
                    }
                }
            }
            EXPRESSION_IDENTIFIER => {
                info.referenced_variables.insert(expression.name_id);
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

    fn clone_expression_value(
        &mut self,
        expression_id: u64,
    ) -> Result<ffi::WireExpression, OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        let mut cloned = expression.clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            let mut arguments = Vec::with_capacity(expression.argument_expression_ids.len());
            for argument_id in expression.argument_expression_ids {
                let cloned_argument = self.clone_expression_to_arena(argument_id)?;
                arguments.push(cloned_argument);
            }
            cloned.argument_expression_ids = arguments;
        }
        Ok(cloned)
    }

    fn clone_expression_to_arena(&mut self, expression_id: u64) -> Result<u64, OptimizerError> {
        let cloned = self.clone_expression_value(expression_id)?;
        let cloned_id = self.request.expressions.len() as u64;
        self.request.expressions.push(cloned);
        Ok(cloned_id)
    }

    fn code_cost_expression(&self, expression_id: u64) -> Result<usize, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        let mut cost = 1usize;
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in &expression.argument_expression_ids {
                    cost += self.code_cost_expression(*argument_id)?;
                }
                if expression.function_name_kind == FUNCTION_NAME_BUILTIN {
                    if let Some(opcode) = self.evm_opcode(expression.function_name_builtin_handle) {
                        if instruction_is_base_or_zero_tier(opcode) {
                            cost = cost.saturating_sub(1);
                        } else if instruction_is_below_high_tier(opcode) {
                            cost += 1;
                        } else {
                            cost += 49;
                        }
                    } else {
                        cost += 49;
                    }
                } else {
                    cost += 49;
                }
            }
            EXPRESSION_LITERAL => {
                let literal = expression;
                if literal.literal_kind == LITERAL_NUMBER && !literal.literal_unlimited {
                    let value = U256::from_big_endian(&literal.literal_value);
                    let mut bytes = 0usize;
                    let mut shifted = value;
                    while shifted >= U256::from(0x100u16) {
                        bytes += 1;
                        shifted >>= 8;
                    }
                    cost += bytes;
                    if value.is_zero() && self.request.dialect.evm_version >= 10 {
                        cost = cost.saturating_sub(1);
                    }
                } else if literal.literal_kind == crate::wire::LITERAL_STRING {
                    cost += self.request.strings[literal.literal_string_id as usize]
                        .bytes
                        .len();
                }
            }
            EXPRESSION_IDENTIFIER => {}
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid expression kind: {}",
                    expression.kind
                )));
            }
        }
        Ok(cost)
    }

    fn evm_opcode(&self, handle: u64) -> Option<u16> {
        self.request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == handle)
            .and_then(|builtin| builtin.has_evm_opcode.then_some(builtin.evm_opcode))
    }

    fn expression_references(&self, expression_id: u64) -> Result<BTreeSet<u64>, OptimizerError> {
        let mut references = BTreeSet::new();
        collect_expression_reference_set(self.request, expression_id, &mut references)?;
        Ok(references)
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

    fn push_scope(&mut self, is_function: bool) {
        self.variable_scopes.push(Scope {
            variables: BTreeSet::new(),
            is_function,
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

    fn in_scope(&self, variable_name: u64) -> bool {
        for scope in self.variable_scopes.iter().rev() {
            if scope.variables.contains(&variable_name) {
                return true;
            }
            if scope.is_function {
                return false;
            }
        }
        false
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

fn count_variable_references(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, usize>, OptimizerError> {
    let mut references = BTreeMap::new();
    collect_block_references(request, request.root_block_id, &mut references)?;
    Ok(references)
}

fn collect_block_references(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    references: &mut BTreeMap<u64, usize>,
) -> Result<(), OptimizerError> {
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        collect_statement_references(request, *statement_id, references)?;
    }
    Ok(())
}

fn collect_statement_references(
    request: &ffi::WireYulOptimizerRequest,
    statement_id: u64,
    references: &mut BTreeMap<u64, usize>,
) -> Result<(), OptimizerError> {
    let statement = &request.statements[statement_id as usize];
    match statement.kind {
        STATEMENT_EXPRESSION => {
            collect_expression_references(request, statement.expression_id, references)
        }
        STATEMENT_ASSIGNMENT => {
            for variable_id in &statement.variable_ids {
                let name = request.identifiers[*variable_id as usize].name_id;
                *references.entry(name).or_default() += 1;
            }
            collect_expression_references(request, statement.value_expression_id, references)
        }
        STATEMENT_VARIABLE_DECLARATION => {
            if statement.has_value {
                collect_expression_references(request, statement.value_expression_id, references)?;
            }
            Ok(())
        }
        STATEMENT_FUNCTION_DEFINITION => {
            collect_block_references(request, statement.body_block_id, references)
        }
        STATEMENT_IF => {
            collect_expression_references(request, statement.condition_expression_id, references)?;
            collect_block_references(request, statement.body_block_id, references)
        }
        STATEMENT_SWITCH => {
            collect_expression_references(request, statement.switch_expression_id, references)?;
            for case_id in &statement.case_ids {
                let switch_case = &request.cases[*case_id as usize];
                if switch_case.has_value {
                    collect_expression_references(
                        request,
                        switch_case.value_expression_id,
                        references,
                    )?;
                }
                collect_block_references(request, switch_case.body_block_id, references)?;
            }
            Ok(())
        }
        STATEMENT_FOR_LOOP => {
            collect_block_references(request, statement.pre_block_id, references)?;
            collect_expression_references(request, statement.condition_expression_id, references)?;
            collect_block_references(request, statement.post_block_id, references)?;
            collect_block_references(request, statement.body_block_id, references)
        }
        STATEMENT_BLOCK => collect_block_references(request, statement.block_id, references),
        _ => Ok(()),
    }
}

fn collect_expression_references(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    references: &mut BTreeMap<u64, usize>,
) -> Result<(), OptimizerError> {
    let expression = &request.expressions[expression_id as usize];
    match expression.kind {
        EXPRESSION_FUNCTION_CALL => {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                collect_expression_references(request, *argument_id, references)?;
            }
        }
        EXPRESSION_IDENTIFIER => {
            *references.entry(expression.name_id).or_default() += 1;
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

fn collect_expression_reference_set(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    references: &mut BTreeSet<u64>,
) -> Result<(), OptimizerError> {
    let expression = &request.expressions[expression_id as usize];
    match expression.kind {
        EXPRESSION_FUNCTION_CALL => {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                collect_expression_reference_set(request, *argument_id, references)?;
            }
        }
        EXPRESSION_IDENTIFIER => {
            references.insert(expression.name_id);
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

fn instruction_is_base_or_zero_tier(opcode: u16) -> bool {
    matches!(
        opcode,
        0x00 | 0x30
            | 0x32
            | 0x33
            | 0x34
            | 0x36
            | 0x38
            | 0x3a
            | 0x3d
            | 0x41
            | 0x42
            | 0x43
            | 0x44
            | 0x45
            | 0x46
            | 0x48
            | 0x4a
            | 0x50
            | 0x58
            | 0x59
            | 0x5a
            | 0x5f
    )
}

fn instruction_is_below_high_tier(opcode: u16) -> bool {
    !matches!(
        opcode,
        0x0a | 0x20 | 0x31 | 0x3b | 0x3c | 0x3f | 0x40 | 0x47 | 0x54
    )
}
