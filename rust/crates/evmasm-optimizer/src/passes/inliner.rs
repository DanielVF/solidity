use crate::bridge::ffi;
use crate::item::{self, EMPTY_SUBASSEMBLY_ID};
use crate::opcode::{self, op, EvmVersion};
use crate::wire;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct InlinableBlock {
    items: Vec<ffi::WireAssemblyItem>,
    push_tag_count: u64,
}

pub fn optimise(
    items: &mut Vec<ffi::WireAssemblyItem>,
    tags_referenced_from_outside: &[u64],
    runs: u64,
    is_creation: bool,
    evm_version: EvmVersion,
) {
    let mut inlinable_blocks = determine_inlinable_blocks(items);
    if inlinable_blocks.is_empty() {
        return;
    }

    let outside_tags = tags_referenced_from_outside
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut new_items = Vec::with_capacity(items.len());
    let mut index = 0usize;

    while index < items.len() {
        let item = &items[index];
        if let Some(next_item) = items.get(index + 1) {
            if item.kind == wire::KIND_PUSH_TAG && item::is_operation(next_item, op::JUMP) {
                if let Some(tag) = local_tag(item) {
                    if let Some(inlinable_block_snapshot) = inlinable_blocks.get(&tag).cloned() {
                        if let Some(exit_item) = should_inline(
                            tag,
                            next_item,
                            &inlinable_block_snapshot,
                            &outside_tags,
                            runs,
                            is_creation,
                            evm_version,
                        ) {
                            new_items.extend(
                                inlinable_block_snapshot.items
                                    [..inlinable_block_snapshot.items.len().saturating_sub(1)]
                                    .iter()
                                    .cloned(),
                            );
                            new_items.push(exit_item);

                            if let Some(block) = inlinable_blocks.get_mut(&tag) {
                                block.push_tag_count = block.push_tag_count.saturating_sub(1);
                            }
                            for inlined_item in &inlinable_block_snapshot.items {
                                if inlined_item.kind == wire::KIND_PUSH_TAG {
                                    if let Some(duplicated_tag) = local_tag(inlined_item) {
                                        if let Some(block) =
                                            inlinable_blocks.get_mut(&duplicated_tag)
                                        {
                                            block.push_tag_count += 1;
                                        }
                                    }
                                }
                            }

                            index += 2;
                            continue;
                        }
                    }
                }
            }
        }

        new_items.push(item.clone());
        index += 1;
    }

    *items = new_items;
}

fn should_inline(
    tag: u64,
    jump: &ffi::WireAssemblyItem,
    block: &InlinableBlock,
    tags_referenced_from_outside: &BTreeSet<u64>,
    runs: u64,
    is_creation: bool,
    evm_version: EvmVersion,
) -> Option<ffi::WireAssemblyItem> {
    debug_assert!(item::is_operation(jump, op::JUMP));
    let mut block_exit = block.items.last()?.clone();

    if jump.jump_type == wire::JUMP_INTO_FUNCTION
        && item::is_operation(&block_exit, op::JUMP)
        && block_exit.jump_type == wire::JUMP_OUT_OF_FUNCTION
        && should_inline_full_function_body(
            tag,
            &block.items,
            block.push_tag_count,
            tags_referenced_from_outside,
            runs,
            is_creation,
            evm_version,
        )
    {
        block_exit.jump_type = wire::JUMP_ORDINARY;
        return Some(block_exit);
    }

    if jump.jump_type == wire::JUMP_ORDINARY
        || (block_exit.kind == wire::KIND_OPERATION
            && opcode::terminates_control_flow(block_exit.opcode))
    {
        let jump_pattern = vec![
            item::data_item(wire::KIND_PUSH_TAG, 0, 0),
            item::operation(op::JUMP, 0),
        ];
        if data_gas(
            code_size(&block.items, evm_version),
            is_creation,
            evm_version,
        ) <= data_gas(
            code_size(&jump_pattern, evm_version),
            is_creation,
            evm_version,
        ) {
            return Some(block_exit);
        }
    }

    None
}

