#[cxx::bridge(namespace = "solidity::evmasm::rust")]
pub mod ffi {
    #[derive(Clone, Debug)]
    struct WireOptimizerSettings {
        run_inliner: bool,
        run_jumpdest_remover: bool,
        run_peephole: bool,
        run_deduplicate: bool,
        run_cse: bool,
        run_constant_optimiser: bool,
        expected_executions_per_deployment: u64,
    }

    #[derive(Clone, Debug)]
    struct WireEvmVersion {
        value: u16,
    }

    #[derive(Clone, Debug)]
    struct WireAssemblyItem {
        kind: u8,
        opcode: u8,
        data: Vec<u8>,
        verbatim_data: Vec<u8>,
        verbatim_arguments: u64,
        verbatim_return_values: u64,
        jump_type: u8,
        modifier_depth: u64,
        debug_data_id: u64,
        has_pushed_value: bool,
        pushed_value: Vec<u8>,
        has_immutable_occurrences: bool,
        immutable_occurrences: u64,
    }

    #[derive(Clone, Debug)]
    struct WireTagReplacement {
        from: Vec<u8>,
        to: Vec<u8>,
    }

    #[derive(Clone, Debug)]
    struct WireDataEntry {
        hash: Vec<u8>,
        data: Vec<u8>,
    }

    #[derive(Clone, Debug)]
    struct OptimizerResult {
        ok: bool,
        error_code: u8,
        error_message: String,
        optimized_items: Vec<WireAssemblyItem>,
        tag_replacements: Vec<WireTagReplacement>,
        data_entries: Vec<WireDataEntry>,
    }

    extern "Rust" {
        fn optimize_assembly_items(
            items: Vec<WireAssemblyItem>,
            settings: WireOptimizerSettings,
            evm_version: WireEvmVersion,
            creation: bool,
            tags_referenced_from_outside: Vec<u64>,
        ) -> OptimizerResult;
    }
}

pub fn optimize_assembly_items(
    items: Vec<ffi::WireAssemblyItem>,
    settings: ffi::WireOptimizerSettings,
    evm_version: ffi::WireEvmVersion,
    creation: bool,
    tags_referenced_from_outside: Vec<u64>,
) -> ffi::OptimizerResult {
    match std::panic::catch_unwind(|| {
        crate::optimizer::optimize_assembly_items(
            items,
            settings,
            evm_version,
            creation,
            tags_referenced_from_outside,
        )
    }) {
        Ok(result) => result,
        Err(_) => ffi::OptimizerResult {
            ok: false,
            error_code: crate::wire::OPTIMIZER_ERROR_PANIC,
            error_message: "Rust evmasm optimizer panicked.".to_string(),
            optimized_items: Vec::new(),
            tag_replacements: Vec::new(),
            data_entries: Vec::new(),
        },
    }
}
