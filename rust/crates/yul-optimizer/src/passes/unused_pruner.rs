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
    let builtin_side_effects = builtin_side_effects(request);
    let msize_builtin_handles = msize_builtin_handles(request);
    let call_graph = call_graph(request, root)?;
    let function_side_effects = side_effects(&call_graph, &builtin_side_effects);
    let allow_msize_optimization = !contains_msize(request, &msize_builtin_handles)?;

    loop {
        let references = count_references(request)?;
        let expression_side_effects_cache = vec![None; request.expressions.len()];
        let mut pruner = UnusedPruner {
            request,
            references,
            function_side_effects: &function_side_effects,
            builtin_side_effects: &builtin_side_effects,
            expression_side_effects_cache,
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
    references: ReferenceCounts,
    function_side_effects: &'b BTreeMap<FunctionHandle, SideEffects>,
    builtin_side_effects: &'b BTreeMap<u64, SideEffects>,
    expression_side_effects_cache: Vec<Option<SideEffects>>,
    allow_msize_optimization: bool,
    should_run_again: bool,
}

impl UnusedPruner<'_, '_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let block_index = block_id as usize;
        let statement_ids = std::mem::take(&mut self.request.blocks[block_index].statement_ids);
        let mut retained = Vec::with_capacity(statement_ids.len());

        for statement_id in statement_ids {
            match self.prune_or_rewrite_statement(statement_id)? {
                StatementAction::Remove => {}
                StatementAction::Keep => retained.push(statement_id),
                StatementAction::Replace(replacement_id) => retained.push(replacement_id),
            }
        }

        self.request.blocks[block_index].statement_ids = retained;

        let retained_len = self.request.blocks[block_index].statement_ids.len();
        for index in 0..retained_len {
            let statement_id = self.request.blocks[block_index].statement_ids[index];
            self.visit_statement(statement_id)?;
        }

        Ok(())
    }

    fn prune_or_rewrite_statement(
        &mut self,
        statement_id: u64,
    ) -> Result<StatementAction, OptimizerError> {
        let statement_index = statement_id as usize;
        match self.request.statements[statement_index].kind {
            STATEMENT_FUNCTION_DEFINITION => {
                let name_id = self.request.statements[statement_index].name_id;
                if self.used(name_id) {
                    return Ok(StatementAction::Keep);
                }

                let body_block_id = self.request.statements[statement_index].body_block_id;
                let removed_references = references_in_block(self.request, body_block_id)?;
                self.subtract_references(&removed_references);
                Ok(StatementAction::Remove)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                let variable_ids_len = self.request.statements[statement_index].variable_ids.len();
                let mut all_unused = true;
                for index in 0..variable_ids_len {
                    let variable_id =
                        self.request.statements[statement_index].variable_ids[index] as usize;
                    let name_id = self.request.names[variable_id].name_id;
                    if self.used(name_id) {
                        all_unused = false;
                        break;
                    }
                }
                if !all_unused {
                    return Ok(StatementAction::Keep);
                }

                if !self.request.statements[statement_index].has_value {
                    return Ok(StatementAction::Remove);
                }

                let value_expression_id =
                    self.request.statements[statement_index].value_expression_id;
                if self.expression_can_be_removed(value_expression_id)? {
                    let removed_references =
                        references_in_expression(self.request, value_expression_id)?;
                    self.subtract_references(&removed_references);
                    return Ok(StatementAction::Remove);
                }

                if variable_ids_len == 1 {
                    if let Some(discard_handle) = self.discard_handle() {
                        let replacement = self.new_discard_statement(
                            self.request.statements[statement_index].debug_data_id,
                            discard_handle,
                            value_expression_id,
                        );
                        return Ok(StatementAction::Replace(replacement));
                    }
                }

                Ok(StatementAction::Keep)
            }
            STATEMENT_EXPRESSION => {
                let expression_id = self.request.statements[statement_index].expression_id;
                if self.expression_can_be_removed(expression_id)? {
                    let removed_references = references_in_expression(self.request, expression_id)?;
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
        let statement_index = statement_id as usize;
        match self.request.statements[statement_index].kind {
            STATEMENT_FUNCTION_DEFINITION | STATEMENT_IF => {
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_SWITCH => {
                let case_ids_len = self.request.statements[statement_index].case_ids.len();
                for index in 0..case_ids_len {
                    let case_id = self.request.statements[statement_index].case_ids[index];
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(self.request.statements[statement_index].pre_block_id)?;
                self.visit_block(self.request.statements[statement_index].post_block_id)?;
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(self.request.statements[statement_index].block_id),
            _ => Ok(()),
        }
    }

    fn used(&self, name_id: u64) -> bool {
        self.references.name_count(name_id) > 0
    }

    fn subtract_references(&mut self, subtrahend: &ReferenceCounts) {
        if subtrahend.is_empty() {
            return;
        }

        for (name_id, count) in subtrahend.name_counts() {
            self.references.subtract_name(name_id, count);
        }
        self.should_run_again = true;
    }

    fn expression_can_be_removed(&mut self, expression_id: u64) -> Result<bool, OptimizerError> {
        Ok(self
            .expression_side_effects(expression_id)?
            .can_be_removed(self.allow_msize_optimization))
    }

    fn expression_side_effects(
        &mut self,
        expression_id: u64,
    ) -> Result<SideEffects, OptimizerError> {
        let expression_index = expression_id as usize;
        if let Some(cached) = self.expression_side_effects_cache[expression_index] {
            return Ok(cached);
        }

        let effects = match self.request.expressions[expression_index].kind {
            EXPRESSION_IDENTIFIER | EXPRESSION_LITERAL => Ok(SideEffects::default()),
            EXPRESSION_FUNCTION_CALL => {
                let mut effects = SideEffects::default();
                let argument_ids_len = self.request.expressions[expression_index]
                    .argument_expression_ids
                    .len();
                for index in (0..argument_ids_len).rev() {
                    let argument_id =
                        self.request.expressions[expression_index].argument_expression_ids[index];
                    effects.add_assign(self.expression_side_effects(argument_id)?);
                }

                let callee_effects =
                    match self.request.expressions[expression_index].function_name_kind {
                        FUNCTION_NAME_BUILTIN => self
                            .builtin_side_effects
                            .get(
                                &self.request.expressions[expression_index]
                                    .function_name_builtin_handle,
                            )
                            .copied()
                            .unwrap_or_else(SideEffects::worst),
                        FUNCTION_NAME_IDENTIFIER => self
                            .function_side_effects
                            .get(&FunctionHandle::Name(
                                self.request.expressions[expression_index].function_name_name_id,
                            ))
                            .copied()
                            .unwrap_or_else(SideEffects::worst),
                        _ => {
                            return Err(OptimizerError::InvalidWire(format!(
                                "invalid function name kind: {}",
                                self.request.expressions[expression_index].function_name_kind
                            )));
                        }
                    };
                effects.add_assign(callee_effects);
                Ok(effects)
            }
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                self.request.expressions[expression_index].kind
            ))),
        }?;

        self.expression_side_effects_cache[expression_index] = Some(effects);
        Ok(effects)
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
) -> Result<ReferenceCounts, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: ReferenceCounts::with_name_count(request.strings.len()),
    };
    counter.visit_block(request.root_block_id)?;
    for reserved in &request.reserved_identifier_ids {
        counter.references.increment_name(*reserved);
    }
    Ok(counter.references)
}

fn references_in_block(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<ReferenceCounts, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: ReferenceCounts::with_name_count(request.strings.len()),
    };
    counter.visit_block(block_id)?;
    Ok(counter.references)
}

