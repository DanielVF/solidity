use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, LITERAL_NUMBER, STATEMENT_ASSIGNMENT,
    STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_LEAVE, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::BTreeMap;

const FRESH_DEBUG_DATA_ID: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ControlFlowSideEffects {
    #[allow(dead_code)]
    can_terminate: bool,
    #[allow(dead_code)]
    can_revert: bool,
    pub(super) can_continue: bool,
}

impl ControlFlowSideEffects {
    fn from_builtin(builtin: &ffi::WireBuiltinFunction) -> Self {
        Self {
            can_terminate: builtin.control_flow_can_terminate,
            can_revert: builtin.control_flow_can_revert,
            can_continue: builtin.control_flow_can_continue,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlFlow {
    FlowOut,
    Break,
    Continue,
    Terminate,
    Leave,
}

#[derive(Clone, Copy, Debug, Default)]
struct FlowSummary {
    falls_through: bool,
    exits_function: bool,
    breaks: bool,
    continues: bool,
}

impl FlowSummary {
    fn falls_through() -> Self {
        Self {
            falls_through: true,
            ..Self::default()
        }
    }

    fn exits_function() -> Self {
        Self {
            exits_function: true,
            ..Self::default()
        }
    }

    fn breaks() -> Self {
        Self {
            breaks: true,
            ..Self::default()
        }
    }

    fn continues() -> Self {
        Self {
            continues: true,
            ..Self::default()
        }
    }

    fn add_alternative(&mut self, other: Self) {
        self.falls_through |= other.falls_through;
        self.exits_function |= other.exits_function;
        self.breaks |= other.breaks;
        self.continues |= other.continues;
    }
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let function_side_effects = function_side_effects(request)?;
    ConditionalSimplifier {
        request,
        function_side_effects,
    }
    .visit_block_root()
}

struct ConditionalSimplifier<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    function_side_effects: BTreeMap<u64, ControlFlowSideEffects>,
}

impl ConditionalSimplifier<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let original_statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        let mut rewritten_statement_ids = Vec::with_capacity(original_statement_ids.len());

        for statement_id in original_statement_ids {
            self.visit_statement(statement_id)?;
            rewritten_statement_ids.push(statement_id);

            let statement = self.request.statements[statement_id as usize].clone();
            if statement.kind == STATEMENT_IF
                && self.request.expressions[statement.condition_expression_id as usize].kind
                    == EXPRESSION_IDENTIFIER
                && !self.request.blocks[statement.body_block_id as usize]
                    .statement_ids
                    .is_empty()
            {
                let last_statement_id = *self.request.blocks[statement.body_block_id as usize]
                    .statement_ids
                    .last()
                    .expect("non-empty checked above");
                if self.control_flow_kind(last_statement_id)? != ControlFlow::FlowOut {
                    let condition = self.request.expressions
                        [statement.condition_expression_id as usize]
                        .name_id;
                    let assignment_id =
                        self.new_condition_zero_assignment(statement.debug_data_id, condition);
                    rewritten_statement_ids.push(assignment_id);
                }
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
            STATEMENT_SWITCH => self.visit_switch(statement_id),
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

    fn visit_switch(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        self.visit_expression(statement.switch_expression_id)?;

        if self.request.expressions[statement.switch_expression_id as usize].kind
            != EXPRESSION_IDENTIFIER
        {
            self.visit_expression(statement.switch_expression_id)?;
            for case_id in statement.case_ids {
                let switch_case = self.request.cases[case_id as usize].clone();
                if switch_case.has_value {
                    self.visit_expression(switch_case.value_expression_id)?;
                }
                self.visit_block(switch_case.body_block_id)?;
            }
            return Ok(());
        }

        let switch_expression_name =
            self.request.expressions[statement.switch_expression_id as usize].name_id;

        for case_id in statement.case_ids {
            let switch_case = self.request.cases[case_id as usize].clone();
            if switch_case.has_value {
                self.visit_expression(switch_case.value_expression_id)?;
                let assignment_id = self.new_case_assignment(
                    switch_case.body_block_id,
                    switch_expression_name,
                    switch_case.value_expression_id,
                );
                self.request.blocks[switch_case.body_block_id as usize]
                    .statement_ids
                    .insert(0, assignment_id);
            }
            self.visit_block(switch_case.body_block_id)?;
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

    fn control_flow_kind(&self, statement_id: u64) -> Result<ControlFlow, OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_VARIABLE_DECLARATION if statement.has_value => {
                if self.contains_non_continuing_function_call(statement.value_expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_ASSIGNMENT => {
                if self.contains_non_continuing_function_call(statement.value_expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_EXPRESSION => {
                if self.contains_non_continuing_function_call(statement.expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_BREAK => Ok(ControlFlow::Break),
            STATEMENT_CONTINUE => Ok(ControlFlow::Continue),
            STATEMENT_LEAVE => Ok(ControlFlow::Leave),
            _ => Ok(ControlFlow::FlowOut),
        }
    }

    fn contains_non_continuing_function_call(
        &self,
        expression_id: u64,
    ) -> Result<bool, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL {
            return Ok(false);
        }

        for argument_id in &expression.argument_expression_ids {
            if self.contains_non_continuing_function_call(*argument_id)? {
                return Ok(true);
            }
        }

        match expression.function_name_kind {
            FUNCTION_NAME_BUILTIN => Ok(self
                .builtin_control_flow(expression.function_name_builtin_handle)
                .is_some_and(|side_effects| !side_effects.can_continue)),
            FUNCTION_NAME_IDENTIFIER => Ok(self
                .function_side_effects
                .get(&expression.function_name_name_id)
                .is_some_and(|side_effects| !side_effects.can_continue)),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid function name kind: {}",
                expression.function_name_kind
            ))),
        }
    }

    fn new_case_assignment(
        &mut self,
        body_block_id: u64,
        variable_name_id: u64,
        case_value_expression_id: u64,
    ) -> u64 {
        let debug_data_id = self.request.blocks[body_block_id as usize].debug_data_id;
        let value_expression_id = self.request.expressions.len() as u64;
        self.request
            .expressions
            .push(self.request.expressions[case_value_expression_id as usize].clone());
        self.new_assignment(debug_data_id, variable_name_id, value_expression_id)
    }

    fn new_condition_zero_assignment(&mut self, debug_data_id: u64, variable_name_id: u64) -> u64 {
        let value_expression_id = self.request.expressions.len() as u64;
        self.request.expressions.push(zero_literal_expression());
        self.new_assignment(debug_data_id, variable_name_id, value_expression_id)
    }

    fn new_assignment(
        &mut self,
        debug_data_id: u64,
        variable_name_id: u64,
        value_expression_id: u64,
    ) -> u64 {
        let identifier_id = self.request.identifiers.len() as u64;
        self.request.identifiers.push(ffi::WireIdentifier {
            debug_data_id,
            name_id: variable_name_id,
        });

        let statement_id = self.request.statements.len() as u64;
        let mut statement = empty_statement(STATEMENT_ASSIGNMENT, debug_data_id);
        statement.variable_ids.push(identifier_id);
        statement.has_value = true;
        statement.value_expression_id = value_expression_id;
        self.request.statements.push(statement);
        statement_id
    }

    fn builtin_control_flow(&self, handle_id: u64) -> Option<ControlFlowSideEffects> {
        self.request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == handle_id)
            .map(ControlFlowSideEffects::from_builtin)
    }
}

pub(super) fn function_side_effects(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, ControlFlowSideEffects>, OptimizerError> {
    let functions = function_definitions(request);
    let mut side_effects = functions
        .keys()
        .map(|name_id| {
            (
                *name_id,
                ControlFlowSideEffects {
                    can_continue: false,
                    ..ControlFlowSideEffects::default()
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut progress = true;
    while progress {
        progress = false;
        for (name_id, body_block_id) in &functions {
            if !side_effects[name_id].can_continue
                && block_flow(request, *body_block_id, &side_effects, true)?.exits_function
            {
                side_effects
                    .get_mut(name_id)
                    .expect("function exists")
                    .can_continue = true;
                progress = true;
            }
        }
    }

    Ok(side_effects)
}

fn function_definitions(request: &ffi::WireYulOptimizerRequest) -> BTreeMap<u64, u64> {
    let mut functions = BTreeMap::new();
    collect_function_definitions(request, request.root_block_id, &mut functions);
    functions
}

fn collect_function_definitions(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    functions: &mut BTreeMap<u64, u64>,
) {
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        let statement = &request.statements[*statement_id as usize];
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => {
                functions.insert(statement.name_id, statement.body_block_id);
                collect_function_definitions(request, statement.body_block_id, functions);
            }
            STATEMENT_IF => {
                collect_function_definitions(request, statement.body_block_id, functions)
            }
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    collect_function_definitions(
                        request,
                        request.cases[*case_id as usize].body_block_id,
                        functions,
                    );
                }
            }
            STATEMENT_FOR_LOOP => {
                collect_function_definitions(request, statement.pre_block_id, functions);
                collect_function_definitions(request, statement.post_block_id, functions);
                collect_function_definitions(request, statement.body_block_id, functions);
            }
            STATEMENT_BLOCK => collect_function_definitions(request, statement.block_id, functions),
            _ => {}
        }
    }
}

fn block_flow(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    function_side_effects: &BTreeMap<u64, ControlFlowSideEffects>,
    fallthrough_exits_function: bool,
) -> Result<FlowSummary, OptimizerError> {
    let mut flow = FlowSummary::default();
    let mut reachable = true;
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        if !reachable {
            break;
        }
        let statement_flow = statement_flow(request, *statement_id, function_side_effects)?;
        flow.exits_function |= statement_flow.exits_function;
        flow.breaks |= statement_flow.breaks;
        flow.continues |= statement_flow.continues;
        reachable = statement_flow.falls_through;
    }

    flow.falls_through = reachable;
    if reachable && fallthrough_exits_function {
        flow.exits_function = true;
    }
    Ok(flow)
}

fn statement_flow(
    request: &ffi::WireYulOptimizerRequest,
    statement_id: u64,
    function_side_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<FlowSummary, OptimizerError> {
    let statement = &request.statements[statement_id as usize];
    match statement.kind {
        STATEMENT_EXPRESSION => {
            flow_for_expression(request, statement.expression_id, function_side_effects)
        }
        STATEMENT_ASSIGNMENT => flow_for_expression(
            request,
            statement.value_expression_id,
            function_side_effects,
        ),
        STATEMENT_VARIABLE_DECLARATION if statement.has_value => flow_for_expression(
            request,
            statement.value_expression_id,
            function_side_effects,
        ),
        STATEMENT_VARIABLE_DECLARATION | STATEMENT_FUNCTION_DEFINITION => {
            Ok(FlowSummary::falls_through())
        }
        STATEMENT_IF => {
            if !expression_can_continue_in_analysis(
                request,
                statement.condition_expression_id,
                function_side_effects,
            )? {
                return Ok(FlowSummary::default());
            }

            let body_flow = block_flow(
                request,
                statement.body_block_id,
                function_side_effects,
                false,
            )?;
            let mut flow = body_flow;
            flow.falls_through = true;
            Ok(flow)
        }
        STATEMENT_SWITCH => {
            if !expression_can_continue_in_analysis(
                request,
                statement.switch_expression_id,
                function_side_effects,
            )? {
                return Ok(FlowSummary::default());
            }

            let has_default = statement
                .case_ids
                .last()
                .is_some_and(|case_id| !request.cases[*case_id as usize].has_value);

            let mut flow = FlowSummary {
                falls_through: !has_default,
                ..FlowSummary::default()
            };
            for case_id in &statement.case_ids {
                flow.add_alternative(block_flow(
                    request,
                    request.cases[*case_id as usize].body_block_id,
                    function_side_effects,
                    false,
                )?);
            }
            Ok(flow)
        }
        STATEMENT_FOR_LOOP => {
            let pre_flow = block_flow(
                request,
                statement.pre_block_id,
                function_side_effects,
                false,
            )?;
            let mut flow = FlowSummary {
                exits_function: pre_flow.exits_function,
                breaks: pre_flow.breaks,
                continues: pre_flow.continues,
                ..FlowSummary::default()
            };
            if !pre_flow.falls_through {
                return Ok(flow);
            }
            if !expression_can_continue_in_analysis(
                request,
                statement.condition_expression_id,
                function_side_effects,
            )? {
                return Ok(flow);
            }

            let body_flow = block_flow(
                request,
                statement.body_block_id,
                function_side_effects,
                false,
            )?;
            flow.exits_function |= body_flow.exits_function;

            if body_flow.falls_through || body_flow.continues {
                let post_flow = block_flow(
                    request,
                    statement.post_block_id,
                    function_side_effects,
                    false,
                )?;
                flow.exits_function |= post_flow.exits_function;
            }

            flow.falls_through = true;
            Ok(flow)
        }
        STATEMENT_BREAK => Ok(FlowSummary::breaks()),
        STATEMENT_CONTINUE => Ok(FlowSummary::continues()),
        STATEMENT_LEAVE => Ok(FlowSummary::exits_function()),
        STATEMENT_BLOCK => block_flow(request, statement.block_id, function_side_effects, false),
        _ => Ok(FlowSummary::falls_through()),
    }
}

fn flow_for_expression(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    function_side_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<FlowSummary, OptimizerError> {
    if expression_can_continue_in_analysis(request, expression_id, function_side_effects)? {
        Ok(FlowSummary::falls_through())
    } else {
        Ok(FlowSummary::default())
    }
}

fn expression_can_continue_in_analysis(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    function_side_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    Ok(!contains_non_continuing_function_call_in_analysis(
        request,
        expression_id,
        function_side_effects,
    )?)
}

fn contains_non_continuing_function_call_in_analysis(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
    function_side_effects: &BTreeMap<u64, ControlFlowSideEffects>,
) -> Result<bool, OptimizerError> {
    let expression = &request.expressions[expression_id as usize];
    if expression.kind != EXPRESSION_FUNCTION_CALL {
        return Ok(false);
    }

    for argument_id in &expression.argument_expression_ids {
        if contains_non_continuing_function_call_in_analysis(
            request,
            *argument_id,
            function_side_effects,
        )? {
            return Ok(true);
        }
    }

    match expression.function_name_kind {
        FUNCTION_NAME_BUILTIN => Ok(request
            .builtins
            .iter()
            .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
            .is_some_and(|builtin| !builtin.control_flow_can_continue)),
        FUNCTION_NAME_IDENTIFIER => Ok(function_side_effects
            .get(&expression.function_name_name_id)
            .is_some_and(|side_effects| !side_effects.can_continue)),
        _ => Err(OptimizerError::InvalidWire(format!(
            "invalid function name kind: {}",
            expression.function_name_kind
        ))),
    }
}

fn zero_literal_expression() -> ffi::WireExpression {
    ffi::WireExpression {
        kind: EXPRESSION_LITERAL,
        debug_data_id: FRESH_DEBUG_DATA_ID,
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
