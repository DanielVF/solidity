use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EFFECT_NONE, EFFECT_READ, EFFECT_WRITE, EXPRESSION_FUNCTION_CALL,
    EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use primitive_types::U256;
use std::collections::{BTreeMap, BTreeSet};

const KECCAK256: u16 = 0x20;
const CALLDATACOPY: u16 = 0x37;
const CODECOPY: u16 = 0x39;
const EXTCODECOPY: u16 = 0x3c;
const RETURNDATASIZE: u16 = 0x3d;
const RETURNDATACOPY: u16 = 0x3e;
const MLOAD: u16 = 0x51;
const MSTORE: u16 = 0x52;
const MSTORE8: u16 = 0x53;
const SLOAD: u16 = 0x54;
const SSTORE: u16 = 0x55;
const MCOPY: u16 = 0x5e;
const LOG0: u16 = 0xa0;
const LOG4: u16 = 0xa4;
const CREATE: u16 = 0xf0;
const CALL: u16 = 0xf1;
const CALLCODE: u16 = 0xf2;
const RETURN: u16 = 0xf3;
const DELEGATECALL: u16 = 0xf4;
const CREATE2: u16 = 0xf5;
const STATICCALL: u16 = 0xfa;
const REVERT: u16 = 0xfd;

type ActiveStores = BTreeMap<Location, BTreeSet<u64>>;

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
struct ControlFlowSideEffects {
    can_continue: bool,
    can_terminate: bool,
}