fn references_in_expression(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
) -> Result<ReferenceCounts, OptimizerError> {
    let mut counter = ReferenceCounter {
        request,
        references: ReferenceCounts::with_name_count(request.strings.len()),
    };
    counter.visit_expression(expression_id)?;
    Ok(counter.references)
}

#[derive(Clone, Debug, Default)]
struct ReferenceCounts {
    names: Vec<usize>,
    nonzero_name_ids: Vec<u64>,
}

impl ReferenceCounts {
    fn with_name_count(name_count: usize) -> Self {
        Self {
            names: vec![0; name_count],
            nonzero_name_ids: Vec::new(),
        }
    }

    fn increment_name(&mut self, name_id: u64) {
        let index = name_id as usize;
        if index >= self.names.len() {
            self.names.resize(index + 1, 0);
        }
        if self.names[index] == 0 {
            self.nonzero_name_ids.push(name_id);
        }
        self.names[index] += 1;
    }

    fn subtract_name(&mut self, name_id: u64, count: usize) {
        let Some(value) = self.names.get_mut(name_id as usize) else {
            return;
        };
        *value = value.saturating_sub(count);
    }

    fn name_count(&self, name_id: u64) -> usize {
        self.names.get(name_id as usize).copied().unwrap_or(0)
    }

    fn is_empty(&self) -> bool {
        self.nonzero_name_ids.is_empty()
    }

    fn name_counts(&self) -> impl Iterator<Item = (u64, usize)> + '_ {
        self.nonzero_name_ids.iter().filter_map(|name_id| {
            let count = self.name_count(*name_id);
            (count > 0).then_some((*name_id, count))
        })
    }
}

struct ReferenceCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    references: ReferenceCounts,
}

