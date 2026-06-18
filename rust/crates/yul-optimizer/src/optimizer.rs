use crate::bridge::ffi;
use crate::wire::{self, OptimizerError};

pub fn optimize_yul(request: ffi::WireYulOptimizerRequest) -> ffi::OptimizerResult {
    match optimize_request(request) {
        Ok(result) => result,
        Err(error) => error_result(error),
    }
}

fn optimize_request(
    mut request: ffi::WireYulOptimizerRequest,
) -> Result<ffi::OptimizerResult, OptimizerError> {
    wire::validate_request(&request)?;
    crate::passes::run_pipeline(&mut request)?;

    Ok(ffi::OptimizerResult {
        ok: true,
        error_code: 0,
        error_message: String::new(),
        strings: request.strings,
        names: request.names,
        identifiers: request.identifiers,
        blocks: request.blocks,
        statements: request.statements,
        expressions: request.expressions,
        cases: request.cases,
        root_block_id: request.root_block_id,
    })
}

fn error_result(error: OptimizerError) -> ffi::OptimizerResult {
    ffi::OptimizerResult {
        ok: false,
        error_code: error.code(),
        error_message: error.to_string(),
        strings: Vec::new(),
        names: Vec::new(),
        identifiers: Vec::new(),
        blocks: Vec::new(),
        statements: Vec::new(),
        expressions: Vec::new(),
        cases: Vec::new(),
        root_block_id: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_request() -> ffi::WireYulOptimizerRequest {
        ffi::WireYulOptimizerRequest {
            settings: ffi::WireYulOptimizerSettings {
                optimize_stack_allocation: false,
                optimization_sequence_id: 0,
                cleanup_sequence_id: 0,
                has_expected_executions_per_deployment: false,
                expected_executions_per_deployment: 0,
                creation: true,
            },
            dialect: ffi::WireDialect {
                evm_version: 0,
                provides_object_access: false,
                can_overcharge_gas_for_call: false,
            },
            special_handles: ffi::WireSpecialHandles {
                has_discard: false,
                discard: 0,
                has_equality: false,
                equality: 0,
                has_boolean_negation: false,
                boolean_negation: 0,
                has_memory_store: false,
                memory_store: 0,
                has_memory_load: false,
                memory_load: 0,
                has_storage_store: false,
                storage_store: 0,
                has_storage_load: false,
                storage_load: 0,
                has_hash: false,
                hash: 0,
                has_add: false,
                add: 0,
                has_exp: false,
                exp: 0,
                has_mul: false,
                mul: 0,
                has_not: false,
                not_: 0,
                has_shl: false,
                shl: 0,
                has_sub: false,
                sub: 0,
                has_memoryguard: false,
                memoryguard: 0,
            },
            builtins: Vec::new(),
            object_context: ffi::WireObjectContext {
                object_name_id: 0,
                object_paths: Vec::new(),
                data_paths: Vec::new(),
            },
            strings: vec![ffi::WireString { bytes: Vec::new() }],
            reserved_identifier_ids: Vec::new(),
            names: Vec::new(),
            identifiers: Vec::new(),
            blocks: vec![ffi::WireBlock {
                debug_data_id: 0,
                statement_ids: Vec::new(),
            }],
            statements: Vec::new(),
            expressions: Vec::new(),
            cases: Vec::new(),
            root_block_id: 0,
        }
    }

    #[test]
    fn no_op_returns_grouped_empty_block() {
        let result = optimize_yul(empty_request());

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(result.root_block_id, 0);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.statements.len(), 1);
        assert_eq!(result.blocks[0].statement_ids, vec![0]);
        assert_eq!(result.statements[0].kind, wire::STATEMENT_BLOCK);
        assert_eq!(result.statements[0].block_id, 1);
        assert!(result.blocks[1].statement_ids.is_empty());
    }

    #[test]
    fn rejects_invalid_root() {
        let mut request = empty_request();
        request.root_block_id = 1;

        let result = optimize_yul(request);

        assert!(!result.ok);
        assert_eq!(result.error_code, 1);
    }
}
