use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EFFECT_NONE, EFFECT_READ, EFFECT_WRITE, EXPRESSION_FUNCTION_CALL,
    EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

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

    fn movable_relative_to(self, other: Self, code_contains_msize: bool) -> bool {
        if !self.cannot_loop {
            return false;
        }
        if self.movable {
            return true;
        }
        if !self.movable_apart_from_effects
            || self.storage == EFFECT_WRITE
            || self.other_state == EFFECT_WRITE
            || self.memory == EFFECT_WRITE
            || self.transient_storage == EFFECT_WRITE
        {
            return false;
        }
        if self.other_state == EFFECT_READ && other.other_state == EFFECT_WRITE {
            return false;
        }
        if self.storage == EFFECT_READ && other.storage == EFFECT_WRITE {
            return false;
        }
        if self.memory == EFFECT_READ && (code_contains_msize || other.memory == EFFECT_WRITE) {
            return false;
        }
        if self.transient_storage == EFFECT_READ && other.transient_storage == EFFECT_WRITE {
            return false;
        }
        true
    }
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let call_graph = call_graph(request, request.root_block_id)?;
    let function_side_effects = side_effects(request, &call_graph);
    let builtin_side_effects = builtin_side_effects(request);
    let contains_msize = contains_msize(request)?;
    let ssa_variables = ssa_variables(request)?;

    LoopInvariantCodeMotion {
        request,
        contains_msize,
        ssa_variables,
        function_side_effects,
        builtin_side_effects,
    }
    .visit_block_root()
}

struct LoopInvariantCodeMotion<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    contains_msize: bool,
    ssa_variables: BTreeSet<u64>,
    function_side_effects: BTreeMap<FunctionHandle, SideEffects>,
    builtin_side_effects: BTreeMap<u64, SideEffects>,
}

impl LoopInvariantCodeMotion<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let original_statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten_statement_ids = Vec::with_capacity(original_statement_ids.len());

        for statement_id in original_statement_ids {
            self.visit_statement(statement_id)?;
            if self.request.statements[statement_id as usize].kind == STATEMENT_FOR_LOOP {
                let mut replacement = self.rewrite_loop(statement_id)?;
                if replacement.is_empty() {
                    rewritten_statement_ids.push(statement_id);
                } else {
                    rewritten_statement_ids.append(&mut replacement);
                }
            } else {
                rewritten_statement_ids.push(statement_id);
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten_statement_ids;
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

    fn rewrite_loop(&mut self, statement_id: u64) -> Result<Vec<u64>, OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "LoopInvariantCodeMotion requires ForLoopInitRewriter.".to_string(),
            ));
        }

        let for_loop_side_effects = self.side_effects_statement(statement_id)?;
        let mut promoted = Vec::new();
        self.promote_from_block(
            statement.post_block_id,
            for_loop_side_effects,
            &mut promoted,
        )?;
        self.promote_from_block(
            statement.body_block_id,
            for_loop_side_effects,
            &mut promoted,
        )?;

        if promoted.is_empty() {
            Ok(Vec::new())
        } else {
            promoted.push(statement_id);
            Ok(promoted)
        }
    }

    fn promote_from_block(
        &mut self,
        block_id: u64,
        for_loop_side_effects: SideEffects,
        promoted: &mut Vec<u64>,
    ) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut retained = Vec::with_capacity(statement_ids.len());
        let mut vars_defined_in_scope = BTreeSet::new();

        for statement_id in statement_ids {
            let statement = self.request.statements[statement_id as usize].clone();
            if statement.kind == STATEMENT_VARIABLE_DECLARATION
                && self.can_be_promoted(
                    &statement,
                    &vars_defined_in_scope,
                    for_loop_side_effects,
                )?
            {
                promoted.push(statement_id);
                continue;
            }

            if statement.kind == STATEMENT_VARIABLE_DECLARATION {
                for variable_id in &statement.variable_ids {
                    vars_defined_in_scope.insert(self.request.names[*variable_id as usize].name_id);
                }
            }
            retained.push(statement_id);
        }

        self.request.blocks[block_id as usize].statement_ids = retained;
        Ok(())
    }

    fn can_be_promoted(
        &self,
        statement: &ffi::WireStatement,
        vars_defined_in_scope: &BTreeSet<u64>,
        for_loop_side_effects: SideEffects,
    ) -> Result<bool, OptimizerError> {
        if statement.kind != STATEMENT_VARIABLE_DECLARATION {
            return Ok(false);
        }

        for variable_id in &statement.variable_ids {
            let name = self.request.names[*variable_id as usize].name_id;
            if !self.ssa_variables.contains(&name) {
                return Ok(false);
            }
        }

        if statement.has_value {
            let references = self.expression_references(statement.value_expression_id)?;
            if references.iter().any(|reference| {
                vars_defined_in_scope.contains(reference) || !self.ssa_variables.contains(reference)
            }) {
                return Ok(false);
            }

            let side_effects = self.side_effects_expression(statement.value_expression_id)?;
            if !side_effects.movable_relative_to(for_loop_side_effects, self.contains_msize) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn expression_references(&self, expression_id: u64) -> Result<BTreeSet<u64>, OptimizerError> {
        let mut references = BTreeSet::new();
        self.collect_expression_references(expression_id, &mut references)?;
        Ok(references)
    }

    fn collect_expression_references(
        &self,
        expression_id: u64,
        references: &mut BTreeSet<u64>,
    ) -> Result<(), OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in &expression.argument_expression_ids {
                    self.collect_expression_references(*argument_id, references)?;
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

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }

    fn side_effects_statement(&self, statement_id: u64) -> Result<SideEffects, OptimizerError> {
        let mut output = SideEffects::default();
        self.collect_statement_side_effects(statement_id, &mut output)?;
        Ok(output)
    }

    fn side_effects_expression(&self, expression_id: u64) -> Result<SideEffects, OptimizerError> {
        let mut output = SideEffects::default();
        self.collect_expression_side_effects(expression_id, &mut output)?;
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
                self.collect_block_side_effects(statement.body_block_id, output)?;
                self.collect_block_side_effects(statement.post_block_id, output)
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
}

fn ssa_variables(request: &ffi::WireYulOptimizerRequest) -> Result<BTreeSet<u64>, OptimizerError> {
    let mut tracker = SsaVariableTracker {
        request,
        variables: BTreeSet::new(),
    };
    tracker.visit_block(request.root_block_id)?;
    Ok(tracker.variables)
}

struct SsaVariableTracker<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    variables: BTreeSet<u64>,
}

impl SsaVariableTracker<'_> {
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
                    self.variables
                        .remove(&self.request.identifiers[*variable_id as usize].name_id);
                }
                Ok(())
            }
            STATEMENT_VARIABLE_DECLARATION => {
                if !statement.has_value {
                    for variable_id in &statement.variable_ids {
                        self.variables
                            .insert(self.request.names[*variable_id as usize].name_id);
                    }
                } else if statement.variable_ids.len() == 1 {
                    self.variables
                        .insert(self.request.names[statement.variable_ids[0] as usize].name_id);
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => {
                for variable_id in &statement.return_variable_ids {
                    self.variables
                        .insert(self.request.names[*variable_id as usize].name_id);
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
