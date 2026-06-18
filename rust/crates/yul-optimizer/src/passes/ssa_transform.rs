use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, FUNCTION_NAME_IDENTIFIER,
    LITERAL_NUMBER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION,
    STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn run(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut super::NameDispenser,
) -> Result<(), OptimizerError> {
    let root_block_id = request.root_block_id;
    let variables_to_replace = assigned_variable_names(request, root_block_id)?;

    IntroduceSSA {
        request,
        variables_to_replace: &variables_to_replace,
        name_dispenser,
    }
    .visit_block(root_block_id)?;

    IntroduceControlFlowSSA {
        request,
        variables_to_replace: &variables_to_replace,
        name_dispenser,
        variables_in_scope: BTreeSet::new(),
        variables_to_reassign: Vec::new(),
    }
    .visit_block(root_block_id)?;

    PropagateValues {
        request,
        variables_to_replace: &variables_to_replace,
        current_variable_values: BTreeMap::new(),
        clear_at_end_of_block: BTreeSet::new(),
    }
    .visit_block(root_block_id)
}

struct IntroduceSSA<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    variables_to_replace: &'b BTreeSet<u64>,
    name_dispenser: &'b mut super::NameDispenser,
}

impl IntroduceSSA<'_, '_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten = Vec::new();

        for statement_id in statement_ids {
            let statement = self.request.statements[statement_id as usize].clone();
            match statement.kind {
                STATEMENT_VARIABLE_DECLARATION => {
                    if statement.has_value {
                        self.visit_expression(statement.value_expression_id)?;
                    }

                    let need_replace = statement.variable_ids.iter().any(|variable_id| {
                        let name = self.request.names[*variable_id as usize].name_id;
                        self.variables_to_replace.contains(&name)
                    });
                    if !need_replace {
                        rewritten.push(statement_id);
                        continue;
                    }

                    let debug_data_id = statement.debug_data_id;
                    let mut new_variables = Vec::with_capacity(statement.variable_ids.len());
                    let mut replacement_ids = Vec::new();
                    for variable_id in &statement.variable_ids {
                        let old_name = self.request.names[*variable_id as usize].name_id;
                        let new_name = self.name_dispenser.new_name_like(self.request, old_name);
                        new_variables.push(self.push_name(debug_data_id, new_name));

                        let value_id = self.push_identifier_expression(debug_data_id, new_name);
                        let old_variable_id = self.push_name(debug_data_id, old_name);
                        replacement_ids.push(self.push_variable_declaration(
                            debug_data_id,
                            vec![old_variable_id],
                            Some(value_id),
                        ));
                    }

                    let first = self.push_variable_declaration(
                        debug_data_id,
                        new_variables,
                        statement.has_value.then_some(statement.value_expression_id),
                    );
                    rewritten.push(first);
                    rewritten.extend(replacement_ids);
                }
                STATEMENT_ASSIGNMENT => {
                    self.visit_expression(statement.value_expression_id)?;
                    let debug_data_id = statement.debug_data_id;
                    let mut new_variables = Vec::with_capacity(statement.variable_ids.len());
                    let mut replacement_ids = Vec::new();

                    for variable_id in &statement.variable_ids {
                        let old_name = self.request.identifiers[*variable_id as usize].name_id;
                        if !self.variables_to_replace.contains(&old_name) {
                            return Err(OptimizerError::InvalidWire(
                                "SSATransform assignment target was not selected".to_string(),
                            ));
                        }
                        let new_name = self.name_dispenser.new_name_like(self.request, old_name);
                        new_variables.push(self.push_name(debug_data_id, new_name));

                        let lhs = self.push_identifier(debug_data_id, old_name);
                        let rhs = self.push_identifier_expression(debug_data_id, new_name);
                        replacement_ids.push(self.push_assignment(debug_data_id, vec![lhs], rhs));
                    }

                    let first = self.push_variable_declaration(
                        debug_data_id,
                        new_variables,
                        Some(statement.value_expression_id),
                    );
                    rewritten.push(first);
                    rewritten.extend(replacement_ids);
                }
                _ => {
                    self.visit_statement(statement_id)?;
                    rewritten.push(statement_id);
                }
            }
        }

        self.request.blocks[block_id as usize].statement_ids = rewritten;
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
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

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }

    fn push_name(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let id = self.request.names.len() as u64;
        self.request.names.push(ffi::WireNameWithDebugData {
            debug_data_id,
            name_id,
        });
        id
    }

    fn push_identifier(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let id = self.request.identifiers.len() as u64;
        self.request.identifiers.push(ffi::WireIdentifier {
            debug_data_id,
            name_id,
        });
        id
    }

    fn push_identifier_expression(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let id = self.request.expressions.len() as u64;
        self.request
            .expressions
            .push(identifier_expression(debug_data_id, name_id));
        id
    }

    fn push_assignment(
        &mut self,
        debug_data_id: u64,
        variable_ids: Vec<u64>,
        value_expression_id: u64,
    ) -> u64 {
        let id = self.request.statements.len() as u64;
        self.request.statements.push(assignment_statement(
            debug_data_id,
            variable_ids,
            value_expression_id,
        ));
        id
    }

    fn push_variable_declaration(
        &mut self,
        debug_data_id: u64,
        variable_ids: Vec<u64>,
        value_expression_id: Option<u64>,
    ) -> u64 {
        let id = self.request.statements.len() as u64;
        self.request.statements.push(variable_declaration_statement(
            debug_data_id,
            variable_ids,
            value_expression_id,
        ));
        id
    }
}

