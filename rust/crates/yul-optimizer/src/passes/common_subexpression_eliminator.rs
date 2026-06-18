use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EFFECT_NONE, EFFECT_READ, EFFECT_WRITE, EXPRESSION_FUNCTION_CALL,
    EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

const LITERAL_ARGUMENT_UNRESTRICTED: u8 = 0xff;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FunctionHandle {
    Name(u64),
    Builtin(u64),
}

#[derive(Clone, Debug, Default)]
struct CallGraph {
    function_calls: BTreeMap<FunctionHandle, Vec<FunctionHandle>>,
    functions_with_loops: BTreeSet<FunctionHandle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SideEffects {
    movable: bool,
    movable_apart_from_effects: bool,
    can_be_removed: bool,
    can_be_removed_if_no_msize: bool,
    cannot_loop: bool,
    other_state: u8,
    storage: u8,
    memory: u8,
    transient_storage: u8,
}

impl Default for SideEffects {
    fn default() -> Self {
        Self {
            movable: true,
            movable_apart_from_effects: true,
            can_be_removed: true,
            can_be_removed_if_no_msize: true,
            cannot_loop: true,
            other_state: EFFECT_NONE,
            storage: EFFECT_NONE,
            memory: EFFECT_NONE,
            transient_storage: EFFECT_NONE,
        }
    }
}

impl SideEffects {
    fn worst() -> Self {
        Self {
            movable: false,
            movable_apart_from_effects: false,
            can_be_removed: false,
            can_be_removed_if_no_msize: false,
            cannot_loop: false,
            other_state: EFFECT_WRITE,
            storage: EFFECT_WRITE,
            memory: EFFECT_WRITE,
            transient_storage: EFFECT_WRITE,
        }
    }

    fn non_movable_looping() -> Self {
        Self {
            movable: false,
            can_be_removed: false,
            can_be_removed_if_no_msize: false,
            cannot_loop: false,
            ..Self::default()
        }
    }

    fn add_assign(&mut self, other: Self) {
        self.movable &= other.movable;
        self.movable_apart_from_effects &= other.movable_apart_from_effects;
        self.can_be_removed &= other.can_be_removed;
        self.can_be_removed_if_no_msize &= other.can_be_removed_if_no_msize;
        self.cannot_loop &= other.cannot_loop;
        self.other_state = combine_effect(self.other_state, other.other_state);
        self.storage = combine_effect(self.storage, other.storage);
        self.memory = combine_effect(self.memory, other.memory);
        self.transient_storage = combine_effect(self.transient_storage, other.transient_storage);
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FunctionNameKey {
    Identifier(u64),
    Builtin(u64),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LiteralValueKey {
    Numeric(Vec<u8>),
    Unlimited(Vec<u8>),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExprKey {
    FunctionCall {
        function_name: FunctionNameKey,
        arguments: Vec<ExprKey>,
    },
    Identifier(u64),
    Literal(LiteralValueKey),
}

#[derive(Clone, Copy, Debug)]
enum ValueRef {
    Expression(u64),
    Zero,
}

#[derive(Clone, Copy, Debug)]
struct AssignedValue {
    value: ValueRef,
    #[allow(dead_code)]
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
    side_effects: SideEffects,
    referenced_variables: BTreeSet<u64>,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let call_graph = call_graph(request, request.root_block_id)?;
    let function_side_effects = side_effects(request, &call_graph);
    let builtin_side_effects = builtin_side_effects(request);

    CommonSubexpressionEliminator {
        request,
        function_side_effects,
        builtin_side_effects,
        state: DataFlowState::default(),
        loop_depth: 0,
        variable_scopes: Vec::new(),
        return_variables: BTreeSet::new(),
        replacement_candidates: BTreeMap::new(),
    }
    .visit_block_root()
}

struct CommonSubexpressionEliminator<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    function_side_effects: BTreeMap<FunctionHandle, SideEffects>,
    builtin_side_effects: BTreeMap<u64, SideEffects>,
    state: DataFlowState,
    loop_depth: usize,
    variable_scopes: Vec<Scope>,
    return_variables: BTreeSet<u64>,
    replacement_candidates: BTreeMap<ExprKey, Vec<u64>>,
}

impl CommonSubexpressionEliminator<'_> {
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
                "scope stack mismatch in CommonSubexpressionEliminator".to_string(),
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
            STATEMENT_FUNCTION_DEFINITION => self.visit_function_definition(statement_id),
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
                    let case_body_id = self.request.cases[case_id as usize].body_block_id;
                    self.visit_block(case_body_id)?;
                    let variables = self.assigned_variable_names(case_body_id)?;
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

    fn visit_function_definition(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        let saved_return_variables = std::mem::take(&mut self.return_variables);
        let saved_replacement_candidates = std::mem::take(&mut self.replacement_candidates);

        for variable_id in &statement.return_variable_ids {
            self.return_variables
                .insert(self.request.names[*variable_id as usize].name_id);
        }

        let result = self.visit_function_definition_data_flow(&statement);
        self.return_variables = saved_return_variables;
        self.replacement_candidates = saved_replacement_candidates;
        result
    }

    fn visit_function_definition_data_flow(
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
                "CommonSubexpressionEliminator requires ForLoopInitRewriter.".to_string(),
            ));
        }

