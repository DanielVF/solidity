use crate::bridge::ffi;
use crate::passes::function_grouper;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER,
    STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FunctionHandle {
    Name(u64),
    Builtin(u64),
}

#[derive(Debug)]
struct CallGraph {
    function_calls: BTreeMap<FunctionHandle, Vec<FunctionHandle>>,
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    prune_block(request, request.root_block_id)?;
    function_grouper::group_functions_in_block(request, request.root_block_id);
    Ok(())
}

fn prune_block(
    request: &mut ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<(), OptimizerError> {
    let call_graph = call_graph(request, block_id)?;
    let functions_to_keep = functions_called_from_outermost_context(request, &call_graph);
    let statement_ids = request.blocks[block_id as usize].statement_ids.clone();
    let mut retained_statement_ids = Vec::with_capacity(statement_ids.len());

    for statement_id in statement_ids {
        let statement = &request.statements[statement_id as usize];
        if statement.kind == STATEMENT_FUNCTION_DEFINITION
            && !functions_to_keep.contains(&statement.name_id)
        {
            continue;
        }

        if statement.kind == STATEMENT_BLOCK
            && request.blocks[statement.block_id as usize]
                .statement_ids
                .is_empty()
        {
            continue;
        }

        retained_statement_ids.push(statement_id);
    }

    request.blocks[block_id as usize].statement_ids = retained_statement_ids;
    Ok(())
}

fn functions_called_from_outermost_context(
    request: &ffi::WireYulOptimizerRequest,
    call_graph: &CallGraph,
) -> BTreeSet<u64> {
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();

    let outermost = FunctionHandle::Name(empty_string_id(request));
    visited.insert(outermost);
    queue.push_back(outermost);

    for reserved_identifier_id in &request.reserved_identifier_ids {
        let handle = FunctionHandle::Name(*reserved_identifier_id);
        if visited.insert(handle) {
            queue.push_back(handle);
        }
    }

    while let Some(function) = queue.pop_front() {
        if let Some(callees) = call_graph.function_calls.get(&function) {
            for callee in callees {
                if matches!(callee, FunctionHandle::Name(_))
                    && call_graph.function_calls.contains_key(callee)
                    && visited.insert(*callee)
                {
                    queue.push_back(*callee);
                }
            }
        }
    }

    visited
        .into_iter()
        .filter_map(|handle| match handle {
            FunctionHandle::Name(name_id) => Some(name_id),
            FunctionHandle::Builtin(_) => None,
        })
        .collect()
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
            let callee = match expression.function_name_kind {
                FUNCTION_NAME_IDENTIFIER => FunctionHandle::Name(expression.function_name_name_id),
                FUNCTION_NAME_BUILTIN => {
                    FunctionHandle::Builtin(expression.function_name_builtin_handle)
                }
                _ => {
                    return Err(OptimizerError::InvalidWire(format!(
                        "invalid function name kind: {}",
                        expression.function_name_kind
                    )));
                }
            };
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

fn empty_string_id(request: &ffi::WireYulOptimizerRequest) -> u64 {
    request
        .strings
        .iter()
        .position(|string| string.bytes.is_empty())
        .unwrap_or(0) as u64
}
