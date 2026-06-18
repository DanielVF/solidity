use super::conditional_simplifier::{
    function_side_effects as control_flow_side_effects, ControlFlowSideEffects,
};
use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EFFECT_NONE, EFFECT_READ, EFFECT_WRITE, EXPRESSION_FUNCTION_CALL,
    EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

type ActiveStores = BTreeMap<u64, BTreeSet<u64>>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FunctionHandle {
    Name(u64),
    Builtin(u64),
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

#[derive(Clone, Debug, Default)]
struct ForLoopInfo {
    pending_break_stmts: Vec<ActiveStores>,
    pending_continue_stmts: Vec<ActiveStores>,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let builtin_side_effects = builtin_side_effects(request);
    let control_flow_side_effects = control_flow_side_effects(request)?;

    let mut eliminator = UnusedAssignEliminator {
        request,
        builtin_side_effects,
        control_flow_side_effects,
        return_variables: BTreeSet::new(),
        all_stores: BTreeSet::new(),
        used_stores: BTreeSet::new(),
        stores_to_remove: BTreeSet::new(),
        active_stores: ActiveStores::new(),
        for_loop_info: ForLoopInfo::default(),
        for_loop_nesting_depth: 0,
    };
    eliminator.visit_block(eliminator.request.root_block_id)?;
    eliminator.stores_to_remove.extend(
        eliminator
            .all_stores
            .difference(&eliminator.used_stores)
            .copied(),
    );
    eliminator.remove_pending_statements();
    Ok(())
}

struct UnusedAssignEliminator<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    builtin_side_effects: BTreeMap<u64, SideEffects>,
    control_flow_side_effects: BTreeMap<u64, ControlFlowSideEffects>,
    return_variables: BTreeSet<u64>,
    all_stores: BTreeSet<u64>,
    used_stores: BTreeSet<u64>,
    stores_to_remove: BTreeSet<u64>,
    active_stores: ActiveStores,
    for_loop_info: ForLoopInfo,
    for_loop_nesting_depth: usize,
}

impl UnusedAssignEliminator<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in &statement_ids {
            self.visit_statement(*statement_id)?;
        }

        for statement_id in statement_ids {
            let statement = &self.request.statements[statement_id as usize];
            if statement.kind == STATEMENT_VARIABLE_DECLARATION {
                for variable_id in &statement.variable_ids {
                    let name = self.request.names[*variable_id as usize].name_id;
                    self.active_stores.remove(&name);
                }
            }
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

        if statement.kind == STATEMENT_ASSIGNMENT {
            if self.expression_movable(statement.value_expression_id)? {
                self.all_stores.insert(statement_id);
                for variable_id in &statement.variable_ids {
                    let name = self.request.identifiers[*variable_id as usize].name_id;
                    self.active_stores
                        .insert(name, BTreeSet::from([statement_id]));
                }
            } else {
                for variable_id in &statement.variable_ids {
                    let name = self.request.identifiers[*variable_id as usize].name_id;
                    self.active_stores.entry(name).or_default().clear();
                }
            }
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
        let saved_return_variables = std::mem::take(&mut self.return_variables);
        for return_variable_id in &statement.return_variable_ids {
            self.return_variables
                .insert(self.request.names[*return_variable_id as usize].name_id);
        }

        let saved_all_stores = std::mem::take(&mut self.all_stores);
        let saved_used_stores = std::mem::take(&mut self.used_stores);
        let saved_active_stores = std::mem::take(&mut self.active_stores);
        let saved_for_loop_info = std::mem::take(&mut self.for_loop_info);
        let saved_for_loop_nesting_depth = self.for_loop_nesting_depth;
        self.for_loop_nesting_depth = 0;

        self.visit_block(statement.body_block_id)?;
        for return_variable_id in &statement.return_variable_ids {
            let name = self.request.names[*return_variable_id as usize].name_id;
            self.mark_used(name);
        }
        self.stores_to_remove
            .extend(self.all_stores.difference(&self.used_stores).copied());

        self.all_stores = saved_all_stores;
        self.used_stores = saved_used_stores;
        self.active_stores = saved_active_stores;
        self.for_loop_info = saved_for_loop_info;
        self.for_loop_nesting_depth = saved_for_loop_nesting_depth;
        self.return_variables = saved_return_variables;
        Ok(())
    }

    fn visit_for_loop(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "UnusedAssignEliminator requires ForLoopInitRewriter.".to_string(),
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
            self.shortcut_nested_loop(&zero_runs);
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
        let return_variables = self.return_variables.iter().copied().collect::<Vec<_>>();
        for name in return_variables {
            self.mark_used(name);
        }
        self.active_stores.clear();
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }
                if !self.function_call_can_continue(&expression) {
                    self.active_stores.clear();
                }
            }
            EXPRESSION_IDENTIFIER => self.mark_used(expression.name_id),
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

    fn shortcut_nested_loop(&mut self, zero_runs: &ActiveStores) {
        for (variable, stores) in &self.active_stores {
            let zero_stores = zero_runs.get(variable);
            for assignment in stores {
                if zero_stores.is_some_and(|items| items.contains(assignment)) {
                    continue;
                }
                self.used_stores.insert(*assignment);
            }
        }
    }

    fn mark_used(&mut self, variable: u64) {
        if let Some(assignments) = self.active_stores.remove(&variable) {
            self.used_stores.extend(assignments);
        }
    }

    fn function_call_can_continue(&self, expression: &ffi::WireExpression) -> bool {
        match expression.function_name_kind {
            FUNCTION_NAME_BUILTIN => self
                .request
                .builtins
                .iter()
                .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
                .map(|builtin| builtin.control_flow_can_continue)
                .unwrap_or(true),
            FUNCTION_NAME_IDENTIFIER => self
                .control_flow_side_effects
                .get(&expression.function_name_name_id)
                .map(|side_effects| side_effects.can_continue)
                .unwrap_or(true),
            _ => true,
        }
    }

    fn expression_movable(&self, expression_id: u64) -> Result<bool, OptimizerError> {
        Ok(self.side_effects_expression(expression_id)?.movable)
    }

    fn side_effects_expression(&self, expression_id: u64) -> Result<SideEffects, OptimizerError> {
        let mut output = SideEffects::default();
        self.collect_expression_side_effects(expression_id, &mut output)?;
        Ok(output)
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
            FunctionHandle::Name(_) => SideEffects::worst(),
        };
        output.add_assign(side_effects);
        Ok(())
    }

    fn remove_pending_statements(&mut self) {
        for block in &mut self.request.blocks {
            block
                .statement_ids
                .retain(|statement_id| !self.stores_to_remove.contains(statement_id));
        }
    }
}

fn merge_active_stores(target: &mut ActiveStores, other: ActiveStores) {
    for (variable, stores) in other {
        target.entry(variable).or_default().extend(stores);
    }
}

fn merge_active_store_vec(target: &mut ActiveStores, sources: Vec<ActiveStores>) {
    for source in sources {
        merge_active_stores(target, source);
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