        self.loop_depth += 1;

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

        self.loop_depth -= 1;
        Ok(())
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        let mut descend = true;

        if expression.kind == EXPRESSION_FUNCTION_CALL
            && expression.function_name_kind == FUNCTION_NAME_BUILTIN
        {
            if let Some(literal_argument_kinds) = self
                .builtin(expression.function_name_builtin_handle)
                .map(|builtin| builtin.literal_argument_kinds.clone())
            {
                for (index, argument_id) in
                    expression.argument_expression_ids.iter().enumerate().rev()
                {
                    if !builtin_argument_must_stay_literal(&literal_argument_kinds, index) {
                        self.visit_expression(*argument_id)?;
                    }
                }
                descend = false;
            }
        }

        if descend {
            self.visit_expression_children(&expression)?;
        }

        self.try_replace_expression(expression_id)
    }

    fn visit_expression_children(
        &mut self,
        expression: &ffi::WireExpression,
    ) -> Result<(), OptimizerError> {
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }

    fn try_replace_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();

        if expression.kind == EXPRESSION_IDENTIFIER {
            if let Some(assigned_value) = self.state.value.get(&expression.name_id) {
                if let Some(value_name) = self.value_ref_identifier_name(assigned_value.value) {
                    if self.in_scope(value_name) {
                        self.replace_expression_with_identifier(expression_id, value_name);
                    }
                }
            }
            return Ok(());
        }

        let expression_key = self.expression_key(expression_id)?;
        let Some(candidates) = self.replacement_candidates.get(&expression_key).cloned() else {
            return Ok(());
        };

