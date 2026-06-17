use crate::bridge::ffi;
use crate::opcode;
use crate::wire;
use primitive_types::U256;
use std::cmp::Ordering;

pub const EMPTY_SUBASSEMBLY_ID: u64 = u64::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitTag {
    pub sub_id: u64,
    pub tag: u64,
}

pub fn operation(opcode: u8, debug_data_id: u64) -> ffi::WireAssemblyItem {
    ffi::WireAssemblyItem {
        kind: wire::KIND_OPERATION,
        opcode,
        data: Vec::new(),
        verbatim_data: Vec::new(),
        verbatim_arguments: 0,
        verbatim_return_values: 0,
        jump_type: wire::JUMP_ORDINARY,
        modifier_depth: 0,
        debug_data_id,
        has_pushed_value: false,
        pushed_value: Vec::new(),
        has_immutable_occurrences: false,
        immutable_occurrences: 0,
    }
}

pub fn operation_like(opcode: u8, item: &ffi::WireAssemblyItem) -> ffi::WireAssemblyItem {
    operation(opcode, item.debug_data_id)
}

pub fn data_item(kind: u8, value: u64, debug_data_id: u64) -> ffi::WireAssemblyItem {
    ffi::WireAssemblyItem {
        kind,
        opcode: 0,
        data: u256_from_u64(value),
        verbatim_data: Vec::new(),
        verbatim_arguments: 0,
        verbatim_return_values: 0,
        jump_type: wire::JUMP_ORDINARY,
        modifier_depth: 0,
        debug_data_id,
        has_pushed_value: false,
        pushed_value: Vec::new(),
        has_immutable_occurrences: false,
        immutable_occurrences: 0,
    }
}

pub fn is_operation(item: &ffi::WireAssemblyItem, opcode: u8) -> bool {
    item.kind == wire::KIND_OPERATION && item.opcode == opcode
}

pub fn is_dup(item: &ffi::WireAssemblyItem) -> bool {
    item.kind == wire::KIND_OPERATION && opcode::is_dup(item.opcode)
}

pub fn is_swap(item: &ffi::WireAssemblyItem) -> bool {
    item.kind == wire::KIND_OPERATION && opcode::is_swap(item.opcode)
}

pub fn is_push_like_for_push_pop(item: &ffi::WireAssemblyItem) -> bool {
    matches!(
        item.kind,
        wire::KIND_PUSH
            | wire::KIND_PUSH_TAG
            | wire::KIND_PUSH_SUB
            | wire::KIND_PUSH_SUB_SIZE
            | wire::KIND_PUSH_PROGRAM_SIZE
            | wire::KIND_PUSH_DATA
            | wire::KIND_PUSH_LIBRARY_ADDRESS
    )
}

pub fn item_eq(a: &ffi::WireAssemblyItem, b: &ffi::WireAssemblyItem) -> bool {
    if a.kind != b.kind {
        return false;
    }
    match a.kind {
        wire::KIND_OPERATION => a.opcode == b.opcode,
        wire::KIND_VERBATIM_BYTECODE => {
            a.verbatim_arguments == b.verbatim_arguments
                && a.verbatim_return_values == b.verbatim_return_values
                && a.verbatim_data == b.verbatim_data
        }
        _ => a.data == b.data,
    }
}

pub fn item_cmp(a: &ffi::WireAssemblyItem, b: &ffi::WireAssemblyItem) -> Ordering {
    match a.kind.cmp(&b.kind) {
        Ordering::Equal => {}
        ordering => return ordering,
    }
    match a.kind {
        wire::KIND_OPERATION => a.opcode.cmp(&b.opcode),
        wire::KIND_VERBATIM_BYTECODE => (
            a.verbatim_arguments,
            a.verbatim_return_values,
            &a.verbatim_data,
        )
            .cmp(&(
                b.verbatim_arguments,
                b.verbatim_return_values,
                &b.verbatim_data,
            )),
        _ => a.data.cmp(&b.data),
    }
}

pub fn split_tag(item: &ffi::WireAssemblyItem) -> SplitTag {
    debug_assert!(matches!(item.kind, wire::KIND_PUSH_TAG | wire::KIND_TAG));
    let high = u64_from_u256_slice(&item.data[16..24]);
    let tag = u64_from_u256_slice(&item.data[24..32]);
    SplitTag {
        sub_id: high.wrapping_sub(1),
        tag,
    }
}

pub fn set_push_tag_sub_id_and_tag(item: &mut ffi::WireAssemblyItem, sub_id: u64, tag: u64) {
    debug_assert!(matches!(item.kind, wire::KIND_PUSH_TAG | wire::KIND_TAG));
    let mut data = vec![0; wire::U256_BYTES];
    if sub_id != EMPTY_SUBASSEMBLY_ID {
        data[16..24].copy_from_slice(&sub_id.wrapping_add(1).to_be_bytes());
    }
    data[24..32].copy_from_slice(&tag.to_be_bytes());
    item.data = data;
}