struct IntroduceControlFlowSSA<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    variables_to_replace: &'b BTreeSet<u64>,
    name_dispenser: &'b mut super::NameDispenser,
    variables_in_scope: BTreeSet<u64>,
    variables_to_reassign: Vec<u64>,
}

impl IntroduceControlFlowSSA<'_, '_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten = Vec::new();
        let mut variables_declared_here = Vec::new();
        let mut assigned_variables = Vec::new();

        for statement_id in statement_ids {
            let statement = self.request.statements[statement_id as usize].clone();
            let mut to_prepend = Vec::new();
            let pending = std::mem::take(&mut self.variables_to_reassign);
            for to_reassign in pending {
                let debug_data_id = statement.debug_data_id;
                let new_name = self.name_dispenser.new_name_like(self.request, to_reassign);
                let value_id = self.push_identifier_expression(debug_data_id, to_reassign);
                let variable_id = self.push_name(debug_data_id, new_name);
                to_prepend.push(self.push_variable_declaration(
                    debug_data_id,
                    vec![variable_id],
                    Some(value_id),
                ));
                push_unique(&mut assigned_variables, to_reassign);
            }

            match statement.kind {
                STATEMENT_VARIABLE_DECLARATION => {
                    for variable_id in &statement.variable_ids {
                        let name = self.request.names[*variable_id as usize].name_id;
                        if self.variables_to_replace.contains(&name) {
                            push_unique(&mut variables_declared_here, name);
                            self.variables_in_scope.insert(name);
                        }
                    }
                }
                STATEMENT_ASSIGNMENT => {
                    for variable_id in &statement.variable_ids {
                        let name = self.request.identifiers[*variable_id as usize].name_id;
                        if self.variables_to_replace.contains(&name) {
                            push_unique(&mut assigned_variables, name);
                        }
                    }
                }
                _ => self.visit_statement(statement_id)?,
            }

            rewritten.extend(to_prepend);
            rewritten.push(statement_id);
        }

        for variable in assigned_variables {
            push_unique(&mut self.variables_to_reassign, variable);
        }
        for variable in &variables_declared_here {
            self.variables_in_scope.remove(variable);
        }
        self.variables_to_reassign
            .retain(|variable| !variables_declared_here.contains(variable));

        self.request.blocks[block_id as usize].statement_ids = rewritten;
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => self.visit_function_definition(&statement),
            STATEMENT_FOR_LOOP => self.visit_for_loop(&statement),
            STATEMENT_SWITCH => self.visit_switch(&statement),
            STATEMENT_IF => self.visit_block(statement.body_block_id),
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_function_definition(
        &mut self,
        statement: &ffi::WireStatement,
    ) -> Result<(), OptimizerError> {
        let saved_scope = std::mem::take(&mut self.variables_in_scope);
        let saved_reassign = std::mem::take(&mut self.variables_to_reassign);

        for parameter_id in &statement.parameter_ids {
            let name = self.request.names[*parameter_id as usize].name_id;
            if self.variables_to_replace.contains(&name) {
                self.variables_in_scope.insert(name);
                push_unique(&mut self.variables_to_reassign, name);
            }
        }

        self.visit_block(statement.body_block_id)?;

        self.variables_in_scope = saved_scope;
        self.variables_to_reassign = saved_reassign;
        Ok(())
    }

    fn visit_for_loop(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "SSATransform requires ForLoopInitRewriter.".to_string(),
            ));
        }

        let mut assigned = assigned_variable_names(self.request, statement.body_block_id)?;
        assigned.extend(assigned_variable_names(
            self.request,
            statement.post_block_id,
        )?);
        for variable in names_sorted_by_yul_string_order(self.request, &assigned) {
            if self.variables_in_scope.contains(&variable) {
                push_unique(&mut self.variables_to_reassign, variable);
            }
        }

        self.visit_block(statement.body_block_id)?;
        self.visit_block(statement.post_block_id)
    }

    fn visit_switch(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.variables_to_reassign.is_empty() {
            return Err(OptimizerError::InvalidWire(
                "SSATransform switch encountered pending reassignments".to_string(),
            ));
        }

        let mut to_reassign = Vec::new();
        for case_id in &statement.case_ids {
            let body_block_id = self.request.cases[*case_id as usize].body_block_id;
            self.visit_block(body_block_id)?;
            let pending = self.variables_to_reassign.clone();
            for variable in pending {
                push_unique(&mut to_reassign, variable);
            }
        }
        for variable in to_reassign {
            push_unique(&mut self.variables_to_reassign, variable);
        }
        Ok(())
    }

    fn push_name(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let id = self.request.names.len() as u64;
        self.request.names.push(ffi::WireNameWithDebugData {
            debug_data_id,
            name_id,
        });
        id
    }

    fn push_identifier_expression(&mut self, debug_data_id: u64, name_id: u64) -> u64 {
        let id = self.request.expressions.len() as u64;
        self.request
            .expressions
            .push(identifier_expression(debug_data_id, name_id));
        id
    }

    fn push_variable_declaration(
        &mut self,
        debug_data_id: u64,
        variable_ids: Vec<u64>,
        value_expression_id: Option<u64>,
    ) -> u64 {
        let id = self.request.statements.len() as u64;
        self.request.statements.push(variable_declaration_statement(
            debug_data_id,
            variable_ids,
            value_expression_id,
        ));
        id
    }
}

