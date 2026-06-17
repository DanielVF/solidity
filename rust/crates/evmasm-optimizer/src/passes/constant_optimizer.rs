use crate::bridge::ffi;
use crate::item;
use crate::opcode::{self, op, EvmVersion};
use crate::u256;
use crate::wire;
use primitive_types::U256;
use sha3::{Digest, Keccak256};
use std::collections::BTreeMap;

const CREATE_DATA_GAS: u128 = 200;
const TX_DATA_ZERO_GAS: u128 = 4;
const COPY_GAS: u128 = 3;
const EXP_GAS: u128 = 10;

#[derive(Debug, Clone)]
struct Candidate {
    gas: u128,
    replacement: Vec<ffi::WireAssemblyItem>,
    data_entry: Option<ffi::WireDataEntry>,
}

pub fn optimise(
    items: &mut Vec<ffi::WireAssemblyItem>,
    is_creation: bool,
    runs: u64,
    evm_version: EvmVersion,
) -> Vec<ffi::WireDataEntry> {
    let mut pushes = BTreeMap::<U256, usize>::new();
    for item in items.iter().filter(|item| item.kind == wire::KIND_PUSH) {
        if let Some(value) = u256::from_be_bytes(&item.data) {
            *pushes.entry(value).or_default() += 1;
        }
    }

    let mut replacements = BTreeMap::<U256, Vec<ffi::WireAssemblyItem>>::new();
    let mut data_entries = Vec::new();

    for (value, multiplicity) in pushes {
        if value < U256::from(0x100u64) {
            continue;
        }
        let literal = literal_candidate(value, multiplicity, is_creation, runs, evm_version);
        let copy = copy_candidate(value, multiplicity, is_creation, runs, evm_version);
        let compute = compute_candidate(value, multiplicity, is_creation, runs, evm_version);

        let best = if copy.gas < literal.gas && copy.gas < compute.gas {
            Some(copy)
        } else if compute.gas < literal.gas && compute.gas <= copy.gas {
            Some(compute)
        } else {
            None
        };

        if let Some(best) = best {
            if let Some(data_entry) = best.data_entry {
                data_entries.push(data_entry);
            }
            if !best.replacement.is_empty() {
                replacements.insert(value, best.replacement);
            }
        }
    }

    if !replacements.is_empty() {
        let mut replaced = Vec::with_capacity(items.len());
        for assembly_item in items.iter() {
            if assembly_item.kind == wire::KIND_PUSH {
                if let Some(value) = u256::from_be_bytes(&assembly_item.data) {
                    if let Some(replacement) = replacements.get(&value) {
                        replaced.extend(replacement.iter().cloned());
                        continue;
                    }
                }
            }
            replaced.push(assembly_item.clone());
        }
        *items = replaced;
    }

    data_entries
}

fn literal_candidate(
    value: U256,
    multiplicity: usize,
    is_creation: bool,
    runs: u64,
    evm_version: EvmVersion,
) -> Candidate {
    let compact = compact_big_endian(value, 1);
    Candidate {
        gas: combine_gas(
            runs,
            multiplicity,
            push_run_gas(value, evm_version) as u128,
            per_code_byte_gas(is_creation, evm_version)
                + data_gas(&compact, is_creation, evm_version),
            0,
        ),
        replacement: Vec::new(),
        data_entry: None,
    }
}

fn copy_candidate(
    value: U256,
    multiplicity: usize,
    is_creation: bool,
    runs: u64,
    evm_version: EvmVersion,
) -> Candidate {
    let data = u256::to_be_bytes(value);
    let hash = Keccak256::digest(&data);
    let hash_value = U256::from_big_endian(&hash);
    let replacement = copy_routine(Some(hash_value), evm_version);
    let gas = combine_gas(
        runs,
        multiplicity,
        simple_run_gas(&copy_routine(None, evm_version), evm_version) + COPY_GAS,
        bytes_required(&copy_routine(None, evm_version), evm_version) as u128
            * per_code_byte_gas(is_creation, evm_version),
        data_gas(&data, is_creation, evm_version),
    );
    Candidate {
        gas,
        replacement,
        data_entry: Some(ffi::WireDataEntry {
            hash: hash.to_vec(),
            data,
        }),
    }
}

