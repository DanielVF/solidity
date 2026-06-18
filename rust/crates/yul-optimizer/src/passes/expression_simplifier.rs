use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use primitive_types::U256;
use std::collections::{BTreeMap, BTreeSet};

use super::expression_simplifier_rules::{
    instruction_name_for_opcode, simplification_rules, MatchGroups, MatchedExpression, Pattern,
    PatternKind,
};

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
    is_function: bool,
}

#[derive(Clone, Debug, Default)]
struct MovableInfo {
    movable: bool,
    referenced_variables: BTreeSet<u64>,
}

enum ResolvedExpression {
    Expression(u64),
    Zero,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    ExpressionSimplifier {
        request,
        state: DataFlowState::default(),
        loop_depth: 0,
        variable_scopes: Vec::new(),
    }
    .visit_block_root()
}

struct ExpressionSimplifier<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    state: DataFlowState,
    loop_depth: usize,
    variable_scopes: Vec<Scope>,
}

impl ExpressionSimplifier<'_> {
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
                "scope stack mismatch in ExpressionSimplifier".to_string(),
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
                "ExpressionSimplifier requires ForLoopInitRewriter.".to_string(),
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
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }

        while self.simplify_once(expression_id)? {}

        self.normalize_zero_length_accesses(expression_id)
    }

    fn simplify_once(&mut self, expression_id: u64) -> Result<bool, OptimizerError> {
        let Some((replacement, groups)) = self.find_simplification(expression_id)? else {
            return Ok(false);
        };

        let debug_data_id = self.request.expressions[expression_id as usize].debug_data_id;
        let replacement_id = self.expression_from_pattern(&replacement, &groups, debug_data_id)?;
        self.request.expressions[expression_id as usize] =
            self.request.expressions[replacement_id as usize].clone();
        Ok(true)
    }

    fn find_simplification(
        &self,
        expression_id: u64,
    ) -> Result<Option<(Pattern, MatchGroups)>, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
        {
            return Ok(None);
        }

        let Some(name) = self.builtin_instruction_name(expression.function_name_builtin_handle)
        else {
            return Ok(None);
        };

        for rule in simplification_rules().iter().filter(|rule| {
            rule.root_name == name && rule.gate.available(self.request.dialect.evm_version)
        }) {
            let mut groups = MatchGroups::new();
            if !self.pattern_matches(&rule.pattern, expression_id, &mut groups)? {
                continue;
            }
            if rule
                .feasible
                .as_ref()
                .is_some_and(|feasible| !feasible(&groups))
            {
                continue;
            }
            return Ok(Some(((rule.action)(&groups), groups)));
        }

        Ok(None)
    }

    fn normalize_zero_length_accesses(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
        {
            return Ok(());
        }

        let Some(name) = self.builtin_name(expression.function_name_builtin_handle) else {
            return Ok(());
        };

        for (start_index, length_index) in
            read_write_start_length_parameters(&name, expression.argument_expression_ids.len())
        {
            let Some(start_argument) = expression.argument_expression_ids.get(start_index).copied()
            else {
                continue;
            };
            let Some(length_argument) = expression
                .argument_expression_ids
                .get(length_index)
                .copied()
            else {
                continue;
            };

            if self.known_to_be_zero(length_argument)
                && !self.known_to_be_zero(start_argument)
                && !self.expression_is_function_call(start_argument)
            {
                let debug_data_id = self.request.expressions[start_argument as usize].debug_data_id;
                self.request.expressions[start_argument as usize] =
                    numeric_literal(debug_data_id, U256::zero());
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
                if movable.movable && !referenced_variables.contains(&name) {
                    self.assign_value(name, ValueRef::Expression(value_id));
                }
            }
        } else {
            for name in &names {
                self.assign_value(*name, ValueRef::Zero);
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
                        info.movable &= self
                            .builtin(expression.function_name_builtin_handle)
                            .is_some_and(|builtin| builtin.movable);
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

    fn pattern_matches(
        &self,
        pattern: &Pattern,
        expression_id: u64,
        groups: &mut MatchGroups,
    ) -> Result<bool, OptimizerError> {
        match &pattern.kind {
            PatternKind::Any => self.any_pattern_matches(pattern, expression_id, groups),
            PatternKind::ConstantAny | PatternKind::ConstantValue(_) => {
                self.constant_pattern_matches(pattern, expression_id, groups)
            }
            PatternKind::Operation(name) => {
                self.operation_pattern_matches(*name, pattern, expression_id, groups)
            }
        }
    }

    fn any_pattern_matches(
        &self,
        pattern: &Pattern,
        expression_id: u64,
        groups: &mut MatchGroups,
    ) -> Result<bool, OptimizerError> {
        if self.expression_is_function_call(expression_id) {
            return Ok(false);
        }

        if let Some(group) = pattern.match_group {
            if let Some(existing) = groups.get(group) {
                return self.syntactically_equal(existing.expression_id, expression_id);
            }
            groups.insert(
                group,
                MatchedExpression {
                    expression_id,
                    constant: literal_numeric_value(
                        &self.request.expressions[expression_id as usize],
                    ),
                },
            );
        }

        Ok(true)
    }

    fn constant_pattern_matches(
        &self,
        pattern: &Pattern,
        expression_id: u64,
        groups: &mut MatchGroups,
    ) -> Result<bool, OptimizerError> {
        let resolved = self.resolved_expression_for_pattern(expression_id);
        let Some(value) = self.numeric_value_of_resolved(&resolved) else {
            return Ok(false);
        };

        if let PatternKind::ConstantValue(expected) = pattern.kind {
            if value != expected {
                return Ok(false);
            }
        }

        if let Some(group) = pattern.match_group {
            groups.insert(
                group,
                MatchedExpression {
                    expression_id: match resolved {
                        ResolvedExpression::Expression(resolved_id) => resolved_id,
                        ResolvedExpression::Zero => expression_id,
                    },
                    constant: Some(value),
                },
            );
        }

        Ok(true)
    }

    fn operation_pattern_matches(
        &self,
        name: &'static str,
        pattern: &Pattern,
        expression_id: u64,
        groups: &mut MatchGroups,
    ) -> Result<bool, OptimizerError> {
        let ResolvedExpression::Expression(resolved_id) =
            self.resolved_expression_for_pattern(expression_id)
        else {
            return Ok(false);
        };
        let expression = &self.request.expressions[resolved_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL
            || expression.function_name_kind != FUNCTION_NAME_BUILTIN
        {
            return Ok(false);
        }

        if self.builtin_instruction_name(expression.function_name_builtin_handle) != Some(name) {
            return Ok(false);
        }
        if expression.argument_expression_ids.len() != pattern.arguments.len() {
            return Ok(false);
        }

        for (argument_pattern, argument_id) in pattern
            .arguments
            .iter()
            .zip(&expression.argument_expression_ids)
        {
            if self.expression_is_function_call(*argument_id) {
                return Ok(false);
            }
            if !self.pattern_matches(argument_pattern, *argument_id, groups)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn resolved_expression_for_pattern(&self, expression_id: u64) -> ResolvedExpression {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind == EXPRESSION_IDENTIFIER
            && self.value_references_are_in_scope(expression.name_id)
        {
            match self.state.value.get(&expression.name_id).copied() {
                Some(ValueRef::Expression(value_id)) => ResolvedExpression::Expression(value_id),
                Some(ValueRef::Zero) => ResolvedExpression::Zero,
                None => ResolvedExpression::Expression(expression_id),
            }
        } else {
            ResolvedExpression::Expression(expression_id)
        }
    }

    fn numeric_value_of_resolved(&self, resolved: &ResolvedExpression) -> Option<U256> {
        match resolved {
            ResolvedExpression::Expression(expression_id) => {
                literal_numeric_value(&self.request.expressions[*expression_id as usize])
            }
            ResolvedExpression::Zero => Some(U256::zero()),
        }
    }

    fn value_of_identifier(&self, name_id: u64) -> Option<U256> {
        match self.state.value.get(&name_id).copied()? {
            ValueRef::Expression(expression_id) => {
                literal_numeric_value(&self.request.expressions[expression_id as usize])
            }
            ValueRef::Zero => Some(U256::zero()),
        }
    }

    fn value_references_are_in_scope(&self, name_id: u64) -> bool {
        self.state
            .sorted_references
            .get(&name_id)
            .into_iter()
            .flatten()
            .all(|referenced| self.in_scope(*referenced))
    }

    fn known_to_be_zero(&self, expression_id: u64) -> bool {
        let expression = &self.request.expressions[expression_id as usize];
        match expression.kind {
            EXPRESSION_LITERAL => {
                literal_numeric_value(expression).is_some_and(|value| value.is_zero())
            }
            EXPRESSION_IDENTIFIER => self
                .value_of_identifier(expression.name_id)
                .is_some_and(|value| value.is_zero()),
            _ => false,
        }
    }

    fn expression_is_function_call(&self, expression_id: u64) -> bool {
        self.request.expressions[expression_id as usize].kind == EXPRESSION_FUNCTION_CALL
    }

    fn syntactically_equal(&self, left: u64, right: u64) -> Result<bool, OptimizerError> {
        let left_expression = &self.request.expressions[left as usize];
        let right_expression = &self.request.expressions[right as usize];
        if left_expression.kind != right_expression.kind {
            return Ok(false);
        }

        match left_expression.kind {
            EXPRESSION_FUNCTION_CALL => {
                if left_expression.function_name_kind != right_expression.function_name_kind
                    || left_expression.function_name_name_id
                        != right_expression.function_name_name_id
                    || left_expression.function_name_builtin_handle
                        != right_expression.function_name_builtin_handle
                    || left_expression.argument_expression_ids.len()
                        != right_expression.argument_expression_ids.len()
                {
                    return Ok(false);
                }

                for (left_argument, right_argument) in left_expression
                    .argument_expression_ids
                    .iter()
                    .zip(&right_expression.argument_expression_ids)
                {
                    if !self.syntactically_equal(*left_argument, *right_argument)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            EXPRESSION_IDENTIFIER => Ok(left_expression.name_id == right_expression.name_id),
            EXPRESSION_LITERAL => Ok(literals_equal(left_expression, right_expression)),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                left_expression.kind
            ))),
        }
    }

    fn clone_expression_tree(&mut self, expression_id: u64) -> Result<u64, OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        let mut cloned = expression.clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            let mut cloned_arguments = Vec::with_capacity(expression.argument_expression_ids.len());
            for argument_id in expression.argument_expression_ids {
                cloned_arguments.push(self.clone_expression_tree(argument_id)?);
            }
            cloned.argument_expression_ids = cloned_arguments;
        }

        let cloned_id = self.request.expressions.len() as u64;
        self.request.expressions.push(cloned);
        Ok(cloned_id)
    }

    fn expression_from_pattern(
        &mut self,
        pattern: &Pattern,
        groups: &MatchGroups,
        debug_data_id: u64,
    ) -> Result<u64, OptimizerError> {
        match &pattern.kind {
            PatternKind::Any => {
                let group = pattern.match_group.ok_or_else(|| {
                    OptimizerError::InvalidWire("unbound Any simplification pattern".to_string())
                })?;
                let expression_id = groups
                    .get(group)
                    .ok_or_else(|| {
                        OptimizerError::InvalidWire(
                            "missing Any simplification match group".to_string(),
                        )
                    })?
                    .expression_id;
                self.clone_expression_tree(expression_id)
            }
            PatternKind::ConstantAny => {
                let group = pattern.match_group.ok_or_else(|| {
                    OptimizerError::InvalidWire(
                        "unbound constant simplification pattern".to_string(),
                    )
                })?;
                let value = groups
                    .get(group)
                    .and_then(|matched| matched.constant)
                    .ok_or_else(|| {
                        OptimizerError::InvalidWire(
                            "missing constant simplification match group".to_string(),
                        )
                    })?;
                self.push_expression(numeric_literal(debug_data_id, value))
            }
            PatternKind::ConstantValue(value) => {
                self.push_expression(numeric_literal(debug_data_id, *value))
            }
            PatternKind::Operation(name) => {
                let mut argument_ids = Vec::with_capacity(pattern.arguments.len());
                for argument in &pattern.arguments {
                    argument_ids.push(self.expression_from_pattern(
                        argument,
                        groups,
                        debug_data_id,
                    )?);
                }
                let handle = self
                    .builtin_handle_by_instruction_name(name)
                    .ok_or_else(|| {
                        OptimizerError::InvalidWire(format!(
                            "missing builtin for simplification instruction: {name}"
                        ))
                    })?;
                self.push_expression(builtin_call(debug_data_id, handle, argument_ids))
            }
        }
    }

    fn push_expression(&mut self, expression: ffi::WireExpression) -> Result<u64, OptimizerError> {
        let expression_id = self.request.expressions.len() as u64;
        self.request.expressions.push(expression);
        Ok(expression_id)
    }

    fn builtin_name(&self, handle: u64) -> Option<String> {
        let builtin = self.builtin(handle)?;
        let bytes = &self.request.strings[builtin.name_id as usize].bytes;
        std::str::from_utf8(bytes)
            .ok()
            .map(|name| name.to_ascii_lowercase())
    }

    fn builtin_instruction_name(&self, handle: u64) -> Option<&'static str> {
        let builtin = self.builtin(handle)?;
        builtin
            .has_evm_opcode
            .then(|| instruction_name_for_opcode(builtin.evm_opcode))
            .flatten()
    }

    fn builtin_handle_by_instruction_name(&self, wanted: &str) -> Option<u64> {
        self.request
            .builtins
            .iter()
            .find(|builtin| {
                builtin.has_evm_opcode
                    && instruction_name_for_opcode(builtin.evm_opcode) == Some(wanted)
            })
            .map(|builtin| builtin.handle_id)
            .or_else(|| self.builtin_handle_by_name(wanted))
    }

    fn builtin_handle_by_name(&self, wanted: &str) -> Option<u64> {
        self.request.builtins.iter().find_map(|builtin| {
            let bytes = &self.request.strings[builtin.name_id as usize].bytes;
            std::str::from_utf8(bytes)
                .ok()
                .filter(|name| name.eq_ignore_ascii_case(wanted))
                .map(|_| builtin.handle_id)
        })
    }

    fn builtin(&self, handle: u64) -> Option<&ffi::WireBuiltinFunction> {
        self.request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == handle)
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

fn literal_numeric_value(expression: &ffi::WireExpression) -> Option<U256> {
    if expression.kind == EXPRESSION_LITERAL
        && expression.literal_kind == LITERAL_NUMBER
        && !expression.literal_unlimited
    {
        (expression.literal_value.len() == 32)
            .then(|| U256::from_big_endian(&expression.literal_value))
    } else {
        None
    }
}

fn literals_equal(left: &ffi::WireExpression, right: &ffi::WireExpression) -> bool {
    match (literal_numeric_value(left), literal_numeric_value(right)) {
        (Some(left_value), Some(right_value)) => left_value == right_value,
        _ => {
            left.literal_kind == right.literal_kind
                && left.literal_unlimited == right.literal_unlimited
                && left.literal_value == right.literal_value
                && left.literal_string_id == right.literal_string_id
        }
    }
}

fn numeric_literal(debug_data_id: u64, value: U256) -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_LITERAL,
        debug_data_id,
        literal_kind: LITERAL_NUMBER,
        literal_unlimited: false,
        literal_value: u256_to_vec(value),
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

fn builtin_call(debug_data_id: u64, handle: u64, arguments: Vec<u64>) -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_FUNCTION_CALL,
        debug_data_id,
        literal_kind: LITERAL_NUMBER,
        literal_unlimited: false,
        literal_value: u256_to_vec(U256::zero()),
        literal_string_id: 0,
        has_literal_hint: false,
        literal_hint_id: 0,
        name_id: 0,
        function_name_kind: FUNCTION_NAME_BUILTIN,
        function_name_debug_data_id: debug_data_id,
        function_name_name_id: 0,
        function_name_builtin_handle: handle,
        argument_expression_ids: arguments,
    }
}

fn read_write_start_length_parameters(name: &str, parameter_count: usize) -> Vec<(usize, usize)> {
    match name {
        "return" | "revert" | "keccak256" => vec![(0, 1)],
        "log0" | "log1" | "log2" | "log3" | "log4" => vec![(0, 1)],
        "extcodecopy" => vec![(1, 3)],
        "codecopy" | "calldatacopy" | "returndatacopy" => vec![(0, 2)],
        "mcopy" => vec![(1, 2), (0, 2)],
        "call" | "callcode" => vec![(parameter_count.saturating_sub(4), parameter_count - 3)],
        "staticcall" | "delegatecall" => {
            vec![(parameter_count.saturating_sub(4), parameter_count - 3)]
        }
        "create" | "create2" => vec![(1, 2)],
        _ => Vec::new(),
    }
}

fn u256_to_vec(value: U256) -> Vec<u8> {
    let mut output = vec![0; 32];
    value.to_big_endian(&mut output);
    output
}
