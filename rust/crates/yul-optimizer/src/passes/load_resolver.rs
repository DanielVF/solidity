use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EFFECT_NONE, EFFECT_READ, EFFECT_WRITE, EXPRESSION_FUNCTION_CALL,
    EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    LITERAL_NUMBER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use primitive_types::U256;
use sha3::{Digest, Keccak256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

type U256Bytes = [u8; 32];

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Default)]
struct Environment {
    memory: BTreeMap<u64, u64>,
    storage: BTreeMap<u64, u64>,
    keccak: BTreeMap<(u64, u64), u64>,
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

#[derive(Clone, Copy, Debug)]
struct VariableOffset {
    reference: Option<u64>,
    offset: U256Bytes,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let contains_msize = contains_msize(request)?;
    let call_graph = call_graph(request, request.root_block_id)?;
    let function_side_effects = side_effects(request, &call_graph);
    let builtin_side_effects = builtin_side_effects(request);

    LoadResolver {
        request,
        contains_msize,
        function_side_effects,
        builtin_side_effects,
        state: DataFlowState::default(),
        environment: Environment::default(),
        loop_depth: 0,
        variable_scopes: Vec::new(),
        offsets: BTreeMap::new(),
        last_known_value: BTreeMap::new(),
        group_members: BTreeMap::new(),
    }
    .visit_block_root()
}

struct LoadResolver<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    contains_msize: bool,
    function_side_effects: BTreeMap<FunctionHandle, SideEffects>,
    builtin_side_effects: BTreeMap<u64, SideEffects>,
    state: DataFlowState,
    environment: Environment,
    loop_depth: usize,
    variable_scopes: Vec<Scope>,
    offsets: BTreeMap<u64, VariableOffset>,
    last_known_value: BTreeMap<u64, Option<ValueRef>>,
    group_members: BTreeMap<u64, BTreeSet<u64>>,
}

