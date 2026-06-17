use crate::bridge::ffi;
use std::fmt;

pub const U256_BYTES: usize = 32;
pub const MAX_TAG_VALUE: u64 = u64::MAX;

pub const KIND_UNDEFINED: u8 = 0;
pub const KIND_OPERATION: u8 = 1;
pub const KIND_PUSH: u8 = 2;
pub const KIND_PUSH_TAG: u8 = 3;
pub const KIND_PUSH_SUB: u8 = 4;
pub const KIND_PUSH_SUB_SIZE: u8 = 5;
pub const KIND_PUSH_PROGRAM_SIZE: u8 = 6;
pub const KIND_TAG: u8 = 7;
pub const KIND_PUSH_DATA: u8 = 8;
pub const KIND_PUSH_LIBRARY_ADDRESS: u8 = 9;
pub const KIND_PUSH_DEPLOY_TIME_ADDRESS: u8 = 10;
pub const KIND_PUSH_IMMUTABLE: u8 = 11;
pub const KIND_ASSIGN_IMMUTABLE: u8 = 12;
pub const KIND_VERBATIM_BYTECODE: u8 = 13;

pub const JUMP_ORDINARY: u8 = 0;
pub const JUMP_INTO_FUNCTION: u8 = 1;
pub const JUMP_OUT_OF_FUNCTION: u8 = 2;

pub const OPTIMIZER_ERROR_PANIC: u8 = 255;

const VALID_ITEM_KINDS: [u8; 14] = [
    KIND_UNDEFINED,
    KIND_OPERATION,
    KIND_PUSH,
    KIND_PUSH_TAG,
    KIND_PUSH_SUB,
    KIND_PUSH_SUB_SIZE,
    KIND_PUSH_PROGRAM_SIZE,
    KIND_TAG,
    KIND_PUSH_DATA,
    KIND_PUSH_LIBRARY_ADDRESS,
    KIND_PUSH_DEPLOY_TIME_ADDRESS,
    KIND_PUSH_IMMUTABLE,
    KIND_ASSIGN_IMMUTABLE,
    KIND_VERBATIM_BYTECODE,
];

const VALID_JUMP_TYPES: [u8; 3] = [JUMP_ORDINARY, JUMP_INTO_FUNCTION, JUMP_OUT_OF_FUNCTION];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizerError {
    InvalidWire,
    InvalidTag(String),
    InvalidState(String),
}

impl OptimizerError {
    pub fn code(&self) -> u8 {
        match self {
            Self::InvalidWire => 1,
            Self::InvalidTag(_) => 2,
            Self::InvalidState(_) => 3,
        }
    }
}

impl fmt::Display for OptimizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWire => write!(f, "invalid optimizer wire item"),
            Self::InvalidTag(message) => write!(f, "{message}"),
            Self::InvalidState(message) => write!(f, "{message}"),
        }
    }
}

pub fn validate_settings(_settings: &ffi::WireOptimizerSettings) -> Result<(), OptimizerError> {
    Ok(())
}

pub fn validate_evm_version(_evm_version: &ffi::WireEvmVersion) -> Result<(), OptimizerError> {
    Ok(())
}

pub fn validate_assembly_item(item: &ffi::WireAssemblyItem) -> Result<(), OptimizerError> {
    if !VALID_ITEM_KINDS.contains(&item.kind) {
        return Err(OptimizerError::InvalidWire);
    }
    if !VALID_JUMP_TYPES.contains(&item.jump_type) {
        return Err(OptimizerError::InvalidWire);
    }
    if item.has_pushed_value && item.pushed_value.len() != U256_BYTES {
        return Err(OptimizerError::InvalidWire);
    }

    match item.kind {
        KIND_UNDEFINED => Err(OptimizerError::InvalidWire),
        KIND_OPERATION => {
            if !item.data.is_empty() || !item.verbatim_data.is_empty() {
                return Err(OptimizerError::InvalidWire);
            }
            Ok(())
        }
        KIND_VERBATIM_BYTECODE => {
            if !item.data.is_empty() || item.opcode != 0 {
                return Err(OptimizerError::InvalidWire);
            }
            Ok(())
        }
        _ => {
            if item.data.len() != U256_BYTES || !item.verbatim_data.is_empty() || item.opcode != 0 {
                return Err(OptimizerError::InvalidWire);
            }
            Ok(())
        }
    }
}
