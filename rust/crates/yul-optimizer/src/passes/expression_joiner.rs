use crate::bridge::ffi;
use crate::passes::function_grouper;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::BTreeMap;

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let references = count_variable_references(request)?;
    let mut joiner = ExpressionJoiner {
        request,
        references,
        current_block: None,
        latest_statement_in_block: usize::MAX,
    };
    joiner.visit_block(joiner.request.root_block_id)?;
    function_grouper::group_functions_in_block(joiner.request, joiner.request.root_block_id);
    Ok(())
}

struct ExpressionJoiner<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    references: BTreeMap<u64, usize>,
    current_block: Option<u64>,
    latest_statement_in_block: usize,
}

impl ExpressionJoiner<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        self.reset_latest_statement_pointer();

        let len = self.request.blocks[block_id as usize].statement_ids.len();
        for index in 0..len {
            let statement_id = self.request.blocks[block_id as usize].statement_ids[index];
            self.visit_statement(statement_id)?;
            self.current_block = Some(block_id);
            self.latest_statement_in_block = index;
        }

        self.remove_empty_blocks(block_id);
        self.reset_latest_statement_pointer();
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            crate::wire::STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
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

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_IDENTIFIER {
            if self.is_latest_statement_var_decl_joinable(expression.name_id) {
                let latest_statement_id = self.latest_statement_id().expect("checked joinable");
                let value_expression_id =
                    self.request.statements[latest_statement_id as usize].value_expression_id;
                self.request.expressions[expression_id as usize] =
                    self.request.expressions[value_expression_id as usize].clone();
                self.replace_statement_with_empty_block(latest_statement_id);
                self.decrement_latest_statement_pointer();
            }
        } else if expression.kind == EXPRESSION_FUNCTION_CALL {
            self.handle_arguments(&expression.argument_expression_ids)?;
        }
        Ok(())
    }

    fn handle_arguments(&mut self, arguments: &[u64]) -> Result<(), OptimizerError> {
        let mut index = arguments.len();
        for argument_id in arguments.iter().rev() {
            index -= 1;
            let argument = &self.request.expressions[*argument_id as usize];
            if argument.kind != EXPRESSION_IDENTIFIER && argument.kind != EXPRESSION_LITERAL {
                break;
            }
        }

        for argument_id in &arguments[index..] {
            self.visit_expression(*argument_id)?;
        }
        Ok(())
    }

    fn is_latest_statement_var_decl_joinable(&self, identifier_name: u64) -> bool {
        let Some(statement_id) = self.latest_statement_id() else {
            return false;
        };
        let statement = &self.request.statements[statement_id as usize];
        if statement.kind != STATEMENT_VARIABLE_DECLARATION
            || statement.variable_ids.len() != 1
            || !statement.has_value
        {
            return false;
        }

        self.request.names[statement.variable_ids[0] as usize].name_id == identifier_name
            && self.references.get(&identifier_name).copied().unwrap_or(0) == 1
    }

    fn latest_statement_id(&self) -> Option<u64> {
        let block_id = self.current_block?;
        self.request.blocks[block_id as usize]
            .statement_ids
            .get(self.latest_statement_in_block)
            .copied()
    }

    fn decrement_latest_statement_pointer(&mut self) {
        if self.current_block.is_none() {
            return;
        }
        if self.latest_statement_in_block > 0 {
            self.latest_statement_in_block -= 1;
        } else {
            self.reset_latest_statement_pointer();
        }
    }

    fn reset_latest_statement_pointer(&mut self) {
        self.current_block = None;
        self.latest_statement_in_block = usize::MAX;
    }

    fn replace_statement_with_empty_block(&mut self, statement_id: u64) {
        let debug_data_id = self.request.statements[statement_id as usize].debug_data_id;
        let block_id = self.request.blocks.len() as u64;
        self.request.blocks.push(ffi::WireBlock {
            debug_data_id,
            statement_ids: Vec::new(),
        });

        self.request.statements[statement_id as usize] = ffi::WireStatement {
            kind: STATEMENT_BLOCK,
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
            block_id,
        };
    }

    fn remove_empty_blocks(&mut self, block_id: u64) {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        self.request.blocks[block_id as usize].statement_ids = statement_ids
            .into_iter()
            .filter(|statement_id| {
                let statement = &self.request.statements[*statement_id as usize];
                statement.kind != STATEMENT_BLOCK
                    || !self.request.blocks[statement.block_id as usize]
                        .statement_ids
                        .is_empty()
            })
            .collect();
    }
}

fn count_variable_references(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, usize>, OptimizerError> {
    let mut counter = VariableReferenceCounter {
        request,
        references: BTreeMap::new(),
    };
    counter.visit_block(request.root_block_id)?;
    Ok(counter.references)
}

struct VariableReferenceCounter<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    references: BTreeMap<u64, usize>,
}

impl VariableReferenceCounter<'_> {
    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        for statement_id in &self.request.blocks[block_id as usize].statement_ids {
            self.visit_statement(*statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = &self.request.statements[statement_id as usize];
        match statement.kind {
            crate::wire::STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => {
                for variable_id in &statement.variable_ids {
                    let name_id = self.request.identifiers[*variable_id as usize].name_id;
                    *self.references.entry(name_id).or_default() += 1;
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
        match expression.kind {
            EXPRESSION_IDENTIFIER => {
                *self.references.entry(expression.name_id).or_default() += 1;
                Ok(())
            }
            EXPRESSION_FUNCTION_CALL => {
                for argument_id in expression.argument_expression_ids.iter().rev() {
                    self.visit_expression(*argument_id)?;
                }
                Ok(())
            }
            EXPRESSION_LITERAL => Ok(()),
            _ => Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            ))),
        }
    }
}
