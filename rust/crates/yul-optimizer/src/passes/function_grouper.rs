use crate::bridge::ffi;
use crate::wire::OptimizerError;
use crate::wire::{STATEMENT_BLOCK, STATEMENT_FUNCTION_DEFINITION};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    group_functions_in_block(request, request.root_block_id);
    Ok(())
}

pub(crate) fn group_functions_in_block(request: &mut ffi::WireYulOptimizerRequest, block_id: u64) {
    let block_index = block_id as usize;
    let original_statement_ids = request.blocks[block_index].statement_ids.clone();

    if already_grouped(request, &original_statement_ids) {
        return;
    }

    let debug_data_id = request.blocks[block_index].debug_data_id;
    let mut grouped_statement_ids = Vec::with_capacity(original_statement_ids.len() + 1);
    let mut function_statement_ids = Vec::new();

    for statement_id in original_statement_ids {
        if request.statements[statement_id as usize].kind == STATEMENT_FUNCTION_DEFINITION {
            function_statement_ids.push(statement_id);
        } else {
            grouped_statement_ids.push(statement_id);
        }
    }

    let grouped_block_id = request.blocks.len() as u64;
    request.blocks.push(ffi::WireBlock {
        debug_data_id,
        statement_ids: grouped_statement_ids,
    });

    let grouped_block_statement_id = request.statements.len() as u64;
    let mut grouped_block_statement = empty_statement(STATEMENT_BLOCK, debug_data_id);
    grouped_block_statement.block_id = grouped_block_id;
    request.statements.push(grouped_block_statement);

    let mut reordered_statement_ids = Vec::with_capacity(function_statement_ids.len() + 1);
    reordered_statement_ids.push(grouped_block_statement_id);
    reordered_statement_ids.extend(function_statement_ids);
    request.blocks[block_index].statement_ids = reordered_statement_ids;
}

fn already_grouped(request: &ffi::WireYulOptimizerRequest, statement_ids: &[u64]) -> bool {
    if statement_ids.is_empty() {
        return false;
    }

    if request.statements[statement_ids[0] as usize].kind != STATEMENT_BLOCK {
        return false;
    }

    statement_ids[1..].iter().all(|statement_id| {
        request.statements[*statement_id as usize].kind == STATEMENT_FUNCTION_DEFINITION
    })
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