fn should_inline_full_function_body(
    tag: u64,
    block: &[ffi::WireAssemblyItem],
    push_tag_count: u64,
    tags_referenced_from_outside: &BTreeSet<u64>,
    runs: u64,
    is_creation: bool,
    evm_version: EvmVersion,
) -> bool {
    let body_end = block.len().saturating_sub(1);
    let function_body_size = code_size(&block[..body_end], evm_version);
    let number_of_calls = push_tag_count as u128;
    let number_of_call_sites = push_tag_count as u128;

    let uninlined_call_site_pattern = vec![
        item::data_item(wire::KIND_PUSH_TAG, 0, 0),
        item::data_item(wire::KIND_PUSH_TAG, 0, 0),
        item::operation(op::JUMP, 0),
        item::data_item(wire::KIND_TAG, 0, 0),
    ];
    let uninlined_function_pattern = vec![
        item::data_item(wire::KIND_TAG, 0, 0),
        item::operation(op::JUMP, 0),
    ];

    let uninlined_execution_cost = number_of_calls
        * (execution_cost(&uninlined_call_site_pattern, evm_version)
            + execution_cost(&uninlined_function_pattern, evm_version));
    let uninlined_deposit_cost = data_gas_u128(
        number_of_call_sites * code_size(&uninlined_call_site_pattern, evm_version) as u128
            + code_size(&uninlined_function_pattern, evm_version) as u128
            + function_body_size as u128,
        is_creation,
        evm_version,
    );
    let mut inlined_deposit_cost = data_gas_u128(
        number_of_call_sites * function_body_size as u128,
        is_creation,
        evm_version,
    );

    if tags_referenced_from_outside.contains(&tag) {
        inlined_deposit_cost += data_gas_u128(
            code_size(&uninlined_function_pattern, evm_version) as u128
                + function_body_size as u128,
            is_creation,
            evm_version,
        );
    }

    let total_uninlined = runs as u128 * uninlined_execution_cost + uninlined_deposit_cost;
    let should_inline = total_uninlined > inlined_deposit_cost;
    should_inline
}

fn determine_inlinable_blocks(items: &[ffi::WireAssemblyItem]) -> BTreeMap<u64, InlinableBlock> {
    let mut inlinable_block_items = BTreeMap::<u64, Vec<ffi::WireAssemblyItem>>::new();
    let mut num_push_tags = BTreeMap::<u64, u64>::new();
    let mut last_tag = None::<usize>;

    for (index, assembly_item) in items.iter().enumerate() {
        if assembly_item.kind == wire::KIND_PUSH_TAG {
            if let Some(tag) = local_tag(assembly_item) {
                *num_push_tags.entry(tag).or_default() += 1;
            }
        }

        if let Some(last_tag_index) = last_tag {
            if breaks_cse_analysis_block(assembly_item, false) {
                let block = items[last_tag_index + 1..=index].to_vec();
                if let Some(tag) = local_tag(&items[last_tag_index]) {
                    if is_inline_candidate(tag, &block) {
                        inlinable_block_items.insert(tag, block);
                    }
                }
                last_tag = None;
            }
        }

        if assembly_item.kind == wire::KIND_TAG {
            debug_assert!(local_tag(assembly_item).is_some());
            last_tag = Some(index);
        }
    }

    inlinable_block_items
        .into_iter()
        .filter_map(|(tag, items)| {
            num_push_tags.get(&tag).copied().map(|push_tag_count| {
                (
                    tag,
                    InlinableBlock {
                        items,
                        push_tag_count,
                    },
                )
            })
        })
        .collect()
}