pub fn tag_value(item: &ffi::WireAssemblyItem) -> Option<u64> {
    let split = split_tag(item);
    (split.sub_id == EMPTY_SUBASSEMBLY_ID).then_some(split.tag)
}

pub fn u256_from_u64(value: u64) -> Vec<u8> {
    let mut out = vec![0; wire::U256_BYTES];
    out[24..32].copy_from_slice(&value.to_be_bytes());
    out
}

pub fn u256_from_value(value: U256) -> Vec<u8> {
    crate::u256::to_be_bytes(value)
}

pub fn u256_from_neg_u64(value: u64) -> Vec<u8> {
    let mut out = vec![0xff; wire::U256_BYTES];
    let low = 0u64.wrapping_sub(value);
    out[24..32].copy_from_slice(&low.to_be_bytes());
    out
}

pub fn u256_is_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

pub fn u256_low32_all_ones(bytes: &[u8]) -> bool {
    bytes.len() == wire::U256_BYTES && bytes[28..32] == [0xff; 4]
}

pub fn number_encoding_size(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| *byte != 0)
        .map_or(0, |index| bytes.len() - index)
}

pub fn bytes_required(
    item: &ffi::WireAssemblyItem,
    address_length: usize,
    evm_version: opcode::EvmVersion,
) -> usize {
    match item.kind {
        wire::KIND_OPERATION | wire::KIND_TAG => 1,
        wire::KIND_PUSH => {
            1 + std::cmp::max(
                if evm_version.has_push0() { 0 } else { 1 },
                number_encoding_size(&item.data),
            )
        }
        wire::KIND_PUSH_SUB_SIZE | wire::KIND_PUSH_PROGRAM_SIZE => 5,
        wire::KIND_PUSH_TAG | wire::KIND_PUSH_DATA | wire::KIND_PUSH_SUB => 1 + address_length,
        wire::KIND_PUSH_LIBRARY_ADDRESS | wire::KIND_PUSH_DEPLOY_TIME_ADDRESS => 21,
        wire::KIND_PUSH_IMMUTABLE => 33,
        wire::KIND_ASSIGN_IMMUTABLE => 35,
        wire::KIND_VERBATIM_BYTECODE => item.verbatim_data.len(),
        _ => 0,
    }
}

pub fn arguments(item: &ffi::WireAssemblyItem) -> usize {
    if item.kind == wire::KIND_OPERATION {
        opcode::instruction_info(item.opcode).args
    } else if item.kind == wire::KIND_VERBATIM_BYTECODE {
        item.verbatim_arguments as usize
    } else if item.kind == wire::KIND_ASSIGN_IMMUTABLE {
        2
    } else {
        0
    }
}

pub fn return_values(item: &ffi::WireAssemblyItem) -> usize {
    match item.kind {
        wire::KIND_OPERATION => opcode::instruction_info(item.opcode).ret,
        wire::KIND_PUSH
        | wire::KIND_PUSH_TAG
        | wire::KIND_PUSH_DATA
        | wire::KIND_PUSH_SUB
        | wire::KIND_PUSH_SUB_SIZE
        | wire::KIND_PUSH_PROGRAM_SIZE
        | wire::KIND_PUSH_LIBRARY_ADDRESS
        | wire::KIND_PUSH_IMMUTABLE
        | wire::KIND_PUSH_DEPLOY_TIME_ADDRESS => 1,
        wire::KIND_VERBATIM_BYTECODE => item.verbatim_return_values as usize,
        _ => 0,
    }
}

pub fn deposit(item: &ffi::WireAssemblyItem) -> i32 {
    return_values(item) as i32 - arguments(item) as i32
}

pub fn push_value(value: U256, debug_data_id: u64) -> ffi::WireAssemblyItem {
    ffi::WireAssemblyItem {
        kind: wire::KIND_PUSH,
        opcode: 0,
        data: u256_from_value(value),
        verbatim_data: Vec::new(),
        verbatim_arguments: 0,
        verbatim_return_values: 0,
        jump_type: wire::JUMP_ORDINARY,
        modifier_depth: 0,
        debug_data_id,
        has_pushed_value: false,
        pushed_value: Vec::new(),
        has_immutable_occurrences: false,
        immutable_occurrences: 0,
    }
}

fn u64_from_u256_slice(bytes: &[u8]) -> u64 {
    let mut out = [0; 8];
    out.copy_from_slice(bytes);
    u64::from_be_bytes(out)
}
