use crate::bridge::ffi;
use crate::item;
use crate::opcode::EvmVersion;
use crate::passes::{
    block_deduplicator, constant_optimizer, cse, inliner, jumpdest_remover, peephole,
};
use crate::wire::{self, OptimizerError};
use std::collections::BTreeMap;

pub fn optimize_assembly_items(
    mut items: Vec<ffi::WireAssemblyItem>,
    settings: ffi::WireOptimizerSettings,
    evm_version: ffi::WireEvmVersion,
    creation: bool,
    mut tags_referenced_from_outside: Vec<u64>,
) -> ffi::OptimizerResult {
    match validate_inputs(
        &items,
        &settings,
        &evm_version,
        &tags_referenced_from_outside,
    ) {
        Ok(()) => match optimize_items(
            &mut items,
            &settings,
            EvmVersion::new(evm_version.value),
            creation,
            &mut tags_referenced_from_outside,
        ) {
            Ok((tag_replacements, data_entries)) => ffi::OptimizerResult {
                ok: true,
                error_code: 0,
                error_message: String::new(),
                optimized_items: items,
                tag_replacements,
                data_entries,
            },
            Err(error) => error_result(error),
        },
        Err(error) => error_result(error),
    }
}

fn optimize_items(
    items: &mut Vec<ffi::WireAssemblyItem>,
    settings: &ffi::WireOptimizerSettings,
    evm_version: EvmVersion,
    creation: bool,
    tags_referenced_from_outside: &mut Vec<u64>,
) -> Result<(Vec<ffi::WireTagReplacement>, Vec<ffi::WireDataEntry>), OptimizerError> {
    let mut tag_replacements = BTreeMap::<u64, u64>::new();

    loop {
        let mut count = 0u64;

        if settings.run_inliner {
            inliner::optimise(
                items,
                tags_referenced_from_outside,
                settings.expected_executions_per_deployment,
                creation,
                evm_version,
            );
        }

        if settings.run_jumpdest_remover
            && jumpdest_remover::optimise(items, tags_referenced_from_outside)
        {
            count += 1;
        }

        if settings.run_peephole {
            while peephole::optimise(items, evm_version) {
                count += 1;
                if count >= 64000 {
                    return Err(OptimizerError::InvalidState(
                        "Peephole optimizer seems to be stuck.".to_string(),
                    ));
                }
            }
        }

        if settings.run_deduplicate {
            let replacements = block_deduplicator::deduplicate(items);
            if !replacements.is_empty() {
                for (from, to) in replacements {
                    if tag_replacements.contains_key(&from) {
                        return Err(OptimizerError::InvalidTag(
                            "Replacement already known.".to_string(),
                        ));
                    }
                    tag_replacements.insert(from, to);
                    if let Some(position) = tags_referenced_from_outside
                        .iter()
                        .position(|referenced_tag| *referenced_tag == from)
                    {
                        tags_referenced_from_outside.remove(position);
                        tags_referenced_from_outside.push(to);
                    }
                }
                count += 1;
            }
        }

        if settings.run_cse && cse::optimise(items, evm_version)? {
            count += 1;
        }

        if count == 0 {
            break;
        }
    }

    let data_entries = if settings.run_constant_optimiser {
        constant_optimizer::optimise(
            items,
            creation,
            if creation {
                1
            } else {
                settings.expected_executions_per_deployment
            },
            evm_version,
        )
    } else {
        Vec::new()
    };

    Ok((
        tag_replacements
            .into_iter()
            .map(|(from, to)| ffi::WireTagReplacement {
                from: item::u256_from_u64(from),
                to: item::u256_from_u64(to),
            })
            .collect(),
        data_entries,
    ))
}

fn error_result(error: OptimizerError) -> ffi::OptimizerResult {
    ffi::OptimizerResult {
        ok: false,
        error_code: error.code(),
        error_message: error.to_string(),
        optimized_items: Vec::new(),
        tag_replacements: Vec::new(),
        data_entries: Vec::new(),
    }
}

