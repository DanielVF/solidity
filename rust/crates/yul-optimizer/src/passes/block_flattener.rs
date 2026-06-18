use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, STATEMENT_BLOCK, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION,
    STATEMENT_IF, STATEMENT_SWITCH,
};

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let root_statement_ids = request.blocks[request.root_block_id as usize]
        .statement_ids
        .clone();

    for statement_id in root_statement_ids {
        let statement = &request.statements[statement_id as usize];
        match statement.kind {
            STATEMENT_BLOCK => flatten_block(request, statement.block_id)?,
            STATEMENT_FUNCTION_DEFINITION => flatten_block(request, statement.body_block_id)?,
            _ => {
                return Err(OptimizerError::InvalidWire(
                    "BlockFlattener requires the FunctionGrouper.".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn flatten_block(
    request: &mut ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<(), OptimizerError> {
    let statement_ids = request.blocks[block_id as usize].statement_ids.clone();

    for statement_id in &statement_ids {
        visit_statement(request, *statement_id)?;
    }

    let mut flattened_statement_ids = Vec::with_capacity(statement_ids.len());
    for statement_id in statement_ids {
        let statement = &request.statements[statement_id as usize];
        if statement.kind == STATEMENT_BLOCK {
            flattened_statement_ids.extend(
                request.blocks[statement.block_id as usize]
                    .statement_ids
                    .iter()
                    .copied(),
            );
        } else {
            flattened_statement_ids.push(statement_id);
        }
    }
    request.blocks[block_id as usize].statement_ids = flattened_statement_ids;

    Ok(())
}

fn visit_statement(
    request: &mut ffi::WireYulOptimizerRequest,
    statement_id: u64,
) -> Result<(), OptimizerError> {
    let statement = request.statements[statement_id as usize].clone();

    match statement.kind {
        STATEMENT_BLOCK => flatten_block(request, statement.block_id),
        STATEMENT_FUNCTION_DEFINITION => flatten_block(request, statement.body_block_id),
        STATEMENT_IF => flatten_block(request, statement.body_block_id),
        STATEMENT_SWITCH => {
            for case_id in statement.case_ids {
                flatten_block(request, request.cases[case_id as usize].body_block_id)?;
            }
            Ok(())
        }
        STATEMENT_FOR_LOOP => {
            flatten_block(request, statement.pre_block_id)?;
            flatten_block(request, statement.post_block_id)?;
            flatten_block(request, statement.body_block_id)
        }
        _ => Ok(()),
    }
}