impl LoadResolver<'_> {
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
                "scope stack mismatch in LoadResolver".to_string(),
            ));
        }
        result
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression_statement(statement.expression_id),
            STATEMENT_ASSIGNMENT => {
                let names = self.identifier_names(&statement.variable_ids);
                self.clear_knowledge_if_invalidated_expression(statement.value_expression_id)?;
                self.visit_expression(statement.value_expression_id)?;
                self.handle_assignment(names, Some(statement.value_expression_id), false)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                let names = self.name_with_debug_data_names(&statement.variable_ids);
                if let Some(scope) = self.variable_scopes.last_mut() {
                    scope.variables.extend(names.iter().copied());
                }
                if statement.has_value {
                    self.clear_knowledge_if_invalidated_expression(statement.value_expression_id)?;
                    self.visit_expression(statement.value_expression_id)?;
                    self.handle_assignment(names, Some(statement.value_expression_id), true)
                } else {
                    self.handle_assignment(names, None, true)
                }
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_function_definition(&statement),
            STATEMENT_IF => {
                self.clear_knowledge_if_invalidated_expression(statement.condition_expression_id)?;
                let pre_environment = self.environment.clone();
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)?;
                self.join_knowledge(&pre_environment);
                let assigned = self.assigned_variable_names(statement.body_block_id)?;
                self.clear_values(&assigned);
                Ok(())
            }
            STATEMENT_SWITCH => self.visit_switch(&statement),
            STATEMENT_FOR_LOOP => self.visit_for_loop(&statement),
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression_statement(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        if let Some((key, value)) = self.simple_store(StoreLocation::Storage, expression_id) {
            self.visit_expression(expression_id)?;
            let storage_snapshot = self.environment.storage.clone();
            let keys_to_remove = storage_snapshot
                .iter()
                .filter_map(|(slot_key, slot_value)| {
                    (!self.known_to_be_different(key, *slot_key) && value != *slot_value)
                        .then_some(*slot_key)
                })
                .collect::<Vec<_>>();
            for slot_key in keys_to_remove {
                self.environment.storage.remove(&slot_key);
            }
            self.environment.storage.insert(key, value);
        } else if let Some((key, value)) = self.simple_store(StoreLocation::Memory, expression_id) {
            self.visit_expression(expression_id)?;
            let memory_keys = self.environment.memory.keys().copied().collect::<Vec<_>>();
            let keys_to_remove = memory_keys
                .into_iter()
                .filter(|slot_key| !self.known_to_be_different_by_at_least_32(key, *slot_key))
                .collect::<Vec<_>>();
            for slot_key in keys_to_remove {
                self.environment.memory.remove(&slot_key);
            }
            self.environment.keccak.clear();
            self.environment.memory.insert(key, value);
        } else {
            self.clear_knowledge_if_invalidated_expression(expression_id)?;
            self.visit_expression(expression_id)?;
        }
        Ok(())
    }

    fn visit_function_definition(
        &mut self,
        statement: &ffi::WireStatement,
    ) -> Result<(), OptimizerError> {
        let saved_state = std::mem::take(&mut self.state);
        let saved_environment = std::mem::take(&mut self.environment);
        let saved_loop_depth = self.loop_depth;
        let saved_offsets = std::mem::take(&mut self.offsets);
        let saved_last_known_value = std::mem::take(&mut self.last_known_value);
        let saved_group_members = std::mem::take(&mut self.group_members);

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
        self.environment = saved_environment;
        self.loop_depth = saved_loop_depth;
        self.offsets = saved_offsets;
        self.last_known_value = saved_last_known_value;
        self.group_members = saved_group_members;
        result
    }

    fn visit_switch(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        self.clear_knowledge_if_invalidated_expression(statement.switch_expression_id)?;
        self.visit_expression(statement.switch_expression_id)?;
        let mut assigned_variables = BTreeSet::new();

        for case_id in &statement.case_ids {
            let body_block_id = self.request.cases[*case_id as usize].body_block_id;
            let pre_environment = self.environment.clone();
            self.visit_block(body_block_id)?;
            self.join_knowledge(&pre_environment);

            let variables = self.assigned_variable_names(body_block_id)?;
            assigned_variables.extend(variables.iter().copied());
            self.clear_values(&variables);
            self.clear_knowledge_if_invalidated_block(body_block_id)?;
        }

        for case_id in &statement.case_ids {
            self.clear_knowledge_if_invalidated_block(
                self.request.cases[*case_id as usize].body_block_id,
            )?;
        }
        self.clear_values(&assigned_variables);
        Ok(())
    }

    fn visit_for_loop(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "LoadResolver requires ForLoopInitRewriter.".to_string(),
            ));
        }

        self.loop_depth += 1;

        let assignments_since_continue =
            self.assignments_since_continue(statement.body_block_id)?;
        let mut assigned_variables = self.assigned_variable_names(statement.body_block_id)?;
        assigned_variables.extend(self.assigned_variable_names(statement.post_block_id)?);
        self.clear_values(&assigned_variables);

        self.clear_knowledge_if_invalidated_expression(statement.condition_expression_id)?;
        self.clear_knowledge_if_invalidated_block(statement.post_block_id)?;
        self.clear_knowledge_if_invalidated_block(statement.body_block_id)?;

        self.visit_expression(statement.condition_expression_id)?;
        self.visit_block(statement.body_block_id)?;
        self.clear_values(&assignments_since_continue);
        self.clear_knowledge_if_invalidated_block(statement.body_block_id)?;
        self.visit_block(statement.post_block_id)?;
        self.clear_values(&assigned_variables);

        self.clear_knowledge_if_invalidated_expression(statement.condition_expression_id)?;
        self.clear_knowledge_if_invalidated_block(statement.post_block_id)?;
        self.clear_knowledge_if_invalidated_block(statement.body_block_id)?;

        self.loop_depth -= 1;
        Ok(())
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        self.try_resolve_load(expression_id)
    }

    fn try_resolve_load(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
        {
            return Ok(());
        }

        let handle = expression.function_name_builtin_handle;
        if self.request.special_handles.has_storage_load
            && handle == self.request.special_handles.storage_load
        {
            if let Some(key) = self.identifier_expression_name(
                expression
                    .argument_expression_ids
                    .first()
                    .copied()
                    .unwrap_or(0),
            ) {
                if let Some(name_id) = self.environment.storage.get(&key).copied() {
                    if self.in_scope(name_id) {
                        self.request.expressions[expression_id as usize] =
                            identifier_expression(expression.debug_data_id, name_id);
                    }
                }
            }
        } else if !self.contains_msize
            && self.request.special_handles.has_memory_load
            && handle == self.request.special_handles.memory_load
        {
            if let Some(key) = self.identifier_expression_name(
                expression
                    .argument_expression_ids
                    .first()
                    .copied()
                    .unwrap_or(0),
            ) {
                if let Some(name_id) = self.environment.memory.get(&key).copied() {
                    if self.in_scope(name_id) {
                        self.request.expressions[expression_id as usize] =
                            identifier_expression(expression.debug_data_id, name_id);
                    }
                }
            }
        } else if !self.contains_msize
            && self.request.special_handles.has_hash
            && handle == self.request.special_handles.hash
        {
            if let Some((start, length)) = self.simple_keccak(expression_id) {
                if let Some(name_id) = self.environment.keccak.get(&(start, length)).copied() {
                    if self.in_scope(name_id) {
                        self.request.expressions[expression_id as usize] =
                            identifier_expression(expression.debug_data_id, name_id);
                        return Ok(());
                    }
                }
            }
            self.try_evaluate_keccak(expression_id)?;
        }
        Ok(())
    }

    fn try_evaluate_keccak(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        if !self.should_evaluate_keccak() {
            return Ok(());
        }

        let Some((memory_key, length)) = self.simple_keccak(expression_id) else {
            return Ok(());
        };
        let Some(value_name) = self.environment.memory.get(&memory_key).copied() else {
            return Ok(());
        };
        if !self.in_scope(value_name) {
            return Ok(());
        }

        let Some(memory_content) = self.value_of_identifier(value_name) else {
            return Ok(());
        };
        let Some(byte_length) = self.value_of_identifier(length) else {
            return Ok(());
        };
        if byte_length > U256::from(32u8) {
            return Ok(());
        }

        let mut content = [0u8; 32];
        memory_content.to_big_endian(&mut content);
        let byte_length = byte_length.as_usize();
        let hash = Keccak256::digest(&content[..byte_length]);

        let debug_data_id = self.request.expressions[expression_id as usize].debug_data_id;
        self.request.expressions[expression_id as usize] =
            number_literal_expression(debug_data_id, hash.to_vec());
        Ok(())
    }

    fn should_evaluate_keccak(&self) -> bool {
        let single_byte_data_gas = self.single_byte_data_gas();
        let cost_of_keccak = self.combined_gas_cost(42, 3 * single_byte_data_gas);
        let cost_of_literal =
            self.combined_gas_cost(3, single_byte_data_gas + 32 * single_byte_data_gas);
        cost_of_literal <= cost_of_keccak
    }

    fn combined_gas_cost(&self, run_gas: u128, data_gas: u128) -> u128 {
        if self.request.settings.has_expected_executions_per_deployment {
            run_gas * self.request.settings.expected_executions_per_deployment as u128 + data_gas
        } else {
            run_gas + data_gas
        }
    }

    fn single_byte_data_gas(&self) -> u128 {
        if self.request.settings.has_expected_executions_per_deployment {
            200
        } else if self.request.dialect.evm_version >= 6 {
            16
        } else {
            68
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

        let mut referenced_variables = BTreeSet::new();
        if let Some(value_id) = value {
            let movable = self.movable_info(value_id)?;
            referenced_variables = movable.referenced_variables;

            if names.len() == 1 {
                let name = *names.iter().next().expect("single item");
                if movable.side_effects.movable && !referenced_variables.contains(&name) {
                    self.assign_value(name, ValueRef::Expression(value_id));
                }
            }
        } else {
            for name in &names {
                self.assign_value(*name, ValueRef::Zero);
            }
        }

        let sorted_references: Vec<u64> = referenced_variables.iter().copied().collect();
        for name in &names {
            self.state
                .sorted_references
                .insert(*name, sorted_references.clone());
            if !is_declaration {
                self.clear_environment_for_variable(*name);
            }
        }

        if let Some(value_id) = value {
            if names.len() == 1 {
                let variable = *names.iter().next().expect("single item");
                if !referenced_variables.contains(&variable) {
                    if let Some(key) = self.simple_load(StoreLocation::Memory, value_id) {
                        self.environment.memory.insert(key, variable);
                    } else if let Some(key) = self.simple_load(StoreLocation::Storage, value_id) {
                        self.environment.storage.insert(key, variable);
                    } else if let Some(arguments) = self.simple_keccak(value_id) {
                        self.environment.keccak.insert(arguments, variable);
                    }
                }
            }
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
        self.environment.storage.retain(|key, value| {
            !variables_to_clear.contains(key) && !variables_to_clear.contains(value)
        });
        self.environment.memory.retain(|key, value| {
            !variables_to_clear.contains(key) && !variables_to_clear.contains(value)
        });
        self.environment.keccak.retain(|(start, length), value| {
            !variables_to_clear.contains(start)
                && !variables_to_clear.contains(length)
                && !variables_to_clear.contains(value)
        });

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

    fn clear_environment_for_variable(&mut self, variable: u64) {
        self.environment
            .storage
            .retain(|key, value| *key != variable && *value != variable);
        self.environment
            .memory
            .retain(|key, value| *key != variable && *value != variable);
        self.environment.keccak.retain(|(start, length), value| {
            *start != variable && *length != variable && *value != variable
        });
    }

    fn clear_knowledge_if_invalidated_expression(
        &mut self,
        expression_id: u64,
    ) -> Result<(), OptimizerError> {
        let side_effects = self.side_effects_expression(expression_id)?;
        self.clear_knowledge_for_side_effects(side_effects);
        Ok(())
    }

    fn clear_knowledge_if_invalidated_block(
        &mut self,
        block_id: u64,
    ) -> Result<(), OptimizerError> {
        let side_effects = self.side_effects_block(block_id)?;
        self.clear_knowledge_for_side_effects(side_effects);
        Ok(())
    }

    fn clear_knowledge_for_side_effects(&mut self, side_effects: SideEffects) {
        if side_effects.storage == EFFECT_WRITE {
            self.environment.storage.clear();
        }
        if side_effects.memory == EFFECT_WRITE {
            self.environment.memory.clear();
            self.environment.keccak.clear();
        }
    }

    fn join_knowledge(&mut self, older_environment: &Environment) {
        self.environment.storage.retain(|key, value| {
            older_environment
                .storage
                .get(key)
                .is_some_and(|old| old == value)
        });
        self.environment.memory.retain(|key, value| {
            older_environment
                .memory
                .get(key)
                .is_some_and(|old| old == value)
        });
        self.environment.keccak.retain(|key, value| {
            older_environment
                .keccak
                .get(key)
                .is_some_and(|old| old == value)
        });
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

                let side_effects = match function_handle(expression)? {
                    FunctionHandle::Builtin(handle_id) => self
                        .builtin_side_effects
                        .get(&handle_id)
                        .copied()
                        .unwrap_or_else(SideEffects::worst),
                    FunctionHandle::Name(name_id) => self
                        .function_side_effects
                        .get(&FunctionHandle::Name(name_id))
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

    fn side_effects_expression(&self, expression_id: u64) -> Result<SideEffects, OptimizerError> {
        let mut output = SideEffects::default();
        self.collect_expression_side_effects(expression_id, &mut output)?;
        Ok(output)
    }

    fn side_effects_block(&self, block_id: u64) -> Result<SideEffects, OptimizerError> {
        let mut output = SideEffects::default();
        self.collect_block_side_effects(block_id, &mut output)?;
        Ok(output)
    }

    fn collect_block_side_effects(
        &self,
        block_id: u64,
        output: &mut SideEffects,
    ) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.collect_statement_side_effects(*statement_id, output)?;
        }
        Ok(())
    }

    fn collect_statement_side_effects(
        &self,
        statement_id: u64,
        output: &mut SideEffects,
    ) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_EXPRESSION => {
                self.collect_expression_side_effects(statement.expression_id, output)
            }
            STATEMENT_ASSIGNMENT => {
                self.collect_expression_side_effects(statement.value_expression_id, output)
            }
            STATEMENT_VARIABLE_DECLARATION if statement.has_value => {
                self.collect_expression_side_effects(statement.value_expression_id, output)
            }
            STATEMENT_FUNCTION_DEFINITION => {
                self.collect_block_side_effects(statement.body_block_id, output)
            }
            STATEMENT_IF => {
                self.collect_expression_side_effects(statement.condition_expression_id, output)?;
                self.collect_block_side_effects(statement.body_block_id, output)
            }
            STATEMENT_SWITCH => {
                self.collect_expression_side_effects(statement.switch_expression_id, output)?;
                for case_id in &statement.case_ids {
                    self.collect_block_side_effects(
                        self.request.cases[*case_id as usize].body_block_id,
                        output,
                    )?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.collect_block_side_effects(statement.pre_block_id, output)?;
                self.collect_expression_side_effects(statement.condition_expression_id, output)?;
                self.collect_block_side_effects(statement.post_block_id, output)?;
                self.collect_block_side_effects(statement.body_block_id, output)
            }
            STATEMENT_BLOCK => self.collect_block_side_effects(statement.block_id, output),
            _ => Ok(()),
        }
    }

    fn collect_expression_side_effects(
        &self,
        expression_id: u64,
        output: &mut SideEffects,
    ) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL {
            return Ok(());
        }

        for argument_id in expression.argument_expression_ids.iter().rev() {
            self.collect_expression_side_effects(*argument_id, output)?;
        }

        let side_effects = match function_handle(expression)? {
            FunctionHandle::Builtin(handle_id) => self
                .builtin_side_effects
                .get(&handle_id)
                .copied()
                .unwrap_or_else(SideEffects::worst),
            FunctionHandle::Name(name_id) => self
                .function_side_effects
                .get(&FunctionHandle::Name(name_id))
                .copied()
                .unwrap_or_else(SideEffects::worst),
        };
        output.add_assign(side_effects);
        Ok(())
    }

    fn simple_store(&self, location: StoreLocation, expression_id: u64) -> Option<(u64, u64)> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
            || expression.argument_expression_ids.len() < 2
            || Some(expression.function_name_builtin_handle) != self.store_handle(location)
        {
            return None;
        }

        let key = self.identifier_expression_name(expression.argument_expression_ids[0])?;
        let value = self.identifier_expression_name(*expression.argument_expression_ids.last()?)?;
        Some((key, value))
    }

    fn simple_load(&self, location: StoreLocation, expression_id: u64) -> Option<u64> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
            || expression.argument_expression_ids.is_empty()
            || Some(expression.function_name_builtin_handle) != self.load_handle(location)
        {
            return None;
        }

        self.identifier_expression_name(expression.argument_expression_ids[0])
    }

    fn simple_keccak(&self, expression_id: u64) -> Option<(u64, u64)> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
            || !self.request.special_handles.has_hash
            || expression.function_name_builtin_handle != self.request.special_handles.hash
            || expression.argument_expression_ids.len() != 2
        {
            return None;
        }

        let start = self.identifier_expression_name(expression.argument_expression_ids[0])?;
        let length = self.identifier_expression_name(expression.argument_expression_ids[1])?;
        Some((start, length))
    }

    fn identifier_expression_name(&self, expression_id: u64) -> Option<u64> {
        let expression = self.request.expressions.get(expression_id as usize)?;
        (expression.kind == EXPRESSION_IDENTIFIER).then_some(expression.name_id)
    }

    fn store_handle(&self, location: StoreLocation) -> Option<u64> {
        match location {
            StoreLocation::Memory => self
                .request
                .special_handles
                .has_memory_store
                .then_some(self.request.special_handles.memory_store),
            StoreLocation::Storage => self
                .request
                .special_handles
                .has_storage_store
                .then_some(self.request.special_handles.storage_store),
        }
    }

    fn load_handle(&self, location: StoreLocation) -> Option<u64> {
        match location {
            StoreLocation::Memory => self
                .request
                .special_handles
                .has_memory_load
                .then_some(self.request.special_handles.memory_load),
            StoreLocation::Storage => self
                .request
                .special_handles
                .has_storage_load
                .then_some(self.request.special_handles.storage_load),
        }
    }

    fn known_to_be_different(&mut self, left: u64, right: u64) -> bool {
        self.difference_if_known_constant(left, right)
            .is_some_and(|difference| !u256_is_zero(&difference))
    }

    fn known_to_be_different_by_at_least_32(&mut self, left: u64, right: u64) -> bool {
        self.difference_if_known_constant(left, right)
            .is_some_and(|difference| {
                cmp_u256(&difference, &u256_from_u64(32)) != Ordering::Less
                    && cmp_u256(&difference, &u256_minus_u64(32)) != Ordering::Greater
            })
    }

    fn difference_if_known_constant(&mut self, left: u64, right: u64) -> Option<U256Bytes> {
        let left_offset = self.explore_variable(left);
        let right_offset = self.explore_variable(right);
        (left_offset.reference == right_offset.reference)
            .then_some(sub_u256(&left_offset.offset, &right_offset.offset))
    }

    fn explore_variable(&mut self, variable: u64) -> VariableOffset {
        let value = self.value_of(variable);
        if let Some(offset) = self.offsets.get(&variable) {
            return *offset;
        }

        if let Some(value) = value {
            if let Some(offset) = self.explore_value(value) {
                return self.set_offset(variable, offset);
            }
        }

        self.set_offset(
            variable,
            VariableOffset {
                reference: Some(variable),
                offset: u256_zero(),
            },
        )
    }

    fn explore_value(&mut self, value: ValueRef) -> Option<VariableOffset> {
        match value {
            ValueRef::Zero => Some(VariableOffset {
                reference: None,
                offset: u256_zero(),
            }),
            ValueRef::Expression(expression_id) => self.explore_expression(expression_id),
        }
    }

    fn explore_expression(&mut self, expression_id: u64) -> Option<VariableOffset> {
        let expression = self.request.expressions[expression_id as usize].clone();
        match expression.kind {
            EXPRESSION_LITERAL if !expression.literal_unlimited => Some(VariableOffset {
                reference: None,
                offset: u256_from_slice(&expression.literal_value)?,
            }),
            EXPRESSION_IDENTIFIER => Some(self.explore_variable(expression.name_id)),
            EXPRESSION_FUNCTION_CALL
                if expression.function_name_kind == FUNCTION_NAME_BUILTIN
                    && expression.argument_expression_ids.len() >= 2 =>
            {
                if self.request.special_handles.has_add
                    && expression.function_name_builtin_handle == self.request.special_handles.add
                {
                    let left = self.explore_expression(expression.argument_expression_ids[0])?;
                    let right = self.explore_expression(expression.argument_expression_ids[1])?;
                    let offset = add_u256(&left.offset, &right.offset);
                    if left.reference.is_none() {
                        Some(VariableOffset {
                            reference: right.reference,
                            offset,
                        })
                    } else if right.reference.is_none() {
                        Some(VariableOffset {
                            reference: left.reference,
                            offset,
                        })
                    } else {
                        None
                    }
                } else if self.request.special_handles.has_sub
                    && expression.function_name_builtin_handle == self.request.special_handles.sub
                {
                    let left = self.explore_expression(expression.argument_expression_ids[0])?;
                    let right = self.explore_expression(expression.argument_expression_ids[1])?;
                    let offset = sub_u256(&left.offset, &right.offset);
                    if left.reference == right.reference {
                        Some(VariableOffset {
                            reference: None,
                            offset,
                        })
                    } else if right.reference.is_none() {
                        Some(VariableOffset {
                            reference: left.reference,
                            offset,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn value_of(&mut self, variable: u64) -> Option<ValueRef> {
        let current_value = self.state.value.get(&variable).map(|value| value.value);
        if self.last_known_value.get(&variable).copied().flatten() != current_value {
            self.reset_knowledge_base_variable(variable);
        }
        self.last_known_value.insert(variable, current_value);
        current_value
    }

    fn reset_knowledge_base_variable(&mut self, variable: u64) {
        self.last_known_value.remove(&variable);
        if let Some(offset) = self.offsets.remove(&variable) {
            if let Some(reference) = offset.reference {
                if let Some(group) = self.group_members.get_mut(&reference) {
                    group.remove(&variable);
                }
            }
        }

        let Some(mut group) = self.group_members.remove(&variable) else {
            return;
        };
        if group.is_empty() {
            return;
        }

        let new_representative = *group.iter().next().expect("non-empty group");
        let new_offset = self
            .offsets
            .get(&new_representative)
            .map(|offset| offset.offset)
            .unwrap_or_else(u256_zero);

        let members = group.iter().copied().collect::<Vec<_>>();
        for member in members {
            if let Some(offset) = self.offsets.get_mut(&member) {
                offset.reference = Some(new_representative);
                offset.offset = sub_u256(&offset.offset, &new_offset);
            }
        }
        self.group_members
            .entry(new_representative)
            .or_default()
            .append(&mut group);
    }

    fn set_offset(&mut self, variable: u64, value: VariableOffset) -> VariableOffset {
        self.offsets.insert(variable, value);
        if let Some(reference) = value.reference {
            self.group_members
                .entry(reference)
                .or_default()
                .insert(variable);
        }
        value
    }

    fn value_of_identifier(&self, variable: u64) -> Option<U256> {
        match self.state.value.get(&variable)?.value {
            ValueRef::Zero => Some(U256::zero()),
            ValueRef::Expression(expression_id) => {
                let expression = &self.request.expressions[expression_id as usize];
                if expression.kind == EXPRESSION_LITERAL && !expression.literal_unlimited {
                    Some(U256::from_big_endian(&expression.literal_value))
                } else {
                    None
                }
            }
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

#[derive(Clone, Copy)]
enum StoreLocation {
    Memory,
    Storage,
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
            STATEMENT_CONTINUE => {
                if self.for_loop_depth == 0 {
                    self.continue_found = true;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn contains_msize(request: &ffi::WireYulOptimizerRequest) -> Result<bool, OptimizerError> {
    let mut finder = MSizeFinder {
        request,
        found: false,
    };
    finder.visit_block(request.root_block_id)?;
    Ok(finder.found)
}

struct MSizeFinder<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    found: bool,
}

impl MSizeFinder<'_> {
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
                && self.request.builtins.iter().any(|builtin| {
                    builtin.handle_id == expression.function_name_builtin_handle && builtin.is_msize
                })
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
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
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

fn number_literal_expression(debug_data_id: u64, literal_value: Vec<u8>) -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_LITERAL,
        debug_data_id,
        literal_kind: LITERAL_NUMBER,
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

fn u256_zero() -> U256Bytes {
    [0; 32]
}

fn u256_from_u64(value: u64) -> U256Bytes {
    let mut output = [0; 32];
    output[24..].copy_from_slice(&value.to_be_bytes());
    output
}

fn u256_minus_u64(value: u64) -> U256Bytes {
    sub_u256(&u256_zero(), &u256_from_u64(value))
}

fn u256_from_slice(value: &[u8]) -> Option<U256Bytes> {
    if value.len() != 32 {
        return None;
    }
    let mut output = [0; 32];
    output.copy_from_slice(value);
    Some(output)
}

fn u256_is_zero(value: &U256Bytes) -> bool {
    value.iter().all(|byte| *byte == 0)
}

fn cmp_u256(left: &U256Bytes, right: &U256Bytes) -> Ordering {
    left.cmp(right)
}

fn add_u256(left: &U256Bytes, right: &U256Bytes) -> U256Bytes {
    let mut output = [0; 32];
    let mut carry = 0u16;
    for index in (0..32).rev() {
        let sum = left[index] as u16 + right[index] as u16 + carry;
        output[index] = sum as u8;
        carry = sum >> 8;
    }
    output
}

fn sub_u256(left: &U256Bytes, right: &U256Bytes) -> U256Bytes {
    let mut output = [0; 32];
    let mut borrow = 0i16;
    for index in (0..32).rev() {
        let difference = left[index] as i16 - right[index] as i16 - borrow;
        if difference < 0 {
            output[index] = (difference + 256) as u8;
            borrow = 1;
        } else {
            output[index] = difference as u8;
            borrow = 0;
        }
    }
    output
}
