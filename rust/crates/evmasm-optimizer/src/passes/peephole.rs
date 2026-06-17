use crate::bridge::ffi;
use crate::item;
use crate::opcode::{self, op, EvmVersion};
use crate::wire;

pub fn optimise(items: &mut Vec<ffi::WireAssemblyItem>, evm_version: EvmVersion) -> bool {
    let mut optimised_items = Vec::with_capacity(items.len());
    let mut index = 0usize;
    while index < items.len() {
        apply_methods(items, &mut index, &mut optimised_items, evm_version);
    }

    let should_replace = optimised_items.len() < items.len()
        || (optimised_items.len() == items.len()
            && (bytes_required(&optimised_items, evm_version)
                < bytes_required(items, evm_version)
                || number_of_pops(&optimised_items) > number_of_pops(items)));

    if should_replace {
        *items = optimised_items;
        true
    } else {
        false
    }
}

fn apply_methods(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
    evm_version: EvmVersion,
) {
    if push_pop(items, index)
        || op_pop(items, index, out)
        || op_stop(items, index, out)
        || op_return_revert(items, index, out)
        || double_push(items, index, out, evm_version)
        || double_swap(items, index)
        || commutative_swap(items, index, out)
        || swap_comparison(items, index, out)
        || dup_swap(items, index, out)
        || iszero_iszero_jumpi(items, index, out)
        || eq_iszero_jumpi(items, index, out)
        || double_jump(items, index, out)
        || jump_to_next(items, index, out)
        || unreachable_code(items, index, out)
        || deduplicate_next_tag_size3(items, index, out)
        || deduplicate_next_tag_size2(items, index, out)
        || deduplicate_next_tag_size1(items, index, out)
        || tag_conjunctions(items, index, out)
        || truthy_and(items, index)
    {
        return;
    }

    out.push(items[*index].clone());
    *index += 1;
}

fn push_pop(items: &[ffi::WireAssemblyItem], index: &mut usize) -> bool {
    let Some([push, pop]) = window::<2>(items, *index) else {
        return false;
    };
    if item::is_operation(pop, op::POP)
        && (item::is_dup(push) || item::is_push_like_for_push_pop(push))
    {
        *index += 2;
        true
    } else {
        false
    }
}

fn op_pop(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([operation, pop]) = window::<2>(items, *index) else {
        return false;
    };
    if !item::is_operation(pop, op::POP) || operation.kind != wire::KIND_OPERATION {
        return false;
    }
    let info = opcode::instruction_info(operation.opcode);
    if info.ret == 1 && !info.side_effects {
        out.extend((0..info.args).map(|_| item::operation_like(op::POP, operation)));
        *index += 2;
        true
    } else {
        false
    }
}

fn op_stop(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([operation, stop]) = window::<2>(items, *index) else {
        return false;
    };
    if !item::is_operation(stop, op::STOP) {
        return false;
    }
    if operation.kind == wire::KIND_OPERATION
        && !opcode::instruction_info(operation.opcode).side_effects
    {
        out.push(item::operation_like(op::STOP, operation));
        *index += 2;
        true
    } else if operation.kind == wire::KIND_PUSH {
        out.push(item::operation_like(op::STOP, operation));
        *index += 2;
        true
    } else {
        false
    }
}

fn op_return_revert(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([operation, push, push_or_dup, return_revert]) = window::<4>(items, *index) else {
        return false;
    };
    if !(item::is_operation(return_revert, op::RETURN)
        || item::is_operation(return_revert, op::REVERT))
        || push.kind != wire::KIND_PUSH
        || !(push_or_dup.kind == wire::KIND_PUSH
            || item::is_operation(push_or_dup, opcode::dup_instruction(1)))
    {
        return false;
    }
    let removable = (operation.kind == wire::KIND_OPERATION
        && !opcode::instruction_info(operation.opcode).side_effects)
        || operation.kind == wire::KIND_PUSH;
    if removable {
        out.push(push.clone());
        out.push(push_or_dup.clone());
        out.push(return_revert.clone());
        *index += 4;
        true
    } else {
        false
    }
}

fn double_push(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
    evm_version: EvmVersion,
) -> bool {
    let Some([push1, push2]) = window::<2>(items, *index) else {
        return false;
    };
    if push1.kind == wire::KIND_PUSH
        && push2.kind == wire::KIND_PUSH
        && push1.data == push2.data
        && (!evm_version.has_push0() || !item::u256_is_zero(&push1.data))
    {
        out.push(push1.clone());
        out.push(item::operation_like(op::DUP1, push2));
        *index += 2;
        true
    } else {
        false
    }
}

