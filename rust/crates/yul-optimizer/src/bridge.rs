#[cxx::bridge(namespace = "solidity::yul::rust")]
pub mod ffi {
    #[derive(Clone, Debug)]
    struct WireString {
        bytes: Vec<u8>,
    }

    #[derive(Clone, Debug)]
    struct WireYulOptimizerSettings {
        optimize_stack_allocation: bool,
        optimization_sequence_id: u64,
        cleanup_sequence_id: u64,
        has_expected_executions_per_deployment: bool,
        expected_executions_per_deployment: u64,
        creation: bool,
    }

    #[derive(Clone, Debug)]
    struct WireDialect {
        evm_version: u16,
        provides_object_access: bool,
        can_overcharge_gas_for_call: bool,
    }

    #[derive(Clone, Debug)]
    struct WireSpecialHandles {
        has_discard: bool,
        discard: u64,
        has_equality: bool,
        equality: u64,
        has_boolean_negation: bool,
        boolean_negation: u64,
        has_memory_store: bool,
        memory_store: u64,
        has_memory_load: bool,
        memory_load: u64,
        has_storage_store: bool,
        storage_store: u64,
        has_storage_load: bool,
        storage_load: u64,
        has_hash: bool,
        hash: u64,
        has_add: bool,
        add: u64,
        has_exp: bool,
        exp: u64,
        has_mul: bool,
        mul: u64,
        has_not: bool,
        not_: u64,
        has_shl: bool,
        shl: u64,
        has_sub: bool,
        sub: u64,
        has_memoryguard: bool,
        memoryguard: u64,
    }

    #[derive(Clone, Debug)]
    struct WireBuiltinFunction {
        handle_id: u64,
        name_id: u64,
        num_parameters: u64,
        num_returns: u64,
        movable: bool,
        movable_apart_from_effects: bool,
        can_be_removed: bool,
        can_be_removed_if_no_msize: bool,
        cannot_loop: bool,
        other_state: u8,
        storage: u8,
        memory: u8,
        transient_storage: u8,
        control_flow_can_terminate: bool,
        control_flow_can_revert: bool,
        control_flow_can_continue: bool,
        is_msize: bool,
        has_evm_opcode: bool,
        evm_opcode: u16,
        literal_argument_kinds: Vec<u8>,
    }

    #[derive(Clone, Debug)]
    struct WireObjectContext {
        object_name_id: u64,
        object_paths: Vec<u64>,
        data_paths: Vec<u64>,
    }

    #[derive(Clone, Debug)]
    struct WireNameWithDebugData {
        debug_data_id: u64,
        name_id: u64,
    }

    #[derive(Clone, Debug)]
    struct WireIdentifier {
        debug_data_id: u64,
        name_id: u64,
    }

    #[derive(Clone, Debug)]
    struct WireBlock {
        debug_data_id: u64,
        statement_ids: Vec<u64>,
    }

    #[derive(Clone, Debug)]
    struct WireStatement {
        kind: u8,
        debug_data_id: u64,
        expression_id: u64,
        has_value: bool,
        value_expression_id: u64,
        variable_ids: Vec<u64>,
        name_id: u64,
        parameter_ids: Vec<u64>,
        return_variable_ids: Vec<u64>,
        body_block_id: u64,
        pre_block_id: u64,
        post_block_id: u64,
        condition_expression_id: u64,
        switch_expression_id: u64,
        case_ids: Vec<u64>,
        block_id: u64,
    }

    #[derive(Clone, Debug)]
    struct WireExpression {
        kind: u8,
        debug_data_id: u64,
        literal_kind: u8,
        literal_unlimited: bool,
        literal_value: Vec<u8>,
        literal_string_id: u64,
        has_literal_hint: bool,
        literal_hint_id: u64,
        name_id: u64,
        function_name_kind: u8,
        function_name_debug_data_id: u64,
        function_name_name_id: u64,
        function_name_builtin_handle: u64,
        argument_expression_ids: Vec<u64>,
    }

    #[derive(Clone, Debug)]
    struct WireCase {
        debug_data_id: u64,
        has_value: bool,
        value_expression_id: u64,
        body_block_id: u64,
    }

    #[derive(Clone, Debug)]
    struct WireYulOptimizerRequest {
        settings: WireYulOptimizerSettings,
        dialect: WireDialect,
        special_handles: WireSpecialHandles,
        builtins: Vec<WireBuiltinFunction>,
        object_context: WireObjectContext,
        strings: Vec<WireString>,
        reserved_identifier_ids: Vec<u64>,
        names: Vec<WireNameWithDebugData>,
        identifiers: Vec<WireIdentifier>,
        blocks: Vec<WireBlock>,
        statements: Vec<WireStatement>,
        expressions: Vec<WireExpression>,
        cases: Vec<WireCase>,
        root_block_id: u64,
    }

    #[derive(Clone, Debug)]
    struct OptimizerResult {
        ok: bool,
        error_code: u8,
        error_message: String,
        strings: Vec<WireString>,
        names: Vec<WireNameWithDebugData>,
        identifiers: Vec<WireIdentifier>,
        blocks: Vec<WireBlock>,
        statements: Vec<WireStatement>,
        expressions: Vec<WireExpression>,
        cases: Vec<WireCase>,
        root_block_id: u64,
    }

    extern "Rust" {
        fn optimize_yul(request: WireYulOptimizerRequest) -> OptimizerResult;
    }
}

pub fn optimize_yul(request: ffi::WireYulOptimizerRequest) -> ffi::OptimizerResult {
    match std::panic::catch_unwind(|| crate::optimizer::optimize_yul(request)) {
        Ok(result) => result,
        Err(_) => ffi::OptimizerResult {
            ok: false,
            error_code: crate::wire::OPTIMIZER_ERROR_PANIC,
            error_message: "Rust Yul optimizer panicked.".to_string(),
            strings: Vec::new(),
            names: Vec::new(),
            identifiers: Vec::new(),
            blocks: Vec::new(),
            statements: Vec::new(),
            expressions: Vec::new(),
            cases: Vec::new(),
            root_block_id: 0,
        },
    }
}