        for variable in candidates {
            let Some(assigned_value) = self.state.value.get(&variable).copied() else {
                continue;
            };

            if self.return_variables.contains(&variable)
                && self.value_ref_is_literal_zero(assigned_value.value)
            {
                continue;
            }

            if self.in_scope(variable)
                && self.value_ref_expression_key(assigned_value.value)? == expression_key
            {
                self.replace_expression_with_identifier(expression_id, variable);
                break;
            }
        }

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
                if movable.side_effects.movable && !referenced_variables.contains(&name) {
                    self.assign_value(name, ValueRef::Expression(value_id))?;
                }
            }
        } else {
            for name in &names {
                self.assign_value(*name, ValueRef::Zero)?;
            }
        }

        let sorted_references: Vec<u64> = referenced_variables.into_iter().collect();
        for name in names {
            self.state
                .sorted_references
                .insert(name, sorted_references.clone());
        }

        Ok(())
    }

    fn assign_value(&mut self, variable: u64, value: ValueRef) -> Result<(), OptimizerError> {
        let key = self.value_ref_expression_key(value)?;
        let candidates = self.replacement_candidates.entry(key).or_default();
        if !candidates.contains(&variable) {
            candidates.push(variable);
        }

        self.state.value.insert(
            variable,
            AssignedValue {
                value,
                loop_depth: self.loop_depth,
            },
        );
        Ok(())
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
        let mut info = MovableInfo::default();
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

                let handle = function_handle(expression)?;
                let side_effects = match handle {
                    FunctionHandle::Builtin(handle_id) => self
                        .builtin_side_effects
                        .get(&handle_id)
                        .copied()
                        .unwrap_or_else(SideEffects::worst),
                    FunctionHandle::Name(_) => self
                        .function_side_effects
                        .get(&handle)
                        .copied()
                        .unwrap_or_else(SideEffects::worst),
                };
                info.side_effects.add_assign(side_effects);
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

    fn expression_key(&self, expression_id: u64) -> Result<ExprKey, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                let function_name = match expression.function_name_kind {
                    FUNCTION_NAME_IDENTIFIER => {
                        FunctionNameKey::Identifier(expression.function_name_name_id)
                    }
                    FUNCTION_NAME_BUILTIN => {
                        FunctionNameKey::Builtin(expression.function_name_builtin_handle)
                    }
                    _ => {
                        return Err(OptimizerError::InvalidWire(format!(
                            "invalid function name kind: {}",
                            expression.function_name_kind
                        )));
                    }
                };
                let arguments = expression
                    .argument_expression_ids
                    .iter()
                    .map(|argument_id| self.expression_key(*argument_id))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ExprKey::FunctionCall {
                    function_name,
                    arguments,
                })
            }
            EXPRESSION_IDENTIFIER => Ok(ExprKey::Identifier(expression.name_id)),
            EXPRESSION_LITERAL => Ok(ExprKey::Literal(literal_value_key(
                expression,
                self.request,
            ))),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }

    fn value_ref_expression_key(&self, value: ValueRef) -> Result<ExprKey, OptimizerError> {
        match value {
            ValueRef::Expression(expression_id) => self.expression_key(expression_id),
            ValueRef::Zero => Ok(zero_expression_key()),
        }
    }

    fn value_ref_identifier_name(&self, value: ValueRef) -> Option<u64> {
        match value {
            ValueRef::Expression(expression_id) => {
                let expression = &self.request.expressions[expression_id as usize];
                (expression.kind == EXPRESSION_IDENTIFIER).then_some(expression.name_id)
            }
            ValueRef::Zero => None,
        }
    }

    fn value_ref_is_literal_zero(&self, value: ValueRef) -> bool {
        match value {
            ValueRef::Zero => true,
            ValueRef::Expression(expression_id) => {
                let expression = &self.request.expressions[expression_id as usize];
                expression.kind == EXPRESSION_LITERAL
                    && !expression.literal_unlimited
                    && expression.literal_value.iter().all(|byte| *byte == 0)
            }
        }
    }

    fn replace_expression_with_identifier(&mut self, expression_id: u64, name_id: u64) {
        let expression = &mut self.request.expressions[expression_id as usize];
        expression.kind = EXPRESSION_IDENTIFIER;
        expression.name_id = name_id;
        expression.argument_expression_ids.clear();
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

    fn builtin(&self, handle: u64) -> Option<&ffi::WireBuiltinFunction> {
        self.request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == handle)
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

fn call_graph(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<CallGraph, OptimizerError> {
    let outermost = FunctionHandle::Name(empty_string_id(request));
    let mut generator = CallGraphGenerator {
        request,
        graph: CallGraph {
            function_calls: BTreeMap::from([(outermost, Vec::new())]),
            functions_with_loops: BTreeSet::new(),
        },
        current_function: outermost,
    };
    generator.visit_block(block_id)?;
    Ok(generator.graph)
}

struct CallGraphGenerator<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    graph: CallGraph,
    current_function: FunctionHandle,
}

impl CallGraphGenerator<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_ASSIGNMENT => self.visit_expression(statement.value_expression_id),
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.visit_expression(statement.value_expression_id)?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                let previous_function = self.current_function;
                self.current_function = FunctionHandle::Name(statement.name_id);
                self.graph
                    .function_calls
                    .insert(self.current_function, Vec::new());
                self.visit_block(statement.body_block_id)?;
                self.current_function = previous_function;
                Ok(())
            }
            STATEMENT_IF => {
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(statement.switch_expression_id)?;
                for case_id in &statement.case_ids {
                    self.visit_block(self.request.cases[*case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.graph
                    .functions_with_loops
                    .insert(self.current_function);
                self.visit_block(statement.pre_block_id)?;
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)?;
                self.visit_block(statement.post_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            crate::wire::STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            let callee = function_handle(expression)?;
            let callees = self
                .graph
                .function_calls
                .entry(self.current_function)
                .or_default();
            if !callees.contains(&callee) {
                callees.push(callee);
            }

            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }
}

fn side_effects(
    request: &ffi::WireYulOptimizerRequest,
    call_graph: &CallGraph,
) -> BTreeMap<FunctionHandle, SideEffects> {
    let builtin_effects = builtin_side_effects(request);
    let mut output = BTreeMap::new();

    for function in &call_graph.functions_with_loops {
        output.insert(*function, SideEffects::non_movable_looping());
    }

    for function in recursive_functions(call_graph) {
        output.insert(function, SideEffects::non_movable_looping());
    }

    for (function, callees) in &call_graph.function_calls {
        let mut combined = SideEffects::default();
        for callee in callees {
            collect_side_effects(
                *callee,
                call_graph,
                &builtin_effects,
                &output,
                &mut BTreeSet::new(),
                &mut combined,
            );
        }
        output.entry(*function).or_default().add_assign(combined);
    }

    output
}

fn collect_side_effects(
    function: FunctionHandle,
    call_graph: &CallGraph,
    builtin_effects: &BTreeMap<u64, SideEffects>,
    known_effects: &BTreeMap<FunctionHandle, SideEffects>,
    visited: &mut BTreeSet<FunctionHandle>,
    side_effects: &mut SideEffects,
) {
    if !visited.insert(function) || *side_effects == SideEffects::worst() {
        return;
    }

    match function {
        FunctionHandle::Builtin(handle) => side_effects.add_assign(
            builtin_effects
                .get(&handle)
                .copied()
                .unwrap_or_else(SideEffects::worst),
        ),
        FunctionHandle::Name(_) => {
            if let Some(known) = known_effects.get(&function) {
                side_effects.add_assign(*known);
            }

            let Some(callees) = call_graph.function_calls.get(&function) else {
                side_effects.add_assign(SideEffects::worst());
                return;
            };
            for callee in callees {
                collect_side_effects(
                    *callee,
                    call_graph,
                    builtin_effects,
                    known_effects,
                    visited,
                    side_effects,
                );
            }
        }
    }
}

fn recursive_functions(call_graph: &CallGraph) -> BTreeSet<FunctionHandle> {
    let mut finder = CycleFinder {
        call_graph,
        contained_in_cycle: BTreeSet::new(),
        visited: BTreeSet::new(),
        current_path: Vec::new(),
    };

    for function in call_graph.function_calls.keys() {
        finder.visit(*function);
    }

    finder.contained_in_cycle
}

struct CycleFinder<'a> {
    call_graph: &'a CallGraph,
    contained_in_cycle: BTreeSet<FunctionHandle>,
    visited: BTreeSet<FunctionHandle>,
    current_path: Vec<FunctionHandle>,
}

impl CycleFinder<'_> {
    fn visit(&mut self, function: FunctionHandle) {
        if self.visited.contains(&function) {
            return;
        }

        if let Some(position) = self
            .current_path
            .iter()
            .position(|current| *current == function)
        {
            self.contained_in_cycle
                .extend(self.current_path[position..].iter().copied());
            return;
        }

        self.current_path.push(function);
        if let Some(children) = self.call_graph.function_calls.get(&function) {
            for child in children {
                self.visit(*child);
            }
        }
        self.current_path.pop();
        self.visited.insert(function);
    }
}