fn compute_candidate(
    value: U256,
    multiplicity: usize,
    is_creation: bool,
    runs: u64,
    evm_version: EvmVersion,
) -> Candidate {
    let mut max_steps = 10000usize;
    let replacement = find_representation(
        value,
        multiplicity,
        is_creation,
        runs,
        evm_version,
        &mut max_steps,
    );
    Candidate {
        gas: compute_gas(&replacement, multiplicity, is_creation, runs, evm_version),
        replacement,
        data_entry: None,
    }
}

fn find_representation(
    value: U256,
    multiplicity: usize,
    is_creation: bool,
    runs: u64,
    evm_version: EvmVersion,
    max_steps: &mut usize,
) -> Vec<ffi::WireAssemblyItem> {
    if value < U256::from(0x10000u64) {
        return vec![item::push_value(value, 0)];
    }
    if u256::number_encoding_size(!value) < u256::number_encoding_size(value) {
        let mut routine = find_representation(
            !value,
            multiplicity,
            is_creation,
            runs,
            evm_version,
            max_steps,
        );
        routine.push(item::operation(op::NOT, 0));
        return routine;
    }

    let mut routine = vec![item::push_value(value, 0)];
    let mut best_gas = compute_gas(&routine, multiplicity, is_creation, runs, evm_version);

    for bits in (9..=255u32).rev() {
        let gap_detector = (value >> (bits - 8)) & U256::from(0x1ffu64);
        if gap_detector != U256::from(0xffu64) && gap_detector != U256::from(0x100u64) {
            continue;
        }
        if *max_steps == 0 {
            break;
        }

        let power_of_two = U256::one() << bits;
        let mut upper_part = value >> bits;
        let mut lower_abs = value & (power_of_two - U256::one());
        let mut lower_negative = false;
        if power_of_two - lower_abs < lower_abs {
            lower_abs = power_of_two - lower_abs;
            lower_negative = true;
            upper_part = upper_part.overflowing_add(U256::one()).0;
        }
        if upper_part.is_zero() || lower_abs >= (power_of_two >> 8) {
            continue;
        }

        let mut new_routine = Vec::new();
        if !lower_abs.is_zero() {
            new_routine.extend(find_representation(
                lower_abs,
                multiplicity,
                is_creation,
                runs,
                evm_version,
                max_steps,
            ));
        }
        if evm_version.has_bitwise_shifting() {
            new_routine.extend(find_representation(
                upper_part,
                multiplicity,
                is_creation,
                runs,
                evm_version,
                max_steps,
            ));
            new_routine.push(item::push_value(U256::from(bits), 0));
            new_routine.push(item::operation(op::SHL, 0));
        } else {
            new_routine.push(item::push_value(U256::from(bits), 0));
            new_routine.push(item::push_value(U256::from(2u8), 0));
            new_routine.push(item::operation(op::EXP, 0));
            if upper_part != U256::one() {
                new_routine.extend(find_representation(
                    upper_part,
                    multiplicity,
                    is_creation,
                    runs,
                    evm_version,
                    max_steps,
                ));
                new_routine.push(item::operation(op::MUL, 0));
            }
        }
        if !lower_abs.is_zero() {
            new_routine.push(item::operation(
                if lower_negative { op::SUB } else { op::ADD },
                0,
            ));
        }

        if *max_steps > 0 {
            *max_steps -= 1;
        }
        let new_gas = compute_gas(&new_routine, multiplicity, is_creation, runs, evm_version);
        if new_gas < best_gas {
            best_gas = new_gas;
            routine = new_routine;
        }
    }

    routine
}

fn copy_routine(
    push_data_hash: Option<U256>,
    evm_version: EvmVersion,
) -> Vec<ffi::WireAssemblyItem> {
    let data_used = push_data_hash
        .map(push_data_item)
        .unwrap_or_else(|| push_data_item(U256::one() << 16));

    if evm_version.has_push0() {
        vec![
            item::push_value(U256::zero(), 0),
            item::operation(op::MLOAD, 0),
            item::push_value(U256::from(32u8), 0),
            data_used,
            item::push_value(U256::zero(), 0),
            item::operation(op::CODECOPY, 0),
            item::push_value(U256::zero(), 0),
            item::operation(op::MLOAD, 0),
            item::operation(op::SWAP1, 0),
            item::push_value(U256::zero(), 0),
            item::operation(op::MSTORE, 0),
        ]
    } else {
        vec![
            item::push_value(U256::zero(), 0),
            item::operation(op::DUP1, 0),
            item::operation(op::MLOAD, 0),
            item::push_value(U256::from(32u8), 0),
            data_used,
            item::operation(opcode::dup_instruction(4), 0),
            item::operation(op::CODECOPY, 0),
            item::operation(opcode::dup_instruction(2), 0),
            item::operation(op::MLOAD, 0),
            item::operation(opcode::swap_instruction(2), 0),
            item::operation(op::MSTORE, 0),
        ]
    }
}