struct PropagateValues<'a, 'b> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    variables_to_replace: &'b BTreeSet<u64>,
    current_variable_values: BTreeMap<u64, u64>,
    clear_at_end_of_block: BTreeSet<u64>,
}

impl PropagateValues<'_, '_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let parent_clear = std::mem::take(&mut self.clear_at_end_of_block);
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
        }
        for variable in &self.clear_at_end_of_block {
            self.current_variable_values.remove(variable);
        }
        self.clear_at_end_of_block = parent_clear;
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => self.visit_assignment(statement_id),
            STATEMENT_VARIABLE_DECLARATION => self.visit_variable_declaration(statement_id),
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
            STATEMENT_FOR_LOOP => self.visit_for_loop(&statement),
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_variable_declaration(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        if statement.has_value {
            self.visit_expression(statement.value_expression_id)?;
        }
        if statement.variable_ids.len() != 1 {
            return Ok(());
        }
        let variable = self.request.names[statement.variable_ids[0] as usize].name_id;
        if self.variables_to_replace.contains(&variable) {
            if let Some(value) = self.identifier_value_name(statement.value_expression_id) {
                self.current_variable_values.insert(variable, value);
                self.clear_at_end_of_block.insert(variable);
            }
        } else if statement.has_value {
            if let Some(value) = self.identifier_value_name(statement.value_expression_id) {
                if self.variables_to_replace.contains(&value) {
                    self.current_variable_values.insert(value, variable);
                    self.clear_at_end_of_block.insert(value);
                }
            }
        }
        Ok(())
    }

    fn visit_assignment(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        self.visit_expression(statement.value_expression_id)?;
        if statement.variable_ids.len() != 1 {
            return Ok(());
        }
        let name = self.request.identifiers[statement.variable_ids[0] as usize].name_id;
        if !self.variables_to_replace.contains(&name) {
            return Ok(());
        }
        if let Some(value) = self.identifier_value_name(statement.value_expression_id) {
            self.current_variable_values.insert(name, value);
            self.clear_at_end_of_block.insert(name);
        }
        Ok(())
    }

    fn visit_for_loop(&mut self, statement: &ffi::WireStatement) -> Result<(), OptimizerError> {
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "SSATransform requires ForLoopInitRewriter.".to_string(),
            ));
        }
        let mut assigned = assigned_variable_names(self.request, statement.body_block_id)?;
        assigned.extend(assigned_variable_names(
            self.request,
            statement.post_block_id,
        )?);
        for variable in assigned {
            self.current_variable_values.remove(&variable);
        }
        self.visit_expression(statement.condition_expression_id)?;
        self.visit_block(statement.body_block_id)?;
        self.visit_block(statement.post_block_id)
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        match expression.kind {
            EXPRESSION_IDENTIFIER => {
                if let Some(value) = self
                    .current_variable_values
                    .get(&expression.name_id)
                    .copied()
                {
                    self.request.expressions[expression_id as usize].name_id = value;
                }
            }
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn identifier_value_name(&self, expression_id: u64) -> Option<u64> {
        let expression = &self.request.expressions[expression_id as usize];
        (expression.kind == EXPRESSION_IDENTIFIER).then_some(expression.name_id)
    }
}