impl Default for ControlFlowSideEffects {
    fn default() -> Self {
        Self {
            can_continue: true,
            can_terminate: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ForLoopInfo {
    pending_break_stmts: Vec<ActiveStores>,
    pending_continue_stmts: Vec<ActiveStores>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Location {
    Memory,
    Storage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Effect {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationLength {
    Name(u64),
    Constant(U256),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Operation {
    location: Location,
    effect: Effect,
    start: Option<u64>,
    length: Option<OperationLength>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueRef {
    Expression(u64),
    Zero,
}

#[derive(Clone, Copy, Debug)]
struct VariableOffset {
    reference: Option<u64>,
    offset: U256,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let call_graph = call_graph(request, request.root_block_id)?;
    let function_side_effects = side_effects(request, &call_graph);
    let builtin_side_effects = builtin_side_effects(request);
    let control_flow_side_effects = control_flow_side_effects(request)?;
    let ssa_values = ssa_values(request)?;
    let ignore_memory = contains_msize(request)?;

    let mut eliminator = UnusedStoreEliminator {
        request,
        function_side_effects,
        builtin_side_effects,
        control_flow_side_effects,
        ssa_values,
        ignore_memory,
        all_stores: BTreeSet::new(),
        used_stores: BTreeSet::new(),
        stores_to_remove: BTreeSet::new(),
        active_stores: ActiveStores::new(),
        for_loop_info: ForLoopInfo::default(),
        for_loop_nesting_depth: 0,
        store_operations: BTreeMap::new(),
        offsets: BTreeMap::new(),
        last_known_value: BTreeMap::new(),
        group_members: BTreeMap::new(),
    };
    eliminator.visit_block(eliminator.request.root_block_id)?;

    if eliminator.request.dialect.provides_object_access {
        eliminator.clear_active(Some(Location::Memory));
    } else {
        eliminator.mark_active_as_used(Some(Location::Memory));
    }
    eliminator.mark_active_as_used(Some(Location::Storage));
    eliminator.stores_to_remove.extend(
        eliminator
            .all_stores
            .difference(&eliminator.used_stores)
            .copied(),
    );
    eliminator.remove_pending_statements();
    Ok(())
}

struct UnusedStoreEliminator<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    function_side_effects: BTreeMap<FunctionHandle, SideEffects>,
    builtin_side_effects: BTreeMap<u64, SideEffects>,
    control_flow_side_effects: BTreeMap<u64, ControlFlowSideEffects>,
    ssa_values: BTreeMap<u64, ValueRef>,
    ignore_memory: bool,
    all_stores: BTreeSet<u64>,
    used_stores: BTreeSet<u64>,
    stores_to_remove: BTreeSet<u64>,
    active_stores: ActiveStores,
    for_loop_info: ForLoopInfo,
    for_loop_nesting_depth: usize,
    store_operations: BTreeMap<u64, Operation>,
    offsets: BTreeMap<u64, VariableOffset>,
    last_known_value: BTreeMap<u64, Option<ValueRef>>,
    group_members: BTreeMap<u64, BTreeSet<u64>>,
}

impl UnusedStoreEliminator<'_> {
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
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id)?,
            STATEMENT_ASSIGNMENT => self.visit_expression(statement.value_expression_id)?,
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.visit_expression(statement.value_expression_id)?;
                }
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_function_definition(&statement)?,
            STATEMENT_IF => self.visit_if(&statement)?,
            STATEMENT_SWITCH => self.visit_switch(&statement)?,
            STATEMENT_FOR_LOOP => self.visit_for_loop(&statement)?,
            STATEMENT_BREAK => self.visit_break(),
            STATEMENT_CONTINUE => self.visit_continue(),
            STATEMENT_LEAVE => self.visit_leave(),
            STATEMENT_BLOCK => self.visit_block(statement.block_id)?,
            _ => {}
        }

        if statement.kind == STATEMENT_EXPRESSION {
            self.maybe_record_store(statement_id, statement.expression_id)?;
        }
        Ok(())
    }

    fn visit_if(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        self.visit_expression(statement.condition_expression_id)?;
        let skip_branch = self.active_stores.clone();
        self.visit_block(statement.body_block_id)?;
        merge_active_stores(&mut self.active_stores, skip_branch);
        Ok(())
    }

    fn visit_switch(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        self.visit_expression(statement.switch_expression_id)?;
        let pre_state = self.active_stores.clone();
        let mut has_default = false;
        let mut branches = Vec::new();

        for case_id in &statement.case_ids {
            let switch_case = self.request.cases[*case_id as usize].clone();
            if !switch_case.has_value {
                has_default = true;
            }
            self.visit_block(switch_case.body_block_id)?;
            branches.push(std::mem::take(&mut self.active_stores));
            self.active_stores = pre_state.clone();
        }

        if has_default {
            self.active_stores = branches.pop().unwrap_or_default();
        }
        for branch in branches {
            merge_active_stores(&mut self.active_stores, branch);
        }
        Ok(())
    }

    fn visit_function_definition(
        &mut self,
        statement: &ffi::WireStatement,
    ) -> Result<(), OptimizerError> {
        let saved_all_stores = std::mem::take(&mut self.all_stores);
        let saved_used_stores = std::mem::take(&mut self.used_stores);
        let saved_active_stores = std::mem::take(&mut self.active_stores);
        let saved_for_loop_info = std::mem::take(&mut self.for_loop_info);
        let saved_for_loop_nesting_depth = self.for_loop_nesting_depth;
        let saved_store_operations = std::mem::take(&mut self.store_operations);

        self.for_loop_nesting_depth = 0;
        self.visit_block(statement.body_block_id)?;
        self.mark_active_as_used(None);
        self.stores_to_remove
            .extend(self.all_stores.difference(&self.used_stores).copied());

        self.all_stores = saved_all_stores;
        self.used_stores = saved_used_stores;
        self.active_stores = saved_active_stores;
        self.for_loop_info = saved_for_loop_info;
        self.for_loop_nesting_depth = saved_for_loop_nesting_depth;
        self.store_operations = saved_store_operations;
        Ok(())
    }

    fn visit_for_loop(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "UnusedStoreEliminator requires ForLoopInitRewriter.".to_string(),
            ));
        }

        let outer_for_loop_info = std::mem::take(&mut self.for_loop_info);
        let outer_for_loop_nesting_depth = self.for_loop_nesting_depth;
        self.for_loop_nesting_depth += 1;

        self.visit_expression(statement.condition_expression_id)?;
        let zero_runs = self.active_stores.clone();

        self.visit_block(statement.body_block_id)?;
        merge_active_store_vec(
            &mut self.active_stores,
            std::mem::take(&mut self.for_loop_info.pending_continue_stmts),
        );
        self.visit_block(statement.post_block_id)?;

        self.visit_expression(statement.condition_expression_id)?;

        if self.for_loop_nesting_depth < 6 {
            let one_run = self.active_stores.clone();

            self.visit_block(statement.body_block_id)?;
            merge_active_store_vec(
                &mut self.active_stores,
                std::mem::take(&mut self.for_loop_info.pending_continue_stmts),
            );
            self.visit_block(statement.post_block_id)?;

            self.visit_expression(statement.condition_expression_id)?;
            merge_active_stores(&mut self.active_stores, one_run);
        } else {
            self.mark_active_as_used(None);
        }