fn validate_inputs(
    items: &[ffi::WireAssemblyItem],
    settings: &ffi::WireOptimizerSettings,
    evm_version: &ffi::WireEvmVersion,
    tags_referenced_from_outside: &[u64],
) -> Result<(), OptimizerError> {
    wire::validate_settings(settings)?;
    wire::validate_evm_version(evm_version)?;
    for item in items {
        wire::validate_assembly_item(item)?;
    }
    for tag in tags_referenced_from_outside {
        if *tag > wire::MAX_TAG_VALUE {
            return Err(OptimizerError::InvalidTag(format!(
                "tag referenced from outside exceeds {}: {}",
                wire::MAX_TAG_VALUE,
                tag
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opcode::op;

    fn push_item(value: u64) -> ffi::WireAssemblyItem {
        ffi::WireAssemblyItem {
            kind: wire::KIND_PUSH,
            opcode: 0,
            data: item::u256_from_u64(value),
            verbatim_data: Vec::new(),
            verbatim_arguments: 0,
            verbatim_return_values: 0,
            jump_type: wire::JUMP_ORDINARY,
            modifier_depth: 0,
            debug_data_id: 1,
            has_pushed_value: false,
            pushed_value: Vec::new(),
            has_immutable_occurrences: false,
            immutable_occurrences: 0,
        }
    }

    fn operation(opcode: u8) -> ffi::WireAssemblyItem {
        item::operation(opcode, 1)
    }

    fn tag(value: u64) -> ffi::WireAssemblyItem {
        item::data_item(wire::KIND_TAG, value, 1)
    }

    fn push_tag(value: u64) -> ffi::WireAssemblyItem {
        item::data_item(wire::KIND_PUSH_TAG, value, 1)
    }

    fn settings() -> ffi::WireOptimizerSettings {
        ffi::WireOptimizerSettings {
            run_inliner: false,
            run_jumpdest_remover: false,
            run_peephole: false,
            run_deduplicate: false,
            run_cse: false,
            run_constant_optimiser: false,
            expected_executions_per_deployment: 1,
        }
    }

    fn optimize_with(
        items: Vec<ffi::WireAssemblyItem>,
        settings: ffi::WireOptimizerSettings,
    ) -> ffi::OptimizerResult {
        optimize_assembly_items(
            items,
            settings,
            ffi::WireEvmVersion { value: 14 },
            false,
            Vec::new(),
        )
    }

    #[test]
    fn no_op_optimizer_returns_items_unchanged() {
        let items = vec![push_item(7), push_item(9)];
        let result = optimize_with(items, settings());

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(result.optimized_items.len(), 2);
        assert_eq!(result.optimized_items[0].data[31], 7);
        assert_eq!(result.optimized_items[1].data[31], 9);
        assert!(result.tag_replacements.is_empty());
    }

    #[test]
    fn rejects_wrong_integer_width() {
        let mut item = push_item(1);
        item.data.pop();

        let result = optimize_with(vec![item], settings());

        assert!(!result.ok);
        assert_eq!(result.error_code, OptimizerError::InvalidWire.code());
    }

    #[test]
    fn jumpdest_remover_drops_unreferenced_tags() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_jumpdest_remover = true;

        let result = optimize_with(
            vec![
                push_tag(2),
                operation(op::JUMP),
                tag(1),
                operation(op::STOP),
                tag(2),
                operation(op::STOP),
            ],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(result.optimized_items.len(), 5);
        assert!(!result
            .optimized_items
            .iter()
            .any(|item| { item.kind == wire::KIND_TAG && item::tag_value(item) == Some(1) }));
    }

    #[test]
    fn peephole_removes_unreachable_code() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_peephole = true;

        let result = optimize_with(
            vec![
                push_tag(1),
                operation(op::JUMP),
                push_item(0),
                operation(op::SLOAD),
                tag(2),
                push_item(5),
                operation(op::STOP),
            ],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        let kinds_and_ops = result
            .optimized_items
            .iter()
            .map(|item| (item.kind, item.opcode))
            .collect::<Vec<_>>();
        assert_eq!(
            kinds_and_ops,
            vec![
                (wire::KIND_PUSH_TAG, 0),
                (wire::KIND_OPERATION, op::JUMP),
                (wire::KIND_TAG, 0),
                (wire::KIND_OPERATION, op::STOP),
            ]
        );
    }

    #[test]
    fn cse_folds_constant_expression() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_cse = true;

        let result = optimize_with(
            vec![push_item(7), push_item(8), operation(op::ADD)],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(result.optimized_items.len(), 1);
        assert_eq!(result.optimized_items[0].kind, wire::KIND_PUSH);
        assert_eq!(result.optimized_items[0].data[31], 15);
    }

    #[test]
    fn cse_preserves_unknown_stack_consumption() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_cse = true;

        let result = optimize_with(
            vec![
                operation(op::POP),
                push_item(4),
                operation(op::CALLDATASIZE),
                operation(op::LT),
                push_tag(1),
                operation(op::JUMPI),
            ],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        assert!(item::is_operation(&result.optimized_items[0], op::POP));
        assert_eq!(result.optimized_items[1].kind, wire::KIND_PUSH);
        assert_eq!(result.optimized_items[1].data[31], 4);
    }

    #[test]
    fn block_deduplicator_rewrites_duplicate_block_tags() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_deduplicate = true;

        let result = optimize_with(
            vec![
                tag(1),
                push_item(5),
                operation(op::STOP),
                tag(2),
                push_item(5),
                operation(op::STOP),
                push_tag(2),
                operation(op::JUMP),
            ],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(item::tag_value(&result.optimized_items[6]), Some(1));
        assert_eq!(result.tag_replacements.len(), 1);
        assert_eq!(result.tag_replacements[0].from, item::u256_from_u64(2));
        assert_eq!(result.tag_replacements[0].to, item::u256_from_u64(1));
    }

    #[test]
    fn block_deduplicator_hashes_self_recursive_blocks_equally() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_deduplicate = true;

        let result = optimize_with(
            vec![
                tag(1),
                push_tag(1),
                operation(op::JUMP),
                tag(2),
                push_tag(2),
                operation(op::JUMP),
                push_tag(2),
                operation(op::JUMP),
            ],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(item::tag_value(&result.optimized_items[4]), Some(1));
        assert_eq!(item::tag_value(&result.optimized_items[6]), Some(1));
        assert_eq!(result.tag_replacements.len(), 1);
        assert_eq!(result.tag_replacements[0].from, item::u256_from_u64(2));
        assert_eq!(result.tag_replacements[0].to, item::u256_from_u64(1));
    }

    #[test]
    fn inliner_replaces_small_jump_target() {
        let mut optimizer_settings = settings();
        optimizer_settings.run_inliner = true;

        let result = optimize_with(
            vec![
                push_tag(1),
                operation(op::JUMP),
                tag(1),
                operation(op::STOP),
            ],
            optimizer_settings,
        );

        assert!(result.ok, "{}", result.error_message);
        assert_eq!(
            result
                .optimized_items
                .iter()
                .map(|item| (item.kind, item.opcode))
                .collect::<Vec<_>>(),
            vec![
                (wire::KIND_OPERATION, op::STOP),
                (wire::KIND_TAG, 0),
                (wire::KIND_OPERATION, op::STOP),
            ]
        );
    }
}