fn assigned_variable_names(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<BTreeSet<u64>, OptimizerError> {
    let mut collector = AssignedVariableCollector {
        request,
        names: BTreeSet::new(),
    };
    collector.visit_block(block_id)?;
    Ok(collector.names)
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

fn push_unique(values: &mut Vec<u64>, value: u64) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn names_sorted_by_yul_string_order(
    request: &ffi::WireYulOptimizerRequest,
    names: &BTreeSet<u64>,
) -> Vec<u64> {
    let mut output = names.iter().copied().collect::<Vec<_>>();
    output.sort_by(|left, right| {
        let left_bytes = request
            .strings
            .get(*left as usize)
            .map(|string| string.bytes.as_slice())
            .unwrap_or_default();
        let right_bytes = request
            .strings
            .get(*right as usize)
            .map(|string| string.bytes.as_slice())
            .unwrap_or_default();
        yul_string_hash(left_bytes)
            .cmp(&yul_string_hash(right_bytes))
            .then_with(|| left_bytes.cmp(right_bytes))
            .then_with(|| left.cmp(right))
    });
    output
}

fn yul_string_hash(bytes: &[u8]) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in bytes {
        hash = hash.wrapping_mul(1_099_511_628_211);
        hash ^= u64::from(*byte);
    }
    hash
}

fn assignment_statement(
    debug_data_id: u64,
    variable_ids: Vec<u64>,
    value_expression_id: u64,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_ASSIGNMENT,
        debug_data_id,
        expression_id: 0,
        has_value: true,
        value_expression_id,
        variable_ids,
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

fn variable_declaration_statement(
    debug_data_id: u64,
    variable_ids: Vec<u64>,
    value_expression_id: Option<u64>,
) -> ffi::WireStatement {
    ffi::WireStatement {
        kind: STATEMENT_VARIABLE_DECLARATION,
        debug_data_id,
        expression_id: 0,
        has_value: value_expression_id.is_some(),
        value_expression_id: value_expression_id.unwrap_or(0),
        variable_ids,
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
        function_name_kind: FUNCTION_NAME_IDENTIFIER,
        function_name_debug_data_id: 0,
        function_name_name_id: 0,
        function_name_builtin_handle: 0,
        argument_expression_ids: Vec::new(),
    }
}
