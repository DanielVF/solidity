use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlFlow {
    FlowOut,
    Break,
    Continue,
    Terminate,
    Leave,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    ControlFlowSimplifier {
        request,
        num_break_statements: 0,
        num_continue_statements: 0,
    }
    .visit_block_root()
}

struct ControlFlowSimplifier<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    num_break_statements: usize,
    num_continue_statements: usize,
}

impl ControlFlowSimplifier<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        self.request.blocks[block_id as usize].statement_ids =
            self.simplify_statement_ids(statement_ids)?;
        Ok(())
    }

    fn simplify_statement_ids(
        &mut self,
        statement_ids: Vec<u64>,
    ) -> Result<Vec<u64>, OptimizerError> {
        let mut output = Vec::with_capacity(statement_ids.len());

        for statement_id in statement_ids {
            if let Some(replacement) = self.replacement_for_statement(statement_id)? {
                output.extend(self.simplify_statement_ids(replacement)?);
            } else {
                self.visit_statement(statement_id)?;
                output.push(statement_id);
            }
        }

        Ok(output)
    }

    fn replacement_for_statement(
        &mut self,
        statement_id: u64,
    ) -> Result<Option<Vec<u64>>, OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_IF => {
                if self.request.blocks[statement.body_block_id as usize]
                    .statement_ids
                    .is_empty()
                {
                    if let Some(discard) = self.discard_handle() {
                        return Ok(Some(vec![self.new_discard_statement(
                            statement.debug_data_id,
                            discard,
                            statement.condition_expression_id,
                        )]));
                    }
                }
                Ok(None)
            }
            STATEMENT_SWITCH => {
                self.remove_empty_default_from_switch(statement_id);
                self.remove_empty_cases_from_switch(statement_id);

                let statement = self.request.statements[statement_id as usize].clone();
                if statement.case_ids.is_empty() {
                    self.reduce_no_case_switch(&statement)
                } else if statement.case_ids.len() == 1 {
                    self.reduce_single_case_switch(&statement)
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
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
                self.visit_block(statement.body_block_id)?;
                if self
                    .request
                    .blocks
                    .get(statement.body_block_id as usize)
                    .and_then(|block| block.statement_ids.last())
                    .is_some_and(|last_statement_id| {
                        self.request.statements[*last_statement_id as usize].kind == STATEMENT_LEAVE
                    })
                {
                    self.request.blocks[statement.body_block_id as usize]
                        .statement_ids
                        .pop();
                }
                Ok(())
            }
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
            STATEMENT_FOR_LOOP => self.visit_for_loop(statement_id),
            STATEMENT_BREAK => {
                self.num_break_statements += 1;
                Ok(())
            }
            STATEMENT_CONTINUE => {
                self.num_continue_statements += 1;
                Ok(())
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_for_loop(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        if !self.request.blocks[statement.pre_block_id as usize]
            .statement_ids
            .is_empty()
        {
            return Err(OptimizerError::InvalidWire(
                "ControlFlowSimplifier requires ForLoopInitRewriter.".to_string(),
            ));
        }

        let outer_break = self.num_break_statements;
        let outer_continue = self.num_continue_statements;
        self.num_break_statements = 0;
        self.num_continue_statements = 0;

        self.visit_block(statement.pre_block_id)?;
        self.visit_expression(statement.condition_expression_id)?;
        self.visit_block(statement.post_block_id)?;
        self.visit_block(statement.body_block_id)?;

        if let Some(last_statement_id) = self.request.blocks[statement.body_block_id as usize]
            .statement_ids
            .last()
            .copied()
        {
            let mut is_terminating = false;
            let control_flow = self.control_flow_kind(last_statement_id)?;
            if control_flow == ControlFlow::Break {
                is_terminating = true;
                self.num_break_statements = self.num_break_statements.saturating_sub(1);
            } else if matches!(control_flow, ControlFlow::Terminate | ControlFlow::Leave) {
                is_terminating = true;
            }

            if is_terminating && self.num_continue_statements == 0 && self.num_break_statements == 0
            {
                if control_flow == ControlFlow::Break {
                    self.request.blocks[statement.body_block_id as usize]
                        .statement_ids
                        .pop();
                }

                let replacement = &mut self.request.statements[statement_id as usize];
                replacement.kind = STATEMENT_IF;
                replacement.condition_expression_id = statement.condition_expression_id;
                replacement.body_block_id = statement.body_block_id;
            }
        }

        self.num_break_statements = outer_break;
        self.num_continue_statements = outer_continue;
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

    fn reduce_no_case_switch(
        &mut self,
        switch_statement: &ffi::WireStatement,
    ) -> Result<Option<Vec<u64>>, OptimizerError> {
        let Some(discard) = self.discard_handle() else {
            return Ok(None);
        };

        let debug_data_id =
            self.request.expressions[switch_statement.switch_expression_id as usize].debug_data_id;
        Ok(Some(vec![self.new_discard_statement(
            debug_data_id,
            discard,
            switch_statement.switch_expression_id,
        )]))
    }

    fn reduce_single_case_switch(
        &mut self,
        switch_statement: &ffi::WireStatement,
    ) -> Result<Option<Vec<u64>>, OptimizerError> {
        let switch_case = self.request.cases[switch_statement.case_ids[0] as usize].clone();
        let debug_data_id =
            self.request.expressions[switch_statement.switch_expression_id as usize].debug_data_id;

        if switch_case.has_value {
            let Some(equality) = self.equality_handle() else {
                return Ok(None);
            };

            let condition_expression_id = self.new_builtin_call_expression(
                debug_data_id,
                equality,
                vec![
                    switch_case.value_expression_id,
                    switch_statement.switch_expression_id,
                ],
            );
            let if_statement_id = self.request.statements.len() as u64;
            let mut if_statement = empty_statement(STATEMENT_IF, switch_statement.debug_data_id);
            if_statement.condition_expression_id = condition_expression_id;
            if_statement.body_block_id = switch_case.body_block_id;
            self.request.statements.push(if_statement);
            Ok(Some(vec![if_statement_id]))
        } else {
            let Some(discard) = self.discard_handle() else {
                return Ok(None);
            };

            let discard_statement_id = self.new_discard_statement(
                debug_data_id,
                discard,
                switch_statement.switch_expression_id,
            );
            let block_statement_id = self.request.statements.len() as u64;
            let mut block_statement = empty_statement(
                STATEMENT_BLOCK,
                self.request.blocks[switch_case.body_block_id as usize].debug_data_id,
            );
            block_statement.block_id = switch_case.body_block_id;
            self.request.statements.push(block_statement);
            Ok(Some(vec![discard_statement_id, block_statement_id]))
        }
    }

    fn remove_empty_default_from_switch(&mut self, statement_id: u64) {
        let case_ids = self.request.statements[statement_id as usize]
            .case_ids
            .clone();
        self.request.statements[statement_id as usize].case_ids = case_ids
            .into_iter()
            .filter(|case_id| {
                let switch_case = &self.request.cases[*case_id as usize];
                switch_case.has_value
                    || !self.request.blocks[switch_case.body_block_id as usize]
                        .statement_ids
                        .is_empty()
            })
            .collect();
    }

    fn remove_empty_cases_from_switch(&mut self, statement_id: u64) {
        let case_ids = self.request.statements[statement_id as usize]
            .case_ids
            .clone();
        let has_default = case_ids
            .iter()
            .any(|case_id| !self.request.cases[*case_id as usize].has_value);
        if has_default {
            return;
        }

        self.request.statements[statement_id as usize].case_ids = case_ids
            .into_iter()
            .filter(|case_id| {
                !self.request.blocks[self.request.cases[*case_id as usize].body_block_id as usize]
                    .statement_ids
                    .is_empty()
            })
            .collect();
    }

    fn control_flow_kind(&self, statement_id: u64) -> Result<ControlFlow, OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_VARIABLE_DECLARATION if statement.has_value => {
                if self.contains_non_continuing_builtin_call(statement.value_expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_ASSIGNMENT => {
                if self.contains_non_continuing_builtin_call(statement.value_expression_id)? {
                    Ok(ControlFlow::Terminate)
                } else {
                    Ok(ControlFlow::FlowOut)
                }
            }
            STATEMENT_EXPRESSION => {
                if self.contains_non_continuing_builtin_call(statement.expression_id)? {
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

    fn contains_non_continuing_builtin_call(
        &self,
        expression_id: u64,
    ) -> Result<bool, OptimizerError> {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_FUNCTION_CALL {
            return Ok(false);
        }

        for argument_id in &expression.argument_expression_ids {
            if self.contains_non_continuing_builtin_call(*argument_id)? {
                return Ok(true);
            }
        }

        match expression.function_name_kind {
            FUNCTION_NAME_BUILTIN => Ok(self
                .request
                .builtins
                .iter()
                .find(|builtin| builtin.handle_id == expression.function_name_builtin_handle)
                .is_some_and(|builtin| !builtin.control_flow_can_continue)),
            FUNCTION_NAME_IDENTIFIER => Ok(false),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid function name kind: {}",
                expression.function_name_kind
            ))),
        }
    }

    fn new_discard_statement(
        &mut self,
        debug_data_id: u64,
        discard_handle: u64,
        argument_expression_id: u64,
    ) -> u64 {
        let expression_id = self.new_builtin_call_expression(
            debug_data_id,
            discard_handle,
            vec![argument_expression_id],
        );
        let statement_id = self.request.statements.len() as u64;
        let mut statement = empty_statement(STATEMENT_EXPRESSION, debug_data_id);
        statement.expression_id = expression_id;
        self.request.statements.push(statement);
        statement_id
    }

    fn new_builtin_call_expression(
        &mut self,
        debug_data_id: u64,
        builtin_handle: u64,
        argument_expression_ids: Vec<u64>,
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
            function_name_builtin_handle: builtin_handle,
            argument_expression_ids,
        });
        expression_id
    }

    fn discard_handle(&self) -> Option<u64> {
        self.request
            .special_handles
            .has_discard
            .then_some(self.request.special_handles.discard)
    }

    fn equality_handle(&self) -> Option<u64> {
        self.request
            .special_handles
            .has_equality
            .then_some(self.request.special_handles.equality)
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
