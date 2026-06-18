use crate::bridge::ffi;
use crate::passes::function_grouper;
use crate::wire::{
    OptimizerError, EFFECT_NONE, EFFECT_READ, EFFECT_WRITE, EXPRESSION_FUNCTION_CALL,
    EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let root = request.root_block_id;
    let call_graph = call_graph(request, root)?;
    let function_side_effects = side_effects(request, &call_graph);
    let allow_msize_optimization = !contains_msize(request)?;

    loop {
        let references = count_references(request)?;
        let mut pruner = UnusedPruner {
            request,
            references,
            function_side_effects: &function_side_effects,
            allow_msize_optimization,
            should_run_again: false,
        };
        pruner.visit_block(root)?;
        if !pruner.should_run_again {
            break;
        }
    }

    function_grouper::group_functions_in_block(request, root);
    Ok(())
}

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

    fn can_be_removed(self, allow_msize_optimization: bool) -> bool {
        if allow_msize_optimization {
            self.can_be_removed_if_no_msize
        } else {
            self.can_be_removed
        }
    }
}

struct UnusedPruner<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    references: BTreeMap<FunctionHandle, usize>,
    function_side_effects: &'b BTreeMap<FunctionHandle, SideEffects>,
    allow_msize_optimization: bool,
    should_run_again: bool,
}

impl UnusedPruner<'_, '_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut retained = Vec::with_capacity(statement_ids.len());

        for statement_id in statement_ids {
            match self.prune_or_rewrite_statement(statement_id)? {
                StatementAction::Remove => {}
                StatementAction::Keep => retained.push(statement_id),
                StatementAction::Replace(replacement_id) => retained.push(replacement_id),
            }
        }

        self.request.blocks[block_id as usize].statement_ids = retained.clone();

        for statement_id in retained {
            self.visit_statement(statement_id)?;
        }

        Ok(())
    }

    fn prune_or_rewrite_statement(
        &mut self,
        statement_id: u64,
    ) -> Result<StatementAction, OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => {
                if self.used(statement.name_id) {
                    return Ok(StatementAction::Keep);
                }

                let removed_references =
                    references_in_block(self.request, statement.body_block_id)?;
                self.subtract_references(&removed_references);
                Ok(StatementAction::Remove)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                let all_unused = statement
                    .variable_ids
                    .iter()
                    .all(|name_id| !self.used(self.request.names[*name_id as usize].name_id));
                if !all_unused {
                    return Ok(StatementAction::Keep);
                }

                if !statement.has_value {
                    return Ok(StatementAction::Remove);
                }

                if self.expression_can_be_removed(statement.value_expression_id)? {
                    let removed_references =
                        references_in_expression(self.request, statement.value_expression_id)?;
                    self.subtract_references(&removed_references);
                    return Ok(StatementAction::Remove);
                }

                if statement.variable_ids.len() == 1 {
                    if let Some(discard_handle) = self.discard_handle() {
                        let replacement = self.new_discard_statement(
                            statement.debug_data_id,
                            discard_handle,
                            statement.value_expression_id,
                        );
                        return Ok(StatementAction::Replace(replacement));
                    }
                }

                Ok(StatementAction::Keep)
            }
            STATEMENT_EXPRESSION => {
                if self.expression_can_be_removed(statement.expression_id)? {
                    let removed_references =
                        references_in_expression(self.request, statement.expression_id)?;
                    self.subtract_references(&removed_references);
                    Ok(StatementAction::Remove)
                } else {
                    Ok(StatementAction::Keep)
                }
            }
            _ => Ok(StatementAction::Keep),
        }
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id),
            STATEMENT_IF => self.visit_block(statement.body_block_id),
            STATEMENT_SWITCH => {
                for case_id in statement.case_ids {
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
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

    fn used(&self, name_id: u64) -> bool {
        self.references
            .get(&FunctionHandle::Name(name_id))
            .copied()
            .unwrap_or(0)
            > 0
    }

    fn subtract_references(&mut self, subtrahend: &BTreeMap<FunctionHandle, usize>) {
        if subtrahend.is_empty() {
            return;
        }

        for (handle, count) in subtrahend {
            let value = self.references.entry(*handle).or_default();
            *value = value.saturating_sub(*count);
        }
        self.should_run_again = true;
    }

    fn expression_can_be_removed(&self, expression_id: u64) -> Result<bool, OptimizerError> {
        Ok(self
            .expression_side_effects(expression_id)?
            .can_be_removed(self.allow_msize_optimization))
    }

    fn expression_side_effects(&self, expression_id: u64) -> Result<SideEffects, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_IDENTIFIER | EXPRESSION_LITERAL => Ok(SideEffects::default()),
            EXPRESSION_FUNCTION_CALL => {
                let mut effects = SideEffects::default();
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    effects.add_assign(self.expression_side_effects(*argument_id)?);
                }

                let callee_effects = match expression.function_name_kind {
                    FUNCTION_NAME_BUILTIN => self
                        .request
                        .builtins
                        .iter()
                        .find(|builtin| {
                            builtin.handle_id == expression.function_name_builtin_handle
                        })
                        .map(builtin_side_effect)
                        .unwrap_or_else(SideEffects::worst),
                    FUNCTION_NAME_IDENTIFIER => self
                        .function_side_effects
                        .get(&FunctionHandle::Name(expression.function_name_name_id))
                        .copied()
                        .unwrap_or_else(SideEffects::worst),
                    _ => {
                        return Err(OptimizerError::InvalidWire(format!(
                            "invalid function name kind: {}",
                            expression.function_name_kind
                        )));
                    }
                };
                effects.add_assign(callee_effects);
                Ok(effects)
            }
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }

    fn discard_handle(&self) -> Option<u64> {
        self.request
            .special_handles
            .has_discard
            .then_some(self.request.special_handles.discard)
    }

    fn new_discard_statement(
        &mut self,
        debug_data_id: u64,
        discard_handle: u64,
        argument_expression_id: u64,
    ) -> u64 {
        let expression_id = self.request.expressions.len() as u64;
        self.request.expressions.push(ffi::WireExpression {
            kind: EXPRESSION_FUNCTION_CALL,
            debug_data_id,
            literal_kind: 0,
            literal_unlimited: false,
            literal_value: Vec::new(),
            literal_string_id: 0,
            has_literal_hint: false,
            literal_hint_id: 0,
            name_id: 0,
            function_name_kind: FUNCTION_NAME_BUILTIN,
            function_name_debug_data_id: debug_data_id,
            function_name_name_id: 0,
            function_name_builtin_handle: discard_handle,
            argument_expression_ids: vec![argument_expression_id],
        });

        let statement_id = self.request.statements.len() as u64;
        let mut statement = empty_statement(STATEMENT_EXPRESSION, debug_data_id);
        statement.expression_id = expression_id;
        self.request.statements.push(statement);
        statement_id
    }
}

