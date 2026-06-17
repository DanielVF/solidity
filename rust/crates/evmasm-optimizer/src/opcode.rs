#[allow(dead_code)]
pub mod op {
    pub const STOP: u8 = 0x00;
    pub const ADD: u8 = 0x01;
    pub const MUL: u8 = 0x02;
    pub const SUB: u8 = 0x03;
    pub const DIV: u8 = 0x04;
    pub const SDIV: u8 = 0x05;
    pub const MOD: u8 = 0x06;
    pub const SMOD: u8 = 0x07;
    pub const ADDMOD: u8 = 0x08;
    pub const MULMOD: u8 = 0x09;
    pub const EXP: u8 = 0x0a;
    pub const SIGNEXTEND: u8 = 0x0b;
    pub const LT: u8 = 0x10;
    pub const GT: u8 = 0x11;
    pub const SLT: u8 = 0x12;
    pub const SGT: u8 = 0x13;
    pub const EQ: u8 = 0x14;
    pub const ISZERO: u8 = 0x15;
    pub const AND: u8 = 0x16;
    pub const OR: u8 = 0x17;
    pub const XOR: u8 = 0x18;
    pub const NOT: u8 = 0x19;
    pub const BYTE: u8 = 0x1a;
    pub const SHL: u8 = 0x1b;
    pub const SHR: u8 = 0x1c;
    pub const SAR: u8 = 0x1d;
    pub const CLZ: u8 = 0x1e;
    pub const KECCAK256: u8 = 0x20;
    pub const ADDRESS: u8 = 0x30;
    pub const BALANCE: u8 = 0x31;
    pub const ORIGIN: u8 = 0x32;
    pub const CALLER: u8 = 0x33;
    pub const CALLVALUE: u8 = 0x34;
    pub const CALLDATALOAD: u8 = 0x35;
    pub const CALLDATASIZE: u8 = 0x36;
    pub const CALLDATACOPY: u8 = 0x37;
    pub const CODESIZE: u8 = 0x38;
    pub const CODECOPY: u8 = 0x39;
    pub const GASPRICE: u8 = 0x3a;
    pub const EXTCODESIZE: u8 = 0x3b;
    pub const EXTCODECOPY: u8 = 0x3c;
    pub const RETURNDATASIZE: u8 = 0x3d;
    pub const RETURNDATACOPY: u8 = 0x3e;
    pub const EXTCODEHASH: u8 = 0x3f;
    pub const BLOCKHASH: u8 = 0x40;
    pub const COINBASE: u8 = 0x41;
    pub const TIMESTAMP: u8 = 0x42;
    pub const NUMBER: u8 = 0x43;
    pub const PREVRANDAO: u8 = 0x44;
    pub const GASLIMIT: u8 = 0x45;
    pub const CHAINID: u8 = 0x46;
    pub const SELFBALANCE: u8 = 0x47;
    pub const BASEFEE: u8 = 0x48;
    pub const BLOBHASH: u8 = 0x49;
    pub const BLOBBASEFEE: u8 = 0x4a;
    pub const POP: u8 = 0x50;
    pub const MLOAD: u8 = 0x51;
    pub const MSTORE: u8 = 0x52;
    pub const MSTORE8: u8 = 0x53;
    pub const SLOAD: u8 = 0x54;
    pub const SSTORE: u8 = 0x55;
    pub const JUMP: u8 = 0x56;
    pub const JUMPI: u8 = 0x57;
    pub const PC: u8 = 0x58;
    pub const MSIZE: u8 = 0x59;
    pub const GAS: u8 = 0x5a;
    pub const JUMPDEST: u8 = 0x5b;
    pub const TLOAD: u8 = 0x5c;
    pub const TSTORE: u8 = 0x5d;
    pub const MCOPY: u8 = 0x5e;
    pub const PUSH0: u8 = 0x5f;
    pub const PUSH1: u8 = 0x60;
    pub const PUSH32: u8 = 0x7f;
    pub const DUP1: u8 = 0x80;
    pub const DUP16: u8 = 0x8f;
    pub const SWAP1: u8 = 0x90;
    pub const SWAP16: u8 = 0x9f;
    pub const LOG0: u8 = 0xa0;
    pub const LOG4: u8 = 0xa4;
    pub const CREATE: u8 = 0xf0;
    pub const CALL: u8 = 0xf1;
    pub const CALLCODE: u8 = 0xf2;
    pub const RETURN: u8 = 0xf3;
    pub const DELEGATECALL: u8 = 0xf4;
    pub const CREATE2: u8 = 0xf5;
    pub const STATICCALL: u8 = 0xfa;
    pub const REVERT: u8 = 0xfd;
    pub const INVALID: u8 = 0xfe;
    pub const SELFDESTRUCT: u8 = 0xff;
}