fn double_swap(items: &[ffi::WireAssemblyItem], index: &mut usize) -> bool {
    let Some([swap1, swap2]) = window::<2>(items, *index) else {
        return false;
    };
    if item::item_eq(swap1, swap2) && item::is_swap(swap1) {
        *index += 2;
        true
    } else {
        false
    }
}

fn commutative_swap(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([swap, operation]) = window::<2>(items, *index) else {
        return false;
    };
    if item::is_operation(swap, op::SWAP1)
        && operation.kind == wire::KIND_OPERATION
        && opcode::is_commutative(operation.opcode)
    {
        out.push(operation.clone());
        *index += 2;
        true
    } else {
        false
    }
}

fn swap_comparison(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([swap, operation]) = window::<2>(items, *index) else {
        return false;
    };
    let replacement = match operation.opcode {
        op::LT => op::GT,
        op::GT => op::LT,
        op::SLT => op::SGT,
        op::SGT => op::SLT,
        _ => return false,
    };
    if item::is_operation(swap, op::SWAP1) && operation.kind == wire::KIND_OPERATION {
        out.push(item::operation_like(replacement, operation));
        *index += 2;
        true
    } else {
        false
    }
}

fn dup_swap(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([dup, swap]) = window::<2>(items, *index) else {
        return false;
    };
    if item::is_dup(dup)
        && item::is_swap(swap)
        && opcode::dup_number(dup.opcode) == opcode::swap_number(swap.opcode)
    {
        out.push(dup.clone());
        *index += 2;
        true
    } else {
        false
    }
}

fn iszero_iszero_jumpi(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([iszero1, iszero2, push_tag, jumpi]) = window::<4>(items, *index) else {
        return false;
    };
    if item::is_operation(iszero1, op::ISZERO)
        && item::is_operation(iszero2, op::ISZERO)
        && push_tag.kind == wire::KIND_PUSH_TAG
        && item::is_operation(jumpi, op::JUMPI)
    {
        out.push(push_tag.clone());
        out.push(jumpi.clone());
        *index += 4;
        true
    } else {
        false
    }
}

fn eq_iszero_jumpi(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([eq, iszero, push_tag, jumpi]) = window::<4>(items, *index) else {
        return false;
    };
    if item::is_operation(eq, op::EQ)
        && item::is_operation(iszero, op::ISZERO)
        && push_tag.kind == wire::KIND_PUSH_TAG
        && item::is_operation(jumpi, op::JUMPI)
    {
        out.push(item::operation_like(op::SUB, eq));
        out.push(push_tag.clone());
        out.push(jumpi.clone());
        *index += 4;
        true
    } else {
        false
    }
}

fn double_jump(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([push_tag1, jumpi, push_tag2, jump, tag1]) = window::<5>(items, *index) else {
        return false;
    };
    if push_tag1.kind == wire::KIND_PUSH_TAG
        && item::is_operation(jumpi, op::JUMPI)
        && push_tag2.kind == wire::KIND_PUSH_TAG
        && item::is_operation(jump, op::JUMP)
        && tag1.kind == wire::KIND_TAG
        && push_tag1.data == tag1.data
    {
        out.push(item::operation_like(op::ISZERO, jumpi));
        out.push(push_tag2.clone());
        out.push(jumpi.clone());
        out.push(tag1.clone());
        *index += 5;
        true
    } else {
        false
    }
}

fn jump_to_next(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([push_tag, jump, tag]) = window::<3>(items, *index) else {
        return false;
    };
    if push_tag.kind == wire::KIND_PUSH_TAG
        && (item::is_operation(jump, op::JUMP) || item::is_operation(jump, op::JUMPI))
        && tag.kind == wire::KIND_TAG
        && push_tag.data == tag.data
    {
        if item::is_operation(jump, op::JUMPI) {
            out.push(item::operation_like(op::POP, jump));
        }
        out.push(tag.clone());
        *index += 3;
        true
    } else {
        false
    }
}

fn tag_conjunctions(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([push_tag, push_constant, and]) = window::<3>(items, *index) else {
        return false;
    };
    if !item::is_operation(and, op::AND) {
        return false;
    }
    if push_tag.kind == wire::KIND_PUSH_TAG
        && push_constant.kind == wire::KIND_PUSH
        && item::u256_low32_all_ones(&push_constant.data)
    {
        out.push(push_tag.clone());
        *index += 3;
        true
    } else if push_constant.kind == wire::KIND_PUSH_TAG
        && push_tag.kind == wire::KIND_PUSH
        && item::u256_low32_all_ones(&push_tag.data)
    {
        out.push(push_constant.clone());
        *index += 3;
        true
    } else {
        false
    }
}