fn push_data_item(hash: U256) -> ffi::WireAssemblyItem {
    let mut item = item::push_value(hash, 0);
    item.kind = wire::KIND_PUSH_DATA;
    item
}

fn compute_gas(
    routine: &[ffi::WireAssemblyItem],
    multiplicity: usize,
    is_creation: bool,
    runs: u64,
    evm_version: EvmVersion,
) -> u128 {
    let exp_count = routine
        .iter()
        .filter(|item| item::is_operation(item, op::EXP))
        .count() as u128;
    combine_gas(
        runs,
        multiplicity,
        simple_run_gas(routine, evm_version) + exp_count * (EXP_GAS + exp_byte_gas(evm_version)),
        bytes_required(routine, evm_version) as u128 * per_code_byte_gas(is_creation, evm_version),
        0,
    )
}

fn combine_gas(
    runs: u64,
    multiplicity: usize,
    run_gas: u128,
    repeated_data_gas: u128,
    unique_data_gas: u128,
) -> u128 {
    runs as u128 * run_gas + multiplicity as u128 * repeated_data_gas + unique_data_gas
}

fn simple_run_gas(items: &[ffi::WireAssemblyItem], evm_version: EvmVersion) -> u128 {
    items
        .iter()
        .map(|assembly_item| match assembly_item.kind {
            wire::KIND_PUSH => u256::from_be_bytes(&assembly_item.data)
                .map(|value| push_run_gas(value, evm_version) as u128)
                .unwrap_or(0),
            wire::KIND_OPERATION if assembly_item.opcode == op::EXP => EXP_GAS,
            wire::KIND_OPERATION => opcode::run_gas(assembly_item.opcode).unwrap_or(0) as u128,
            _ => 0,
        })
        .sum()
}

fn push_run_gas(value: U256, evm_version: EvmVersion) -> u64 {
    if evm_version.has_push0() && value.is_zero() {
        2
    } else {
        3
    }
}

fn exp_byte_gas(evm_version: EvmVersion) -> u128 {
    if evm_version.is_spurious_dragon_or_newer() {
        50
    } else {
        10
    }
}

fn bytes_required(items: &[ffi::WireAssemblyItem], evm_version: EvmVersion) -> usize {
    items
        .iter()
        .map(|assembly_item| item::bytes_required(assembly_item, 3, evm_version))
        .sum()
}

fn data_gas(data: &[u8], is_creation: bool, evm_version: EvmVersion) -> u128 {
    if is_creation {
        data.iter()
            .map(|byte| {
                if *byte == 0 {
                    TX_DATA_ZERO_GAS
                } else {
                    tx_data_non_zero_gas(evm_version)
                }
            })
            .sum()
    } else {
        CREATE_DATA_GAS * data.len() as u128
    }
}

fn per_code_byte_gas(is_creation: bool, evm_version: EvmVersion) -> u128 {
    if is_creation {
        tx_data_non_zero_gas(evm_version)
    } else {
        CREATE_DATA_GAS
    }
}

fn tx_data_non_zero_gas(evm_version: EvmVersion) -> u128 {
    if evm_version.is_istanbul_or_newer() {
        16
    } else {
        68
    }
}

fn compact_big_endian(value: U256, min_bytes: usize) -> Vec<u8> {
    let mut bytes = u256::to_be_bytes(value);
    let leading_zeroes = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    bytes.drain(0..std::cmp::min(leading_zeroes, bytes.len().saturating_sub(min_bytes)));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_byte_gas_matches_spurious_dragon_boundary() {
        assert_eq!(exp_byte_gas(EvmVersion::new(0)), 10);
        assert_eq!(exp_byte_gas(EvmVersion::new(1)), 10);
        assert_eq!(exp_byte_gas(EvmVersion::new(2)), 50);
    }
}