impl ReferenceCounter<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let block_index = block_id as usize;
        let statement_ids_len = self.request.blocks[block_index].statement_ids.len();
        for index in 0..statement_ids_len {
            let statement_id = self.request.blocks[block_index].statement_ids[index];
            self.visit_statement(statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement_index = statement_id as usize;
        match self.request.statements[statement_index].kind {
            STATEMENT_EXPRESSION => {
                self.visit_expression(self.request.statements[statement_index].expression_id)
            }
            STATEMENT_ASSIGNMENT => {
                let variable_ids_len = self.request.statements[statement_index].variable_ids.len();
                for index in 0..variable_ids_len {
                    let variable_id = self.request.statements[statement_index].variable_ids[index];
                    let name_id = self.request.identifiers[variable_id as usize].name_id;
                    self.references.increment_name(name_id);
                }
                self.visit_expression(self.request.statements[statement_index].value_expression_id)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                if self.request.statements[statement_index].has_value {
                    self.visit_expression(
                        self.request.statements[statement_index].value_expression_id,
                    )?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_IF => {
                self.visit_expression(
                    self.request.statements[statement_index].condition_expression_id,
                )?;
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(
                    self.request.statements[statement_index].switch_expression_id,
                )?;
                let case_ids_len = self.request.statements[statement_index].case_ids.len();
                for index in 0..case_ids_len {
                    let case_id = self.request.statements[statement_index].case_ids[index];
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(self.request.statements[statement_index].pre_block_id)?;
                self.visit_expression(
                    self.request.statements[statement_index].condition_expression_id,
                )?;
                self.visit_block(self.request.statements[statement_index].body_block_id)?;
                self.visit_block(self.request.statements[statement_index].post_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(self.request.statements[statement_index].block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression_index = expression_id as usize;
        match self.request.expressions[expression_index].kind {
            EXPRESSION_FUNCTION_CALL => {
                match self.request.expressions[expression_index].function_name_kind {
                    FUNCTION_NAME_IDENTIFIER => self.references.increment_name(
                        self.request.expressions[expression_index].function_name_name_id,
                    ),
                    FUNCTION_NAME_BUILTIN => {}
                    _ => {
                        return Err(OptimizerError::InvalidWire(format!(
                            "invalid function name kind: {}",
                            self.request.expressions[expression_index].function_name_kind
                        )));
                    }
                }
                let argument_ids_len = self.request.expressions[expression_index]
                    .argument_expression_ids
                    .len();
                for index in (0..argument_ids_len).rev() {
                    let argument_id =
                        self.request.expressions[expression_index].argument_expression_ids[index];
                    self.visit_expression(argument_id)?;
                }
            }
            EXPRESSION_IDENTIFIER => {
                self.references
                    .increment_name(self.request.expressions[expression_index].name_id);
            }
            EXPRESSION_LITERAL => {}
            _ => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid expression kind: {}",
                    self.request.expressions[expression_index].kind
                )));
            }
        }
        Ok(())
    }
}

fn contains_msize(
    request: &ffi::WireYulOptimizerRequest,
    msize_builtin_handles: &BTreeSet<u64>,
) -> Result<bool, OptimizerError> {
    let mut finder = MSizeFinder {
        request,
        msize_builtin_handles,
        found: false,
    };
    finder.visit_block(request.root_block_id)?;
    Ok(finder.found)
}

struct MSizeFinder<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    msize_builtin_handles: &'a BTreeSet<u64>,
    found: bool,
}

impl MSizeFinder<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let block_index = block_id as usize;
        let statement_ids_len = self.request.blocks[block_index].statement_ids.len();
        for index in 0..statement_ids_len {
            let statement_id = self.request.blocks[block_index].statement_ids[index];
            self.visit_statement(statement_id)?;
            if self.found {
                break;
            }
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement_index = statement_id as usize;
        match self.request.statements[statement_index].kind {
            STATEMENT_EXPRESSION => {
                self.visit_expression(self.request.statements[statement_index].expression_id)
            }
            STATEMENT_ASSIGNMENT => {
                self.visit_expression(self.request.statements[statement_index].value_expression_id)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                if self.request.statements[statement_index].has_value {
                    self.visit_expression(
                        self.request.statements[statement_index].value_expression_id,
                    )?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_IF => {
                self.visit_expression(
                    self.request.statements[statement_index].condition_expression_id,
                )?;
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(
                    self.request.statements[statement_index].switch_expression_id,
                )?;
                let case_ids_len = self.request.statements[statement_index].case_ids.len();
                for index in 0..case_ids_len {
                    let case_id = self.request.statements[statement_index].case_ids[index];
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(self.request.statements[statement_index].pre_block_id)?;
                self.visit_expression(
                    self.request.statements[statement_index].condition_expression_id,
                )?;
                self.visit_block(self.request.statements[statement_index].post_block_id)?;
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(self.request.statements[statement_index].block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression_index = expression_id as usize;
        if self.request.expressions[expression_index].kind != EXPRESSION_FUNCTION_CALL {
            return Ok(());
        }

        if self.request.expressions[expression_index].function_name_kind == FUNCTION_NAME_BUILTIN
            && self
                .msize_builtin_handles
                .contains(&self.request.expressions[expression_index].function_name_builtin_handle)
        {
            self.found = true;
            return Ok(());
        }

        let argument_ids_len = self.request.expressions[expression_index]
            .argument_expression_ids
            .len();
        for index in (0..argument_ids_len).rev() {
            let argument_id =
                self.request.expressions[expression_index].argument_expression_ids[index];
            self.visit_expression(argument_id)?;
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
        let block_index = block_id as usize;
        let statement_ids_len = self.request.blocks[block_index].statement_ids.len();
        for index in 0..statement_ids_len {
            let statement_id = self.request.blocks[block_index].statement_ids[index];
            self.visit_statement(statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement_index = statement_id as usize;
        match self.request.statements[statement_index].kind {
            STATEMENT_EXPRESSION => {
                self.visit_expression(self.request.statements[statement_index].expression_id)
            }
            STATEMENT_ASSIGNMENT => {
                self.visit_expression(self.request.statements[statement_index].value_expression_id)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                if self.request.statements[statement_index].has_value {
                    self.visit_expression(
                        self.request.statements[statement_index].value_expression_id,
                    )?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                let previous_function = self.current_function;
                self.current_function =
                    FunctionHandle::Name(self.request.statements[statement_index].name_id);
                self.graph
                    .function_calls
                    .insert(self.current_function, Vec::new());
                self.visit_block(self.request.statements[statement_index].body_block_id)?;
                self.current_function = previous_function;
                Ok(())
            }
            STATEMENT_IF => {
                self.visit_expression(
                    self.request.statements[statement_index].condition_expression_id,
                )?;
                self.visit_block(self.request.statements[statement_index].body_block_id)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(
                    self.request.statements[statement_index].switch_expression_id,
                )?;
                let case_ids_len = self.request.statements[statement_index].case_ids.len();
                for index in 0..case_ids_len {
                    let case_id = self.request.statements[statement_index].case_ids[index];
                    self.visit_block(self.request.cases[case_id as usize].body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.graph
                    .functions_with_loops
                    .insert(self.current_function);
                self.visit_block(self.request.statements[statement_index].pre_block_id)?;
                self.visit_expression(
                    self.request.statements[statement_index].condition_expression_id,
                )?;
                self.visit_block(self.request.statements[statement_index].body_block_id)?;
                self.visit_block(self.request.statements[statement_index].post_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(self.request.statements[statement_index].block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression_index = expression_id as usize;
        if self.request.expressions[expression_index].kind == EXPRESSION_FUNCTION_CALL {
            let callee = function_handle_from_parts(
                self.request.expressions[expression_index].function_name_kind,
                self.request.expressions[expression_index].function_name_name_id,
                self.request.expressions[expression_index].function_name_builtin_handle,
            )?;

            {
                let callees = self
                    .graph
                    .function_calls
                    .entry(self.current_function)
                    .or_default();
                if !callees.contains(&callee) {
                    callees.push(callee);
                }
            }

            let argument_ids_len = self.request.expressions[expression_index]
                .argument_expression_ids
                .len();
            for index in (0..argument_ids_len).rev() {
                let argument_id =
                    self.request.expressions[expression_index].argument_expression_ids[index];
                self.visit_expression(argument_id)?;
            }
        }
        Ok(())
    }
}

fn side_effects(
    call_graph: &CallGraph,
    builtin_effects: &BTreeMap<u64, SideEffects>,
) -> BTreeMap<FunctionHandle, SideEffects> {
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
                builtin_effects,
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

fn msize_builtin_handles(request: &ffi::WireYulOptimizerRequest) -> BTreeSet<u64> {
    request
        .builtins
        .iter()
        .filter_map(|builtin| builtin.is_msize.then_some(builtin.handle_id))
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

fn function_handle_from_parts(
    function_name_kind: u8,
    function_name_name_id: u64,
    function_name_builtin_handle: u64,
) -> Result<FunctionHandle, OptimizerError> {
    match function_name_kind {
        FUNCTION_NAME_IDENTIFIER => Ok(FunctionHandle::Name(function_name_name_id)),
        FUNCTION_NAME_BUILTIN => Ok(FunctionHandle::Builtin(function_name_builtin_handle)),
        _ => Err(OptimizerError::InvalidWire(format!(
            "invalid function name kind: {}",
            function_name_kind
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