enum StatementAction {
    Keep,
    Remove,
    Replace(u64),
}

fn count_references(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<FunctionHandle, usize>, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: BTreeMap::new(),
    };
    counter.visit_block(request.root_block_id)?;
    for reserved in &request.reserved_identifier_ids {
        *counter
            .references
            .entry(FunctionHandle::Name(*reserved))
            .or_default() += 1;
    }
    Ok(counter.references)
}

fn references_in_block(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<BTreeMap<FunctionHandle, usize>, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: BTreeMap::new(),
    };
    counter.visit_block(block_id)?;
    Ok(counter.references)
}

fn references_in_expression(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
) -> Result<BTreeMap<FunctionHandle, usize>, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: BTreeMap::new(),
    };
    counter.visit_expression(expression_id)?;
    Ok(counter.references)
}

struct ReferenceCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    references: BTreeMap<FunctionHandle, usize>,
}

impl ReferenceCounter<'_> {
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
                    *self
                        .references
                        .entry(FunctionHandle::Name(name_id))
                        .or_default() += 1;
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
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
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
            EXPRESSION_FUNCTION_CALL => {
                *self
                    .references
                    .entry(function_handle(&expression)?)
                    .or_default() += 1;
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }
            }
            EXPRESSION_IDENTIFIER => {
                *self
                    .references
                    .entry(FunctionHandle::Name(expression.name_id))
                    .or_default() += 1;
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
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
            if self.found {
                break;
            }
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
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
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

        if expression.function_name_kind == FUNCTION_NAME_BUILTIN
            && self.request.builtins.iter().any(|builtin| {
                builtin.handle_id == expression.function_name_builtin_handle && builtin.is_msize
            })
        {
            self.found = true;
            return Ok(());
        }

        for argument_id in expression.argument_expression_ids.iter().rev() {
            self.visit_expression(*argument_id)?;
            if self.found {
                break;
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
                for case_id in statement.case_ids {
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
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
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            let callee = function_handle(&expression)?;
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
        .map(|builtin| (builtin.handle_id, builtin_side_effect(builtin)))
        .collect()
}

fn builtin_side_effect(builtin: &ffi::WireBuiltinFunction) -> SideEffects {
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
    }
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

fn empty_statement(kind: u8, debug_data_id: u64) -> ffi::WireStatement {
    ffi::WireStatement {
        kind,
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
        block_id: 0,
    }
}