#[derive(Debug, Clone, Copy)]
pub struct EvmVersion {
    value: u16,
}

impl EvmVersion {
    pub fn new(value: u16) -> Self {
        Self { value }
    }

    pub fn is_istanbul_or_newer(self) -> bool {
        self.value >= 6
    }

    pub fn is_spurious_dragon_or_newer(self) -> bool {
        self.value >= 2
    }

    pub fn has_push0(self) -> bool {
        self.value >= 10
    }

    pub fn has_bitwise_shifting(self) -> bool {
        self.value >= 4
    }

    pub fn reachable_stack_depth(self) -> usize {
        16
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InstructionInfo {
    pub args: usize,
    pub ret: usize,
    pub side_effects: bool,
    pub gas_tier: GasTier,
}

#[derive(Debug, Clone, Copy)]
pub enum GasTier {
    Zero,
    Base,
    VeryLow,
    Low,
    Mid,
    High,
    BlockHash,
    WarmAccess,
    Special,
    Invalid,
}

pub fn instruction_info(opcode: u8) -> InstructionInfo {
    use op::*;
    match opcode {
        STOP => info(0, 0, true, GasTier::Zero),
        ADD | SUB | LT | GT | SLT | SGT | EQ | AND | OR | XOR | BYTE | SHL | SHR | SAR => {
            info(2, 1, false, GasTier::VeryLow)
        }
        MUL | DIV | SDIV | MOD | SMOD | SIGNEXTEND => info(2, 1, false, GasTier::Low),
        CLZ => info(1, 1, false, GasTier::Low),
        ADDMOD | MULMOD => info(3, 1, false, GasTier::Mid),
        EXP => info(2, 1, false, GasTier::Special),
        NOT | ISZERO => info(1, 1, false, GasTier::VeryLow),
        KECCAK256 => info(2, 1, true, GasTier::Special),
        ADDRESS | ORIGIN | CALLER | CALLVALUE | CODESIZE | GASPRICE | RETURNDATASIZE | COINBASE
        | TIMESTAMP | NUMBER | PREVRANDAO | GASLIMIT | CHAINID | BASEFEE | BLOBBASEFEE | PC
        | MSIZE | GAS => info(0, 1, false, GasTier::Base),
        SELFBALANCE => info(0, 1, false, GasTier::Low),
        BLOCKHASH => info(1, 1, false, GasTier::BlockHash),
        BALANCE | EXTCODESIZE | EXTCODEHASH => info(1, 1, false, GasTier::Special),
        CALLDATALOAD | BLOBHASH => info(1, 1, false, GasTier::VeryLow),
        CALLDATASIZE => info(0, 1, false, GasTier::Base),
        CALLDATACOPY | CODECOPY | RETURNDATACOPY | MCOPY => info(3, 0, true, GasTier::VeryLow),
        EXTCODECOPY => info(4, 0, true, GasTier::Special),
        POP => info(1, 0, false, GasTier::Base),
        MLOAD => info(1, 1, true, GasTier::VeryLow),
        MSTORE | MSTORE8 => info(2, 0, true, GasTier::VeryLow),
        SLOAD => info(1, 1, false, GasTier::Special),
        SSTORE => info(2, 0, true, GasTier::Special),
        TLOAD => info(1, 1, false, GasTier::WarmAccess),
        TSTORE => info(2, 0, true, GasTier::WarmAccess),
        JUMP => info(1, 0, true, GasTier::Mid),
        JUMPI => info(2, 0, true, GasTier::High),
        JUMPDEST => info(0, 0, true, GasTier::Special),
        PUSH0 => info(0, 1, false, GasTier::Base),
        PUSH1..=PUSH32 => info(0, 1, false, GasTier::VeryLow),
        DUP1..=DUP16 => info(
            (opcode - DUP1 + 1) as usize,
            (opcode - DUP1 + 2) as usize,
            false,
            GasTier::VeryLow,
        ),
        SWAP1..=SWAP16 => info(
            (opcode - SWAP1 + 2) as usize,
            (opcode - SWAP1 + 2) as usize,
            false,
            GasTier::VeryLow,
        ),
        LOG0..=LOG4 => info((opcode - LOG0 + 2) as usize, 0, true, GasTier::Special),
        CREATE => info(3, 1, true, GasTier::Special),
        CALL | CALLCODE => info(7, 1, true, GasTier::Special),
        RETURN => info(2, 0, true, GasTier::Zero),
        DELEGATECALL | STATICCALL => info(6, 1, true, GasTier::Special),
        CREATE2 => info(4, 1, true, GasTier::Special),
        REVERT => info(2, 0, true, GasTier::Zero),
        INVALID => info(0, 0, true, GasTier::Zero),
        SELFDESTRUCT => info(1, 0, true, GasTier::Special),
        _ => info(0, 0, false, GasTier::Invalid),
    }
}

fn info(args: usize, ret: usize, side_effects: bool, gas_tier: GasTier) -> InstructionInfo {
    InstructionInfo {
        args,
        ret,
        side_effects,
        gas_tier,
    }
}

pub fn run_gas(opcode: u8) -> Option<u64> {
    use GasTier::*;
    if opcode == op::JUMPDEST {
        return Some(1);
    }
    Some(match instruction_info(opcode).gas_tier {
        Zero => 0,
        Base => 2,
        VeryLow => 3,
        Low => 5,
        Mid => 8,
        High => 10,
        BlockHash => 20,
        WarmAccess => 100,
        Special | Invalid => return None,
    })
}

pub fn is_dup(opcode: u8) -> bool {
    (op::DUP1..=op::DUP16).contains(&opcode)
}

pub fn is_swap(opcode: u8) -> bool {
    (op::SWAP1..=op::SWAP16).contains(&opcode)
}

pub fn dup_number(opcode: u8) -> usize {
    debug_assert!(is_dup(opcode));
    (opcode - op::DUP1 + 1) as usize
}

pub fn swap_number(opcode: u8) -> usize {
    debug_assert!(is_swap(opcode));
    (opcode - op::SWAP1 + 1) as usize
}

pub fn dup_instruction(number: usize) -> u8 {
    debug_assert!((1..=16).contains(&number));
    op::DUP1 + number as u8 - 1
}

pub fn swap_instruction(number: usize) -> u8 {
    debug_assert!((1..=16).contains(&number));
    op::SWAP1 + number as u8 - 1
}

pub fn is_commutative(opcode: u8) -> bool {
    matches!(
        opcode,
        op::ADD | op::MUL | op::EQ | op::AND | op::OR | op::XOR
    )
}

pub fn terminates_control_flow(opcode: u8) -> bool {
    matches!(
        opcode,
        op::RETURN | op::SELFDESTRUCT | op::STOP | op::INVALID | op::REVERT
    )
}

pub fn alters_control_flow(opcode: u8) -> bool {
    matches!(
        opcode,
        op::JUMP | op::JUMPI | op::RETURN | op::SELFDESTRUCT | op::STOP | op::INVALID | op::REVERT
    )
}

pub fn breaks_cse_analysis_block(opcode: u8, msize_important: bool) -> bool {
    if is_swap(opcode) || is_dup(opcode) {
        return false;
    }
    if matches!(opcode, op::GAS | op::PC | op::MSIZE) {
        return true;
    }
    if matches!(opcode, op::SSTORE | op::MSTORE) {
        return false;
    }
    if !msize_important && matches!(opcode, op::MLOAD | op::KECCAK256) {
        return false;
    }
    let info = instruction_info(opcode);
    info.side_effects || info.args > 2
}

pub fn is_deterministic(opcode: u8) -> bool {
    !matches!(
        opcode,
        op::CALL
            | op::CALLCODE
            | op::DELEGATECALL
            | op::STATICCALL
            | op::CREATE
            | op::CREATE2
            | op::GAS
            | op::PC
            | op::MSIZE
            | op::BALANCE
            | op::SELFBALANCE
            | op::EXTCODESIZE
            | op::EXTCODEHASH
            | op::RETURNDATACOPY
            | op::RETURNDATASIZE
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    None,
    Read,
    Write,
}

pub fn memory_effect(opcode: u8) -> Effect {
    match opcode {
        op::CALLDATACOPY
        | op::CODECOPY
        | op::EXTCODECOPY
        | op::RETURNDATACOPY
        | op::MCOPY
        | op::MSTORE
        | op::MSTORE8
        | op::CALL
        | op::CALLCODE
        | op::DELEGATECALL
        | op::STATICCALL => Effect::Write,
        op::CREATE
        | op::CREATE2
        | op::KECCAK256
        | op::MLOAD
        | op::MSIZE
        | op::RETURN
        | op::REVERT
        | op::LOG0..=op::LOG4 => Effect::Read,
        _ => Effect::None,
    }
}

pub fn storage_effect(opcode: u8) -> Effect {
    match opcode {
        op::CALL | op::CALLCODE | op::DELEGATECALL | op::CREATE | op::CREATE2 | op::SSTORE => {
            Effect::Write
        }
        op::SLOAD | op::STATICCALL => Effect::Read,
        _ => Effect::None,
    }
}