        merge_active_stores(&mut self.active_stores, zero_runs);
        merge_active_store_vec(
            &mut self.active_stores,
            std::mem::take(&mut self.for_loop_info.pending_break_stmts),
        );

        self.for_loop_info = outer_for_loop_info;
        self.for_loop_nesting_depth = outer_for_loop_nesting_depth;
        Ok(())
    }

    fn visit_break(&mut self) {
        self.for_loop_info
            .pending_break_stmts
            .push(std::mem::take(&mut self.active_stores));
    }

    fn visit_continue(&mut self) {
        self.for_loop_info
            .pending_continue_stmts
            .push(std::mem::take(&mut self.active_stores));
    }

    fn visit_leave(&mut self) {
        self.mark_active_as_used(None);
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }

                for operation in self.operations_from_function_call(&expression)? {
                    self.apply_operation(operation);
                }

                let side_effects = self.control_flow_for_call(&expression);
                if side_effects.can_terminate {
                    self.mark_active_as_used(Some(Location::Storage));
                }
                if !side_effects.can_continue {
                    self.clear_active(Some(Location::Memory));
                    if !side_effects.can_terminate {
                        self.clear_active(Some(Location::Storage));
                    }
                }
            }
            EXPRESSION_IDENTIFIER | EXPRESSION_LITERAL => {}
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid expression kind: {}",
                    expression.kind
                )));
            }
        }
        Ok(())
    }

    fn maybe_record_store(
        &mut self,
        statement_id: u64,
        expression_id: u64,
    ) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
            || !expression_arguments_identifier_or_literal(self.request, &expression)
        {
            return Ok(());
        }

        let Some(instruction) = self.evm_instruction(&expression) else {
            return Ok(());
        };

        let is_storage_write = instruction == SSTORE;
        let is_memory_write = matches!(
            instruction,
            EXTCODECOPY | CODECOPY | CALLDATACOPY | RETURNDATACOPY | MSTORE | MSTORE8
        );
        let is_candidate =
            instruction != MCOPY && (is_storage_write || (!self.ignore_memory && is_memory_write));
        if !is_candidate {
            return Ok(());
        }

        if instruction == RETURNDATACOPY && !self.returndatacopy_can_be_removed(&expression) {
            return Ok(());
        }

        let operations = self.operations_from_function_call(&expression)?;
        if operations.len() != 1 {
            return Ok(());
        }
        let operation = operations[0];

        self.all_stores.insert(statement_id);
        match operation.location {
            Location::Storage => self
                .active_stores
                .entry(Location::Storage)
                .or_default()
                .insert(statement_id),
            Location::Memory => self
                .active_stores
                .entry(Location::Memory)
                .or_default()
                .insert(statement_id),
        };
        self.store_operations.insert(statement_id, operation);
        Ok(())
    }

    fn returndatacopy_can_be_removed(&mut self, expression: &ffi::WireExpression) -> bool {
        let Some(start_offset) = expression
            .argument_expression_ids
            .get(1)
            .and_then(|id| self.identifier_name_if_ssa(*id))
        else {
            return false;
        };
        let Some(length) = expression
            .argument_expression_ids
            .get(2)
            .and_then(|id| self.identifier_name_if_ssa(*id))
        else {
            return false;
        };

        if !self.known_to_be_zero(start_offset) {
            return false;
        }

        let Some(ValueRef::Expression(length_value_id)) = self.ssa_values.get(&length).copied()
        else {
            return false;
        };
        let length_value = &self.request.expressions[length_value_id as usize];
        length_value.kind == EXPRESSION_FUNCTION_CALL
            && self.evm_instruction(length_value) == Some(RETURNDATASIZE)
    }

    fn operations_from_function_call(
        &self,
        expression: &ffi::WireExpression,
    ) -> Result<Vec<Operation>, OptimizerError> {
        let side_effects = match function_handle(expression)? {
            FunctionHandle::Builtin(handle) => self
                .builtin_side_effects
                .get(&handle)
                .copied()
                .unwrap_or_else(SideEffects::worst),
            FunctionHandle::Name(name) => self
                .function_side_effects
                .get(&FunctionHandle::Name(name))
                .copied()
                .unwrap_or_else(SideEffects::worst),
        };

        let Some(instruction) = self.evm_instruction(expression) else {
            let mut result = Vec::new();
            if side_effects.memory != EFFECT_NONE {
                result.push(Operation {
                    location: Location::Memory,
                    effect: Effect::Read,
                    start: None,
                    length: None,
                });
            }
            if side_effects.storage != EFFECT_NONE {
                result.push(Operation {
                    location: Location::Storage,
                    effect: Effect::Read,
                    start: None,
                    length: None,
                });
            }
            return Ok(result);
        };

        Ok(self.read_write_operations(instruction, &expression.argument_expression_ids))
    }

    fn read_write_operations(&self, instruction: u16, arguments: &[u64]) -> Vec<Operation> {
        let operation = |location: Location,
                         effect: Effect,
                         start_parameter: Option<usize>,
                         length_parameter: Option<usize>,
                         length_constant: Option<U256>|
         -> Operation {
            Operation {
                location,
                effect,
                start: start_parameter
                    .and_then(|parameter| arguments.get(parameter))
                    .and_then(|expression_id| self.identifier_name_if_ssa(*expression_id)),
                length: length_parameter
                    .and_then(|parameter| arguments.get(parameter))
                    .and_then(|expression_id| self.identifier_name_if_ssa(*expression_id))
                    .map(OperationLength::Name)
                    .or_else(|| length_constant.map(OperationLength::Constant)),
            }
        };

        match instruction {
            SSTORE => vec![operation(
                Location::Storage,
                Effect::Write,
                Some(0),
                None,
                Some(U256::one()),
            )],
            SLOAD => vec![operation(
                Location::Storage,
                Effect::Read,
                Some(0),
                None,
                Some(U256::one()),
            )],
            MSTORE => vec![operation(
                Location::Memory,
                Effect::Write,
                Some(0),
                None,
                Some(U256::from(32u8)),
            )],
            MSTORE8 => vec![operation(
                Location::Memory,
                Effect::Write,
                Some(0),
                None,
                Some(U256::one()),
            )],
            MLOAD => vec![operation(
                Location::Memory,
                Effect::Read,
                Some(0),
                None,
                Some(U256::from(32u8)),
            )],
            RETURN | REVERT | KECCAK256 | LOG0..=LOG4 => vec![operation(
                Location::Memory,
                Effect::Read,
                Some(0),
                Some(1),
                None,
            )],
            EXTCODECOPY => vec![operation(
                Location::Memory,
                Effect::Write,
                Some(1),
                Some(3),
                None,
            )],
            CODECOPY | CALLDATACOPY | RETURNDATACOPY => vec![operation(
                Location::Memory,
                Effect::Write,
                Some(0),
                Some(2),
                None,
            )],
            MCOPY => vec![
                operation(Location::Memory, Effect::Read, Some(1), Some(2), None),
                operation(Location::Memory, Effect::Write, Some(0), Some(2), None),
            ],
            STATICCALL | CALL | CALLCODE | DELEGATECALL => {
                let param_count = if matches!(instruction, CALL | CALLCODE) {
                    7
                } else {
                    6
                };
                let mut operations = vec![
                    operation(
                        Location::Memory,
                        Effect::Read,
                        Some(param_count - 4),
                        Some(param_count - 3),
                        None,
                    ),
                    Operation {
                        location: Location::Storage,
                        effect: Effect::Read,
                        start: None,
                        length: None,
                    },
                ];
                if instruction != STATICCALL {
                    operations.push(Operation {
                        location: Location::Storage,
                        effect: Effect::Write,
                        start: None,
                        length: None,
                    });
                }
                operations.push(operation(
                    Location::Memory,
                    Effect::Write,
                    Some(param_count - 2),
                    None,
                    None,
                ));
                operations
            }
            CREATE | CREATE2 => vec![
                operation(Location::Memory, Effect::Read, Some(1), Some(2), None),
                Operation {
                    location: Location::Storage,
                    effect: Effect::Read,
                    start: None,
                    length: None,
                },
                Operation {
                    location: Location::Storage,
                    effect: Effect::Write,
                    start: None,
                    length: None,
                },
            ],
            _ => Vec::new(),
        }
    }

    fn apply_operation(&mut self, operation: Operation) {
        let active = self
            .active_stores
            .entry(operation.location)
            .or_default()
            .clone();
        let mut retained = BTreeSet::new();

        for statement_id in active {
            let Some(store_operation) = self.store_operations.get(&statement_id).copied() else {
                continue;
            };
            if operation.effect == Effect::Read && !self.known_unrelated(store_operation, operation)
            {
                self.used_stores.insert(statement_id);
            } else if operation.effect == Effect::Write
                && self.known_covered(store_operation, operation)
            {
            } else {
                retained.insert(statement_id);
            }
        }

        self.active_stores.insert(operation.location, retained);
    }

    fn known_unrelated(&mut self, op1: Operation, op2: Operation) -> bool {
        if op1.location != op2.location {
            return true;
        }
        if op1.location == Location::Storage {
            if let (Some(start1), Some(start2)) = (op1.start, op2.start) {
                return self.known_to_be_different(start1, start2);
            }
        } else {
            if op1
                .length
                .is_some_and(|length| self.length_value(length) == Some(U256::zero()))
                || op2
                    .length
                    .is_some_and(|length| self.length_value(length) == Some(U256::zero()))
            {
                return true;
            }

            if let (Some(start1), Some(length1), Some(start2)) = (op1.start, op1.length, op2.start)
            {
                let length1 = self.length_value(length1);
                let start1_value = self.value_if_known_constant(start1);
                let start2_value = self.value_if_known_constant(start2);
                if let (Some(length1), Some(start1_value), Some(start2_value)) =
                    (length1, start1_value, start2_value)
                {
                    let (end1, overflow) = start1_value.overflowing_add(length1);
                    if !overflow && end1 <= start2_value {
                        return true;
                    }
                }
            }
            if let (Some(start2), Some(length2), Some(start1)) = (op2.start, op2.length, op1.start)
            {
                let length2 = self.length_value(length2);
                let start2_value = self.value_if_known_constant(start2);
                let start1_value = self.value_if_known_constant(start1);
                if let (Some(length2), Some(start2_value), Some(start1_value)) =
                    (length2, start2_value, start1_value)
                {
                    let (end2, overflow) = start2_value.overflowing_add(length2);
                    if !overflow && end2 <= start1_value {
                        return true;
                    }
                }
            }
            if let (Some(start1), Some(length1), Some(start2), Some(length2)) =
                (op1.start, op1.length, op2.start, op2.length)
            {
                let length1 = self.length_value(length1);
                let length2 = self.length_value(length2);
                if length1.is_some_and(|value| value <= U256::from(32u8))
                    && length2.is_some_and(|value| value <= U256::from(32u8))
                    && self.known_to_be_different_by_at_least_32(start1, start2)
                {
                    return true;
                }
            }
        }

        false
    }

    fn known_covered(&mut self, covered: Operation, covering: Operation) -> bool {
        if covered.location != covering.location {
            return false;
        }
        if covered.start == covering.start
            && covered.start.is_some()
            && covered.length == covering.length
            && covered.length.is_some()
        {
            return true;
        }
        if covered.location == Location::Memory {
            if covered
                .length
                .is_some_and(|length| self.length_value(length) == Some(U256::zero()))
            {
                return true;
            }
            let (
                Some(covered_start),
                Some(covering_start),
                Some(covered_length),
                Some(covering_length),
            ) = (
                covered.start,
                covering.start,
                covered.length,
                covering.length,
            )
            else {
                return false;
            };

            let covered_length = self.length_value(covered_length);
            let covering_length = self.length_value(covering_length);
            if covered_start == covering_start
                && covered_length
                    .zip(covering_length)
                    .is_some_and(|(covered, covering)| covered <= covering)
            {
                return true;
            }

            let covered_start_value = self.value_if_known_constant(covered_start);
            let covering_start_value = self.value_if_known_constant(covering_start);
            if let (
                Some(covered_start_value),
                Some(covering_start_value),
                Some(covered_length),
                Some(covering_length),
            ) = (
                covered_start_value,
                covering_start_value,
                covered_length,
                covering_length,
            ) {
                let (covering_end, covering_overflow) =
                    covering_start_value.overflowing_add(covering_length);
                let (covered_end, covered_overflow) =
                    covered_start_value.overflowing_add(covered_length);
                if covering_start_value <= covered_start_value
                    && !covering_overflow
                    && !covered_overflow
                    && covered_end <= covering_end
                {
                    return true;
                }
            }
        }
        false
    }

    fn mark_active_as_used(&mut self, only_location: Option<Location>) {
        if only_location.is_none() || only_location == Some(Location::Memory) {
            if let Some(stores) = self.active_stores.get(&Location::Memory) {
                self.used_stores.extend(stores.iter().copied());
            }
        }
        if only_location.is_none() || only_location == Some(Location::Storage) {
            if let Some(stores) = self.active_stores.get(&Location::Storage) {
                self.used_stores.extend(stores.iter().copied());
            }
        }
        self.clear_active(only_location);
    }

    fn clear_active(&mut self, only_location: Option<Location>) {
        if only_location.is_none() || only_location == Some(Location::Memory) {
            self.active_stores.insert(Location::Memory, BTreeSet::new());
        }
        if only_location.is_none() || only_location == Some(Location::Storage) {
            self.active_stores
                .insert(Location::Storage, BTreeSet::new());
        }
    }

    fn control_flow_for_call(&self, expression: &ffi::WireExpression) -> ControlFlowSideEffects {
        match expression.function_name_kind {
            FUNCTION_NAME_BUILTIN => self
                .request
                .builtins
                .iter()
                .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
                .map(|builtin| ControlFlowSideEffects {
                    can_continue: builtin.control_flow_can_continue,
                    can_terminate: builtin.control_flow_can_terminate,
                })
                .unwrap_or_default(),
            FUNCTION_NAME_IDENTIFIER => self
                .control_flow_side_effects
                .get(&expression.function_name_name_id)
                .copied()
                .unwrap_or_default(),
            _ => ControlFlowSideEffects::default(),
        }
    }

    fn evm_instruction(&self, expression: &ffi::WireExpression) -> Option<u16> {
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
        {
            return None;
        }
        self.request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
            .and_then(|builtin| builtin.has_evm_opcode.then_some(builtin.evm_opcode))
    }

    fn identifier_name_if_ssa(&self, expression_id: u64) -> Option<u64> {
        let expression = &self.request.expressions[expression_id as usize];
        (expression.kind == EXPRESSION_IDENTIFIER
            && self.ssa_values.contains_key(&expression.name_id))
        .then_some(expression.name_id)
    }

    fn length_value(&mut self, length: OperationLength) -> Option<U256> {
        match length {
            OperationLength::Name(name) => self.value_if_known_constant(name),
            OperationLength::Constant(value) => Some(value),
        }
    }

    fn known_to_be_zero(&mut self, variable: u64) -> bool {
        self.value_if_known_constant(variable) == Some(U256::zero())
    }

    fn known_to_be_different(&mut self, left: u64, right: u64) -> bool {
        self.difference_if_known_constant(left, right)
            .is_some_and(|difference| !difference.is_zero())
    }

    fn known_to_be_different_by_at_least_32(&mut self, left: u64, right: u64) -> bool {
        self.difference_if_known_constant(left, right)
            .is_some_and(|difference| {
                difference >= U256::from(32u8) && difference <= U256::max_value() - U256::from(31u8)
            })
    }

    fn difference_if_known_constant(&mut self, left: u64, right: u64) -> Option<U256> {
        let left_offset = self.explore_variable(left);
        let right_offset = self.explore_variable(right);
        (left_offset.reference == right_offset.reference)
            .then_some(left_offset.offset.overflowing_sub(right_offset.offset).0)
    }

    fn value_if_known_constant(&mut self, variable: u64) -> Option<U256> {
        let offset = self.explore_variable(variable);
        offset.reference.is_none().then_some(offset.offset)
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
                offset: U256::zero(),
            },
        )
    }

    fn explore_value(&mut self, value: ValueRef) -> Option<VariableOffset> {
        match value {
            ValueRef::Zero => Some(VariableOffset {
                reference: None,
                offset: U256::zero(),
            }),
            ValueRef::Expression(expression_id) => self.explore_expression(expression_id),
        }
    }

    fn explore_expression(&mut self, expression_id: u64) -> Option<VariableOffset> {
        let expression = self.request.expressions[expression_id as usize].clone();
        match expression.kind {
            EXPRESSION_LITERAL if !expression.literal_unlimited => Some(VariableOffset {
                reference: None,
                offset: U256::from_big_endian(&expression.literal_value),
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
                    let offset = left.offset.overflowing_add(right.offset).0;
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
                    let offset = left.offset.overflowing_sub(right.offset).0;
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
        let current_value = self.ssa_values.get(&variable).copied();
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
            .unwrap_or_else(U256::zero);

        let members = group.iter().copied().collect::<Vec<_>>();
        for member in members {
            if let Some(offset) = self.offsets.get_mut(&member) {
                offset.reference = Some(new_representative);
                offset.offset = offset.offset.overflowing_sub(new_offset).0;
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

    fn remove_pending_statements(&mut self) {
        for block in &mut self.request.blocks {
            block
                .statement_ids
                .retain(|statement_id| !self.stores_to_remove.contains(statement_id));
        }
    }
}

fn expression_arguments_identifier_or_literal(
    request: &ffi::WireYulOptimizerRequest,
    expression: &ffi::WireExpression,
) -> bool {
    expression
        .argument_expression_ids
        .iter()
        .all(|argument_id| {
            let argument = &request.expressions[*argument_id as usize];
            matches!(argument.kind, EXPRESSION_IDENTIFIER | EXPRESSION_LITERAL)
        })
}

fn merge_active_stores(target: &mut ActiveStores, other: ActiveStores) {
    for (location, stores) in other {
        target.entry(location).or_default().extend(stores);
    }
}

fn merge_active_store_vec(target: &mut ActiveStores, sources: Vec<ActiveStores>) {
    for source in sources {
        merge_active_stores(target, source);
    }
}

fn ssa_values(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, ValueRef>, OptimizerError> {
    let mut tracker = SsaValueTracker {
        request,
        values: BTreeMap::new(),
    };
    tracker.visit_block(request.root_block_id)?;
    Ok(tracker.values)
}

struct SsaValueTracker<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    values: BTreeMap<u64, ValueRef>,
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
                    self.values
                        .remove(&self.request.identifiers[*variable_id as usize].name_id);
                }
                Ok(())
            }
            STATEMENT_VARIABLE_DECLARATION => {
                if !statement.has_value {
                    for variable_id in &statement.variable_ids {
                        self.values.insert(
                            self.request.names[*variable_id as usize].name_id,
                            ValueRef::Zero,
                        );
                    }
                } else if statement.variable_ids.len() == 1 {
                    self.values.insert(
                        self.request.names[statement.variable_ids[0] as usize].name_id,
                        ValueRef::Expression(statement.value_expression_id),
                    );
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                for variable_id in &statement.return_variable_ids {
                    self.values.insert(
                        self.request.names[*variable_id as usize].name_id,
                        ValueRef::Zero,
                    );
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
                self.visit_block(statement.body_block_id)?;
                self.visit_block(statement.post_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
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

fn control_flow_side_effects(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, ControlFlowSideEffects>, OptimizerError> {
    let functions = root_functions(request);
    let mut output: BTreeMap<u64, ControlFlowSideEffects> = functions
        .keys()
        .map(|name| {
            (
                *name,
                ControlFlowSideEffects {
                    can_continue: false,
                    can_terminate: false,
                },
            )
        })
        .collect();

    loop {
        let mut changed = false;
        for (name, statement_id) in &functions {
            let body_block_id = request.statements[*statement_id as usize].body_block_id;
            let can_continue = block_can_continue(request, body_block_id, &output)?;
            let can_terminate = block_can_terminate(request, body_block_id, &output)?;
            let entry = output.entry(*name).or_default();
            if entry.can_continue != can_continue || entry.can_terminate != can_terminate {
                entry.can_continue = can_continue;
                entry.can_terminate = can_terminate;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    Ok(output)
}

fn block_can_continue(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    function_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    let mut can_continue = true;
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        if !can_continue {
            break;
        }
        can_continue = statement_can_continue(request, *statement_id, function_effects)?;
    }
    Ok(can_continue)
}

fn statement_can_continue(
    request: &ffi::WireYulOptimizerRequest,
    statement_id: u64,
    function_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    let statement = &request.statements[statement_id as usize];
    match statement.kind {
        STATEMENT_EXPRESSION => {
            expression_can_continue(request, statement.expression_id, function_effects)
        }
        STATEMENT_ASSIGNMENT => {
            expression_can_continue(request, statement.value_expression_id, function_effects)
        }
        STATEMENT_VARIABLE_DECLARATION => {
            if statement.has_value {
                expression_can_continue(request, statement.value_expression_id, function_effects)
            } else {
                Ok(true)
            }
        }
        STATEMENT_FUNCTION_DEFINITION => Ok(true),
        STATEMENT_IF => {
            expression_can_continue(request, statement.condition_expression_id, function_effects)
        }
        STATEMENT_SWITCH => {
            if !expression_can_continue(request, statement.switch_expression_id, function_effects)?
            {
                return Ok(false);
            }
            let has_default = statement
                .case_ids
                .iter()
                .any(|case_id| !request.cases[*case_id as usize].has_value);
            if !has_default {
                return Ok(true);
            }
            for case_id in &statement.case_ids {
                if block_can_continue(
                    request,
                    request.cases[*case_id as usize].body_block_id,
                    function_effects,
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        STATEMENT_FOR_LOOP => {
            block_can_continue(request, statement.pre_block_id, function_effects)?;
            expression_can_continue(request, statement.condition_expression_id, function_effects)
        }
        STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => Ok(false),
        STATEMENT_BLOCK => block_can_continue(request, statement.block_id, function_effects),
        _ => Ok(true),
    }
}

fn expression_can_continue(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    function_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    let expression = &request.expressions[expression_id as usize];
    if expression.kind != EXPRESSION_FUNCTION_CALL {
        return Ok(true);
    }

    for argument_id in expression.argument_expression_ids.iter().rev() {
        if !expression_can_continue(request, *argument_id, function_effects)? {
            return Ok(false);
        }
    }

    match expression.function_name_kind {
        FUNCTION_NAME_BUILTIN => Ok(request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
            .map(|builtin| builtin.control_flow_can_continue)
            .unwrap_or(true)),
        FUNCTION_NAME_IDENTIFIER => Ok(function_effects
            .get(&expression.function_name_name_id)
            .copied()
            .unwrap_or_default()
            .can_continue),
        _ => Err(OptimizerError::InvalidWire(format!(
            "invalid function name kind: {}",
            expression.function_name_kind
        ))),
    }
}

fn block_can_terminate(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    function_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        if statement_can_terminate(request, *statement_id, function_effects)? {
            return Ok(true);
        }
        if !statement_can_continue(request, *statement_id, function_effects)? {
            return Ok(false);
        }
    }
    Ok(false)
}

fn statement_can_terminate(
    request: &ffi::WireYulOptimizerRequest,
    statement_id: u64,
    function_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    let statement = &request.statements[statement_id as usize];
    match statement.kind {
        STATEMENT_EXPRESSION => {
            expression_can_terminate(request, statement.expression_id, function_effects)
        }
        STATEMENT_ASSIGNMENT => {
            expression_can_terminate(request, statement.value_expression_id, function_effects)
        }
        STATEMENT_VARIABLE_DECLARATION => {
            if statement.has_value {
                expression_can_terminate(request, statement.value_expression_id, function_effects)
            } else {
                Ok(false)
            }
        }
        STATEMENT_IF => {
            expression_can_terminate(request, statement.condition_expression_id, function_effects)
                .map(|value| value)
                .and_then(|condition_terminates| {
                    if condition_terminates {
                        Ok(true)
                    } else {
                        block_can_terminate(request, statement.body_block_id, function_effects)
                    }
                })
        }
        STATEMENT_SWITCH => {
            if expression_can_terminate(request, statement.switch_expression_id, function_effects)?
            {
                return Ok(true);
            }
            for case_id in &statement.case_ids {
                if block_can_terminate(
                    request,
                    request.cases[*case_id as usize].body_block_id,
                    function_effects,
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        STATEMENT_FOR_LOOP => {
            if expression_can_terminate(
                request,
                statement.condition_expression_id,
                function_effects,
            )? {
                return Ok(true);
            }
            block_can_terminate(request, statement.body_block_id, function_effects)
                .or_else(|_| Ok(false))
        }
        STATEMENT_BLOCK => block_can_terminate(request, statement.block_id, function_effects),
        _ => Ok(false),
    }
}

fn expression_can_terminate(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    function_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    let expression = &request.expressions[expression_id as usize];
    if expression.kind != EXPRESSION_FUNCTION_CALL {
        return Ok(false);
    }

    for argument_id in expression.argument_expression_ids.iter().rev() {
        if expression_can_terminate(request, *argument_id, function_effects)? {
            return Ok(true);
        }
        if !expression_can_continue(request, *argument_id, function_effects)? {
            return Ok(false);
        }
    }

    match expression.function_name_kind {
        FUNCTION_NAME_BUILTIN => Ok(request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
            .map(|builtin| builtin.control_flow_can_terminate)
            .unwrap_or(false)),
        FUNCTION_NAME_IDENTIFIER => Ok(function_effects
            .get(&expression.function_name_name_id)
            .copied()
            .unwrap_or_default()
            .can_terminate),
        _ => Err(OptimizerError::InvalidWire(format!(
            "invalid function name kind: {}",
            expression.function_name_kind
        ))),
    }
}

fn root_functions(request: &ffi::WireYulOptimizerRequest) -> BTreeMap<u64, u64> {
    let mut functions = BTreeMap::new();
    for statement_id in &request.blocks[request.root_block_id as usize].statement_ids {
        let statement = &request.statements[*statement_id as usize];
        if statement.kind == STATEMENT_FUNCTION_DEFINITION {
            functions.insert(statement.name_id, *statement_id);
        }
    }
    functions
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