fn builtin_side_effects(request: &ffi::WireYulOptimizerRequest) -> BTreeMap<u64, SideEffects> {
    request
        .builtins
        .iter()
        .map(|builtin| {
            (
                builtin.handle_id,
                SideEffects {
                    movable: builtin.movable,
                    movable_apart_from_effects: builtin.movable_apart_from_effects,
                    can_be_removed: builtin.can_be_removed,
                    can_be_removed_if_no_msize: builtin.can_be_removed_if_no_msize,
                    cannot_loop: builtin.cannot_loop,
                    other_state: builtin.other_state,
                    storage: builtin.storage,
                    memory: builtin.memory,
                    transient_storage: builtin.transient_storage,
                },
            )
        })
        .collect()
}

fn function_handle(expression: &ffi::WireExpression) -> Result<FunctionHandle, OptimizerError> {
    match expression.function_name_kind {
        FUNCTION_NAME_IDENTIFIER => Ok(FunctionHandle::Name(expression.function_name_name_id)),
        FUNCTION_NAME_BUILTIN => Ok(FunctionHandle::Builtin(
            expression.function_name_builtin_handle,
        )),
        _ => Err(OptimizerError::InvalidWire(format!(
            "invalid function name kind: {}",
            expression.function_name_kind
        ))),
    }
}

fn literal_value_key(
    expression: &ffi::WireExpression,
    request: &ffi::WireYulOptimizerRequest,
) -> LiteralValueKey {
    if expression.literal_unlimited {
        LiteralValueKey::Unlimited(
            request.strings[expression.literal_string_id as usize]
                .bytes
                .clone(),
        )
    } else {
        LiteralValueKey::Numeric(expression.literal_value.clone())
    }
}

fn zero_expression_key() -> ExprKey {
    ExprKey::Literal(LiteralValueKey::Numeric(vec![0; 32]))
}

fn builtin_argument_must_stay_literal(literal_argument_kinds: &[u8], index: usize) -> bool {
    literal_argument_kinds
        .get(index)
        .is_some_and(|kind| *kind != LITERAL_ARGUMENT_UNRESTRICTED)
}

fn combine_effect(left: u8, right: u8) -> u8 {
    debug_assert!(matches!(left, EFFECT_NONE | EFFECT_READ | EFFECT_WRITE));
    debug_assert!(matches!(right, EFFECT_NONE | EFFECT_READ | EFFECT_WRITE));
    left.max(right)
}

fn empty_string_id(request: &ffi::WireYulOptimizerRequest) -> u64 {
    request
        .strings
        .iter()
        .position(|string| string.bytes.is_empty())
        .unwrap_or(0) as u64
}