fn truthy_and(items: &[ffi::WireAssemblyItem], index: &mut usize) -> bool {
    let Some([push, not, and]) = window::<3>(items, *index) else {
        return false;
    };
    if push.kind == wire::KIND_PUSH
        && item::u256_is_zero(&push.data)
        && item::is_operation(not, op::NOT)
        && item::is_operation(and, op::AND)
    {
        *index += 3;
        true
    } else {
        false
    }
}

fn unreachable_code(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some(first) = items.get(*index) else {
        return false;
    };
    if !matches!(
        (first.kind, first.opcode),
        (wire::KIND_OPERATION, op::JUMP)
            | (wire::KIND_OPERATION, op::RETURN)
            | (wire::KIND_OPERATION, op::STOP)
            | (wire::KIND_OPERATION, op::INVALID)
            | (wire::KIND_OPERATION, op::SELFDESTRUCT)
            | (wire::KIND_OPERATION, op::REVERT)
    ) {
        return false;
    }

    let mut offset = 1usize;
    while *index + offset < items.len() && items[*index + offset].kind != wire::KIND_TAG {
        offset += 1;
    }
    if offset > 1 {
        out.push(first.clone());
        *index += offset;
        true
    } else {
        false
    }
}

fn deduplicate_next_tag_size3(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([preceding, item_a, item_b, breaking, tag, item_c, item_d, breaking2]) =
        window::<8>(items, *index)
    else {
        return false;
    };
    if preceding.kind != wire::KIND_TAG
        && item::item_eq(item_a, item_c)
        && item::item_eq(item_b, item_d)
        && item::item_eq(breaking, breaking2)
        && tag.kind == wire::KIND_TAG
        && breaking.kind == wire::KIND_OPERATION
        && opcode::terminates_control_flow(breaking.opcode)
    {
        out.push(preceding.clone());
        out.push(tag.clone());
        out.push(item_c.clone());
        out.push(item_d.clone());
        out.push(breaking2.clone());
        *index += 8;
        true
    } else {
        false
    }
}

fn deduplicate_next_tag_size2(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([preceding, item_a, breaking, tag, item_c, breaking2]) = window::<6>(items, *index)
    else {
        return false;
    };
    if preceding.kind != wire::KIND_TAG
        && item::item_eq(item_a, item_c)
        && item::item_eq(breaking, breaking2)
        && tag.kind == wire::KIND_TAG
        && breaking.kind == wire::KIND_OPERATION
        && opcode::terminates_control_flow(breaking.opcode)
    {
        out.push(preceding.clone());
        out.push(tag.clone());
        out.push(item_c.clone());
        out.push(breaking2.clone());
        *index += 6;
        true
    } else {
        false
    }
}

fn deduplicate_next_tag_size1(
    items: &[ffi::WireAssemblyItem],
    index: &mut usize,
    out: &mut Vec<ffi::WireAssemblyItem>,
) -> bool {
    let Some([preceding, breaking, tag, breaking2]) = window::<4>(items, *index) else {
        return false;
    };
    if preceding.kind != wire::KIND_TAG
        && item::item_eq(breaking, breaking2)
        && tag.kind == wire::KIND_TAG
        && breaking.kind == wire::KIND_OPERATION
        && opcode::terminates_control_flow(breaking.opcode)
    {
        out.push(preceding.clone());
        out.push(tag.clone());
        out.push(breaking2.clone());
        *index += 4;
        true
    } else {
        false
    }
}

fn bytes_required(items: &[ffi::WireAssemblyItem], evm_version: EvmVersion) -> usize {
    items
        .iter()
        .map(|assembly_item| item::bytes_required(assembly_item, 3, evm_version))
        .sum()
}

fn number_of_pops(items: &[ffi::WireAssemblyItem]) -> usize {
    items
        .iter()
        .filter(|assembly_item| item::is_operation(assembly_item, op::POP))
        .count()
}

fn window<const N: usize>(
    items: &[ffi::WireAssemblyItem],
    index: usize,
) -> Option<&[ffi::WireAssemblyItem; N]> {
    items.get(index..index + N)?.try_into().ok()
}