fn is_inline_candidate(tag: u64, items: &[ffi::WireAssemblyItem]) -> bool {
    let Some(last) = items.last() else {
        return false;
    };
    if last.kind != wire::KIND_OPERATION {
        return false;
    }
    if !item::is_operation(last, op::JUMP) && !opcode::terminates_control_flow(last.opcode) {
        return false;
    }

    for assembly_item in items {
        if assembly_item.kind == wire::KIND_PUSH_TAG && local_tag(assembly_item) == Some(tag) {
            return false;
        }
    }
    true
}

fn local_tag(item: &ffi::WireAssemblyItem) -> Option<u64> {
    if !matches!(item.kind, wire::KIND_PUSH_TAG | wire::KIND_TAG) {
        return None;
    }
    let split = item::split_tag(item);
    (split.sub_id == EMPTY_SUBASSEMBLY_ID).then_some(split.tag)
}

fn breaks_cse_analysis_block(item: &ffi::WireAssemblyItem, msize_important: bool) -> bool {
    match item.kind {
        wire::KIND_UNDEFINED
        | wire::KIND_TAG
        | wire::KIND_PUSH_DEPLOY_TIME_ADDRESS
        | wire::KIND_ASSIGN_IMMUTABLE
        | wire::KIND_VERBATIM_BYTECODE => true,
        wire::KIND_PUSH
        | wire::KIND_PUSH_TAG
        | wire::KIND_PUSH_SUB
        | wire::KIND_PUSH_SUB_SIZE
        | wire::KIND_PUSH_PROGRAM_SIZE
        | wire::KIND_PUSH_DATA
        | wire::KIND_PUSH_LIBRARY_ADDRESS
        | wire::KIND_PUSH_IMMUTABLE => false,
        wire::KIND_OPERATION => opcode::breaks_cse_analysis_block(item.opcode, msize_important),
        _ => true,
    }
}

fn code_size(items: &[ffi::WireAssemblyItem], evm_version: EvmVersion) -> u64 {
    items
        .iter()
        .map(|assembly_item| item::bytes_required(assembly_item, 2, evm_version) as u64)
        .sum()
}

fn execution_cost(items: &[ffi::WireAssemblyItem], evm_version: EvmVersion) -> u128 {
    items
        .iter()
        .map(|assembly_item| match assembly_item.kind {
            wire::KIND_PUSH => push_run_gas(item::u256_is_zero(&assembly_item.data), evm_version),
            wire::KIND_PUSH_TAG
            | wire::KIND_PUSH_DATA
            | wire::KIND_PUSH_SUB
            | wire::KIND_PUSH_SUB_SIZE
            | wire::KIND_PUSH_PROGRAM_SIZE
            | wire::KIND_PUSH_LIBRARY_ADDRESS
            | wire::KIND_PUSH_DEPLOY_TIME_ADDRESS => 3,
            wire::KIND_TAG => 1,
            wire::KIND_OPERATION => opcode::run_gas(assembly_item.opcode).unwrap_or(u64::MAX / 4),
            _ => u64::MAX / 4,
        } as u128)
        .sum()
}

fn push_run_gas(is_zero: bool, evm_version: EvmVersion) -> u64 {
    if evm_version.has_push0() && is_zero {
        2
    } else {
        3
    }
}

fn data_gas(length: u64, is_creation: bool, evm_version: EvmVersion) -> u128 {
    data_gas_u128(length as u128, is_creation, evm_version)
}

fn data_gas_u128(length: u128, is_creation: bool, evm_version: EvmVersion) -> u128 {
    length
        * if is_creation {
            tx_data_non_zero_gas(evm_version) as u128
        } else {
            200
        }
}

fn tx_data_non_zero_gas(evm_version: EvmVersion) -> u64 {
    if evm_version.is_istanbul_or_newer() {
        16
    } else {
        68
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use primitive_types::U256;

    #[test]
    fn literal_zero_execution_cost_uses_push0_when_available() {
        let items = vec![item::push_value(U256::zero(), 0)];

        assert_eq!(execution_cost(&items, EvmVersion::new(9)), 3);
        assert_eq!(execution_cost(&items, EvmVersion::new(10)), 2);
    }
}
