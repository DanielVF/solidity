use primitive_types::{U256, U512};
use std::sync::OnceLock;

pub type MatchGroup = u8;

pub const GROUP_A: MatchGroup = 1;
pub const GROUP_B: MatchGroup = 2;
pub const GROUP_C: MatchGroup = 3;
pub const GROUP_X: MatchGroup = 5;
pub const GROUP_Y: MatchGroup = 6;
pub const GROUP_Z: MatchGroup = 7;

const MATCH_GROUP_CAPACITY: usize = GROUP_Z as usize + 1;

const ADD: &str = "add";
const MUL: &str = "mul";
const SUB: &str = "sub";
const DIV: &str = "div";
const SDIV: &str = "sdiv";
const MOD: &str = "mod";
const SMOD: &str = "smod";
const ADDMOD: &str = "addmod";
const MULMOD: &str = "mulmod";
const EXP: &str = "exp";
const SIGNEXTEND: &str = "signextend";
const LT: &str = "lt";
const GT: &str = "gt";
const SLT: &str = "slt";
const SGT: &str = "sgt";
const EQ: &str = "eq";
const ISZERO: &str = "iszero";
const AND: &str = "and";
const OR: &str = "or";
const XOR: &str = "xor";
const NOT: &str = "not";
const BYTE: &str = "byte";
const SHL: &str = "shl";
const SHR: &str = "shr";
const SAR: &str = "sar";
const ADDRESS: &str = "address";
const BALANCE: &str = "balance";
const ORIGIN: &str = "origin";
const CALLER: &str = "caller";
const COINBASE: &str = "coinbase";
const SELFBALANCE: &str = "selfbalance";

#[derive(Debug, Clone, Copy)]
pub struct MatchedExpression {
    pub expression_id: u64,
    pub constant: Option<U256>,
}

#[derive(Debug, Clone)]
pub struct MatchGroups {
    entries: [Option<MatchedExpression>; MATCH_GROUP_CAPACITY],
}

impl MatchGroups {
    pub fn new() -> Self {
        Self {
            entries: [None; MATCH_GROUP_CAPACITY],
        }
    }

    pub fn get(&self, group: MatchGroup) -> Option<&MatchedExpression> {
        self.entries.get(group as usize).and_then(Option::as_ref)
    }

    pub fn insert(&mut self, group: MatchGroup, expression: MatchedExpression) {
        self.entries[group as usize] = Some(expression);
    }
}

#[derive(Debug, Clone)]
pub enum PatternKind {
    Any,
    ConstantAny,
    ConstantValue(U256),
    Operation(&'static str),
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: PatternKind,
    pub arguments: Vec<Pattern>,
    pub match_group: Option<MatchGroup>,
}

impl Pattern {
    fn any(group: MatchGroup) -> Self {
        Self {
            kind: PatternKind::Any,
            arguments: Vec::new(),
            match_group: Some(group),
        }
    }

    fn constant_any(group: MatchGroup) -> Self {
        Self {
            kind: PatternKind::ConstantAny,
            arguments: Vec::new(),
            match_group: Some(group),
        }
    }

    fn constant_value(value: U256) -> Self {
        Self {
            kind: PatternKind::ConstantValue(value),
            arguments: Vec::new(),
            match_group: None,
        }
    }

    fn operation(name: &'static str, arguments: Vec<Pattern>) -> Self {
        Self {
            kind: PatternKind::Operation(name),
            arguments,
            match_group: None,
        }
    }

    fn root_name(&self) -> &'static str {
        match self.kind {
            PatternKind::Operation(name) => name,
            _ => panic!("simplification rule root must be an operation"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleGate {
    Always,
    HasBitwiseShifting,
    HasSelfBalance,
}

impl RuleGate {
    pub fn available(self, evm_version: u16) -> bool {
        match self {
            RuleGate::Always => true,
            // EVMVersion::Constantinople is index 4 in EVMVersion::allVersions().
            RuleGate::HasBitwiseShifting => evm_version >= 4,
            // EVMVersion::Istanbul is index 6 in EVMVersion::allVersions().
            RuleGate::HasSelfBalance => evm_version >= 6,
        }
    }
}

pub struct SimplificationRule {
    pub root_name: &'static str,
    pub pattern: Pattern,
    pub action: Box<dyn Fn(&MatchGroups) -> Pattern + Send + Sync>,
    pub feasible: Option<Box<dyn Fn(&MatchGroups) -> bool + Send + Sync>>,
    pub gate: RuleGate,
}

pub fn simplification_rules() -> &'static [SimplificationRule] {
    static RULES: OnceLock<Vec<SimplificationRule>> = OnceLock::new();
    RULES.get_or_init(build_simplification_rules).as_slice()
}

pub fn instruction_name_for_opcode(opcode: u16) -> Option<&'static str> {
    Some(match opcode {
        0x01 => ADD,
        0x02 => MUL,
        0x03 => SUB,
        0x04 => DIV,
        0x05 => SDIV,
        0x06 => MOD,
        0x07 => SMOD,
        0x08 => ADDMOD,
        0x09 => MULMOD,
        0x0a => EXP,
        0x0b => SIGNEXTEND,
        0x10 => LT,
        0x11 => GT,
        0x12 => SLT,
        0x13 => SGT,
        0x14 => EQ,
        0x15 => ISZERO,
        0x16 => AND,
        0x17 => OR,
        0x18 => XOR,
        0x19 => NOT,
        0x1a => BYTE,
        0x1b => SHL,
        0x1c => SHR,
        0x1d => SAR,
        0x30 => ADDRESS,
        0x31 => BALANCE,
        0x32 => ORIGIN,
        0x33 => CALLER,
        0x41 => COINBASE,
        0x47 => SELFBALANCE,
        _ => return None,
    })
}

fn build_simplification_rules() -> Vec<SimplificationRule> {
    let mut rules = Vec::new();
    add_part1_rules(&mut rules);
    add_part2_rules(&mut rules);
    add_part3_rules(&mut rules);
    add_part4_rules(&mut rules);
    add_part4_5_rules(&mut rules);
    add_part5_rules(&mut rules);
    add_part6_rules(&mut rules);
    add_part7_rules(&mut rules);
    add_part8_rules(&mut rules);
    add_evm_rules(&mut rules);
    rules
}

fn rule<F>(pattern: Pattern, action: F) -> SimplificationRule
where
    F: Fn(&MatchGroups) -> Pattern + Send + Sync + 'static,
{
    gated_rule(pattern, action, RuleGate::Always)
}

fn gated_rule<F>(pattern: Pattern, action: F, gate: RuleGate) -> SimplificationRule
where
    F: Fn(&MatchGroups) -> Pattern + Send + Sync + 'static,
{
    SimplificationRule {
        root_name: pattern.root_name(),
        pattern,
        action: Box::new(action),
        feasible: None,
        gate,
    }
}

fn feasible_rule<F, G>(pattern: Pattern, action: F, feasible: G) -> SimplificationRule
where
    F: Fn(&MatchGroups) -> Pattern + Send + Sync + 'static,
    G: Fn(&MatchGroups) -> bool + Send + Sync + 'static,
{
    gated_feasible_rule(pattern, action, feasible, RuleGate::Always)
}

fn gated_feasible_rule<F, G>(
    pattern: Pattern,
    action: F,
    feasible: G,
    gate: RuleGate,
) -> SimplificationRule
where
    F: Fn(&MatchGroups) -> Pattern + Send + Sync + 'static,
    G: Fn(&MatchGroups) -> bool + Send + Sync + 'static,
{
    SimplificationRule {
        root_name: pattern.root_name(),
        pattern,
        action: Box::new(action),
        feasible: Some(Box::new(feasible)),
        gate,
    }
}

fn any(group: MatchGroup) -> Pattern {
    Pattern::any(group)
}

fn push_any(group: MatchGroup) -> Pattern {
    Pattern::constant_any(group)
}

fn push_value(value: U256) -> Pattern {
    Pattern::constant_value(value)
}

fn push_u64(value: u64) -> Pattern {
    Pattern::constant_value(U256::from(value))
}

fn p_op(name: &'static str, arguments: Vec<Pattern>) -> Pattern {
    Pattern::operation(name, arguments)
}

fn group_const(groups: &MatchGroups, group: MatchGroup) -> U256 {
    groups
        .get(group)
        .and_then(|matched| matched.constant)
        .expect("constant match group must contain a number literal")
}

fn all_ones() -> U256 {
    !U256::zero()
}

fn wrapping_add(a: U256, b: U256) -> U256 {
    a.overflowing_add(b).0
}

fn wrapping_mul(a: U256, b: U256) -> U256 {
    a.overflowing_mul(b).0
}

fn wrapping_sub(a: U256, b: U256) -> U256 {
    a.overflowing_sub(b).0
}

fn bool_word(value: bool) -> U256 {
    if value {
        U256::one()
    } else {
        U256::zero()
    }
}

fn signed_is_negative(value: U256) -> bool {
    value.bit(255)
}

fn wrapping_neg(value: U256) -> U256 {
    (!value).overflowing_add(U256::one()).0
}

fn signed_abs(value: U256) -> (bool, U256) {
    if signed_is_negative(value) {
        (true, wrapping_neg(value))
    } else {
        (false, value)
    }
}

fn signed_cmp(a: U256, b: U256) -> std::cmp::Ordering {
    match (signed_is_negative(a), signed_is_negative(b)) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.cmp(&b),
    }
}

fn signed_div(a: U256, b: U256) -> U256 {
    if b.is_zero() {
        return U256::zero();
    }
    let (a_neg, a_abs) = signed_abs(a);
    let (b_neg, b_abs) = signed_abs(b);
    let quotient = a_abs / b_abs;
    if a_neg ^ b_neg {
        wrapping_neg(quotient)
    } else {
        quotient
    }
}

fn signed_mod(a: U256, b: U256) -> U256 {
    if b.is_zero() {
        return U256::zero();
    }
    let (a_neg, a_abs) = signed_abs(a);
    let (_, b_abs) = signed_abs(b);
    let remainder = a_abs % b_abs;
    if a_neg {
        wrapping_neg(remainder)
    } else {
        remainder
    }
}

fn byte(index: U256, value: U256) -> U256 {
    if index >= U256::from(32u8) {
        return U256::zero();
    }
    let shift = 8 * (31 - index.low_u32() as usize);
    (value >> shift) & U256::from(0xffu16)
}

fn signextend(index: U256, value: U256) -> U256 {
    if index >= U256::from(31u8) {
        return value;
    }
    let test_bit = index.low_u32() as usize * 8 + 7;
    let mask = (U256::one() << test_bit) - U256::one();
    if value.bit(test_bit) {
        value | !mask
    } else {
        value & mask
    }
}

fn addmod(a: U256, b: U256, modulus: U256) -> U256 {
    let result = (u256_to_u512(a) + u256_to_u512(b)) % u256_to_u512(modulus);
    u512_to_u256(result)
}

fn mulmod(a: U256, b: U256, modulus: U256) -> U256 {
    let result = (u256_to_u512(a) * u256_to_u512(b)) % u256_to_u512(modulus);
    u512_to_u256(result)
}

fn u256_to_u512(value: U256) -> U512 {
    let mut bytes = [0; 32];
    value.to_big_endian(&mut bytes);
    U512::from_big_endian(&bytes)
}

fn u512_to_u256(value: U512) -> U256 {
    let mut bytes = [0; 64];
    value.to_big_endian(&mut bytes);
    U256::from_big_endian(&bytes[32..])
}

fn binary_logarithm(value: U256) -> Option<usize> {
    if value.is_zero() {
        return None;
    }
    let bit = value.bits() - 1;
    (value == (U256::one() << bit)).then_some(bit)
}

fn is_power_of_two(value: U256) -> bool {
    !value.is_zero() && (value & value.overflowing_sub(U256::one()).0).is_zero()
}

fn u256_sum_ge_256(a: U256, b: U256) -> bool {
    a >= U256::from(256u16) || b >= U256::from(256u16) || a.low_u32() + b.low_u32() >= 256
}

fn full_mask_shr(shift: U256) -> U256 {
    if shift >= U256::from(256u16) {
        U256::zero()
    } else {
        all_ones() >> shift.low_u32()
    }
}

fn low_bits_mask(bits: usize) -> U256 {
    (U256::one() << bits) - U256::one()
}

fn add_part1_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let b = push_any(GROUP_B);
    let c = push_any(GROUP_C);

    rules.push(rule(p_op(ADD, vec![a.clone(), b.clone()]), |groups| {
        push_value(wrapping_add(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(MUL, vec![a.clone(), b.clone()]), |groups| {
        push_value(wrapping_mul(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(SUB, vec![a.clone(), b.clone()]), |groups| {
        push_value(wrapping_sub(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(DIV, vec![a.clone(), b.clone()]), |groups| {
        let divisor = group_const(groups, GROUP_B);
        push_value(if divisor.is_zero() {
            U256::zero()
        } else {
            group_const(groups, GROUP_A) / divisor
        })
    }));
    rules.push(rule(p_op(SDIV, vec![a.clone(), b.clone()]), |groups| {
        push_value(signed_div(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(MOD, vec![a.clone(), b.clone()]), |groups| {
        let divisor = group_const(groups, GROUP_B);
        push_value(if divisor.is_zero() {
            U256::zero()
        } else {
            group_const(groups, GROUP_A) % divisor
        })
    }));
    rules.push(rule(p_op(SMOD, vec![a.clone(), b.clone()]), |groups| {
        push_value(signed_mod(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(EXP, vec![a.clone(), b.clone()]), |groups| {
        push_value(
            group_const(groups, GROUP_A)
                .overflowing_pow(group_const(groups, GROUP_B))
                .0,
        )
    }));
    rules.push(rule(p_op(NOT, vec![a.clone()]), |groups| {
        push_value(!group_const(groups, GROUP_A))
    }));
    rules.push(rule(p_op(LT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            group_const(groups, GROUP_A) < group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(GT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            group_const(groups, GROUP_A) > group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(SLT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            signed_cmp(group_const(groups, GROUP_A), group_const(groups, GROUP_B)).is_lt(),
        ))
    }));
    rules.push(rule(p_op(SGT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            signed_cmp(group_const(groups, GROUP_A), group_const(groups, GROUP_B)).is_gt(),
        ))
    }));
    rules.push(rule(p_op(EQ, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            group_const(groups, GROUP_A) == group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(ISZERO, vec![a.clone()]), |groups| {
        push_value(bool_word(group_const(groups, GROUP_A).is_zero()))
    }));
    rules.push(rule(p_op(AND, vec![a.clone(), b.clone()]), |groups| {
        push_value(group_const(groups, GROUP_A) & group_const(groups, GROUP_B))
    }));
    rules.push(rule(p_op(OR, vec![a.clone(), b.clone()]), |groups| {
        push_value(group_const(groups, GROUP_A) | group_const(groups, GROUP_B))
    }));
    rules.push(rule(p_op(XOR, vec![a.clone(), b.clone()]), |groups| {
        push_value(group_const(groups, GROUP_A) ^ group_const(groups, GROUP_B))
    }));
    rules.push(rule(p_op(BYTE, vec![a.clone(), b.clone()]), |groups| {
        push_value(byte(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(
        p_op(ADDMOD, vec![a.clone(), b.clone(), c.clone()]),
        |groups| {
            let modulus = group_const(groups, GROUP_C);
            push_value(if modulus.is_zero() {
                U256::zero()
            } else {
                addmod(
                    group_const(groups, GROUP_A),
                    group_const(groups, GROUP_B),
                    modulus,
                )
            })
        },
    ));
    rules.push(rule(
        p_op(MULMOD, vec![a.clone(), b.clone(), c.clone()]),
        |groups| {
            let modulus = group_const(groups, GROUP_C);
            push_value(if modulus.is_zero() {
                U256::zero()
            } else {
                mulmod(
                    group_const(groups, GROUP_A),
                    group_const(groups, GROUP_B),
                    modulus,
                )
            })
        },
    ));
    rules.push(rule(
        p_op(SIGNEXTEND, vec![a.clone(), b.clone()]),
        |groups| {
            push_value(signextend(
                group_const(groups, GROUP_A),
                group_const(groups, GROUP_B),
            ))
        },
    ));
    rules.push(rule(p_op(SHL, vec![a.clone(), b.clone()]), |groups| {
        let shift = group_const(groups, GROUP_A);
        push_value(if shift >= U256::from(256u16) {
            U256::zero()
        } else {
            group_const(groups, GROUP_B) << shift.low_u32()
        })
    }));
    rules.push(rule(p_op(SHR, vec![a, b]), |groups| {
        let shift = group_const(groups, GROUP_A);
        push_value(if shift >= U256::from(256u16) {
            U256::zero()
        } else {
            group_const(groups, GROUP_B) >> shift.low_u32()
        })
    }));
}

fn add_part2_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    let y = any(GROUP_Y);
    let zero = push_u64(0);
    let one = push_u64(1);
    let max = push_value(all_ones());

    rules.push(rule(p_op(ADD, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(ADD, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(SUB, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(SUB, vec![max.clone(), x.clone()]), |_| {
        p_op(NOT, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(MUL, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(MUL, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(MUL, vec![x.clone(), one.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(MUL, vec![one.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(MUL, vec![x.clone(), max.clone()]), |_| {
        p_op(SUB, vec![push_u64(0), any(GROUP_X)])
    }));
    rules.push(rule(p_op(MUL, vec![max.clone(), x.clone()]), |_| {
        p_op(SUB, vec![push_u64(0), any(GROUP_X)])
    }));
    rules.push(rule(p_op(DIV, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(DIV, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(DIV, vec![x.clone(), one.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(SDIV, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(SDIV, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(SDIV, vec![x.clone(), one.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(AND, vec![x.clone(), max.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(AND, vec![max.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(AND, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(AND, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(OR, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(OR, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(OR, vec![x.clone(), max.clone()]), |_| {
        push_value(all_ones())
    }));
    rules.push(rule(p_op(OR, vec![max.clone(), x.clone()]), |_| {
        push_value(all_ones())
    }));
    rules.push(rule(p_op(XOR, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(XOR, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(MOD, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(MOD, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(EQ, vec![x.clone(), zero.clone()]), |_| {
        p_op(ISZERO, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(EQ, vec![zero.clone(), x.clone()]), |_| {
        p_op(ISZERO, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(SHL, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(SHR, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(SHL, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(SHR, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(GT, vec![x.clone(), zero.clone()]), |_| {
        p_op(ISZERO, vec![p_op(ISZERO, vec![any(GROUP_X)])])
    }));
    rules.push(rule(p_op(LT, vec![zero.clone(), x.clone()]), |_| {
        p_op(ISZERO, vec![p_op(ISZERO, vec![any(GROUP_X)])])
    }));
    rules.push(rule(p_op(GT, vec![x.clone(), max.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(LT, vec![max, x.clone()]), |_| push_u64(0)));
    rules.push(rule(p_op(GT, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(LT, vec![x.clone(), zero]), |_| push_u64(0)));
    rules.push(rule(
        p_op(
            AND,
            vec![p_op(BYTE, vec![x.clone(), y.clone()]), push_u64(0xff)],
        ),
        |_| p_op(BYTE, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(p_op(BYTE, vec![push_u64(31), x]), |_| {
        p_op(AND, vec![any(GROUP_X), push_u64(0xff)])
    }));
}

fn add_part3_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    for name in [AND, OR] {
        rules.push(rule(p_op(name, vec![x.clone(), x.clone()]), |_| {
            any(GROUP_X)
        }));
    }
    rules.push(rule(p_op(XOR, vec![x.clone(), x.clone()]), |_| push_u64(0)));
    rules.push(rule(p_op(SUB, vec![x.clone(), x.clone()]), |_| push_u64(0)));
    rules.push(rule(p_op(EQ, vec![x.clone(), x.clone()]), |_| push_u64(1)));
    for name in [LT, SLT, GT, SGT, MOD] {
        rules.push(rule(p_op(name, vec![x.clone(), x.clone()]), |_| {
            push_u64(0)
        }));
    }
}

fn add_part4_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    let y = any(GROUP_Y);

    rules.push(rule(p_op(NOT, vec![p_op(NOT, vec![x.clone()])]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(
        p_op(XOR, vec![x.clone(), p_op(XOR, vec![x.clone(), y.clone()])]),
        |_| any(GROUP_Y),
    ));
    rules.push(rule(
        p_op(XOR, vec![x.clone(), p_op(XOR, vec![y.clone(), x.clone()])]),
        |_| any(GROUP_Y),
    ));
    rules.push(rule(
        p_op(XOR, vec![p_op(XOR, vec![x.clone(), y.clone()]), x.clone()]),
        |_| any(GROUP_Y),
    ));
    rules.push(rule(
        p_op(XOR, vec![p_op(XOR, vec![y.clone(), x.clone()]), x.clone()]),
        |_| any(GROUP_Y),
    ));

    for pattern in [
        p_op(OR, vec![x.clone(), p_op(AND, vec![x.clone(), y.clone()])]),
        p_op(OR, vec![x.clone(), p_op(AND, vec![y.clone(), x.clone()])]),
        p_op(OR, vec![p_op(AND, vec![x.clone(), y.clone()]), x.clone()]),
        p_op(OR, vec![p_op(AND, vec![y.clone(), x.clone()]), x.clone()]),
        p_op(AND, vec![x.clone(), p_op(OR, vec![x.clone(), y.clone()])]),
        p_op(AND, vec![x.clone(), p_op(OR, vec![y.clone(), x.clone()])]),
        p_op(AND, vec![p_op(OR, vec![x.clone(), y.clone()]), x.clone()]),
        p_op(AND, vec![p_op(OR, vec![y.clone(), x.clone()]), x.clone()]),
    ] {
        rules.push(rule(pattern, |_| any(GROUP_X)));
    }

    rules.push(rule(
        p_op(AND, vec![x.clone(), p_op(NOT, vec![x.clone()])]),
        |_| push_u64(0),
    ));
    rules.push(rule(
        p_op(AND, vec![p_op(NOT, vec![x.clone()]), x.clone()]),
        |_| push_u64(0),
    ));
    rules.push(rule(
        p_op(OR, vec![x.clone(), p_op(NOT, vec![x.clone()])]),
        |_| push_value(all_ones()),
    ));
    rules.push(rule(
        p_op(OR, vec![p_op(NOT, vec![x]), any(GROUP_X)]),
        |_| push_value(all_ones()),
    ));
}

fn add_part4_5_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let b = push_any(GROUP_B);
    let x = any(GROUP_X);
    let y = any(GROUP_Y);

    for pattern in [
        p_op(AND, vec![p_op(AND, vec![x.clone(), y.clone()]), y.clone()]),
        p_op(AND, vec![y.clone(), p_op(AND, vec![x.clone(), y.clone()])]),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(AND, vec![any(GROUP_X), any(GROUP_Y)])
        }));
    }
    for pattern in [
        p_op(AND, vec![p_op(AND, vec![y.clone(), x.clone()]), y.clone()]),
        p_op(AND, vec![y.clone(), p_op(AND, vec![y.clone(), x.clone()])]),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(AND, vec![any(GROUP_Y), any(GROUP_X)])
        }));
    }
    for pattern in [
        p_op(OR, vec![p_op(OR, vec![x.clone(), y.clone()]), y.clone()]),
        p_op(OR, vec![y.clone(), p_op(OR, vec![x.clone(), y.clone()])]),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(OR, vec![any(GROUP_X), any(GROUP_Y)])
        }));
    }
    for pattern in [
        p_op(OR, vec![p_op(OR, vec![y.clone(), x.clone()]), y.clone()]),
        p_op(OR, vec![y, p_op(OR, vec![any(GROUP_Y), x.clone()])]),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(OR, vec![any(GROUP_Y), any(GROUP_X)])
        }));
    }
    rules.push(rule(
        p_op(
            SIGNEXTEND,
            vec![x.clone(), p_op(SIGNEXTEND, vec![x.clone(), any(GROUP_Y)])],
        ),
        |_| p_op(SIGNEXTEND, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(
        p_op(SIGNEXTEND, vec![a, p_op(SIGNEXTEND, vec![b, any(GROUP_X)])]),
        |groups| {
            p_op(
                SIGNEXTEND,
                vec![
                    push_value(std::cmp::min(
                        group_const(groups, GROUP_A),
                        group_const(groups, GROUP_B),
                    )),
                    any(GROUP_X),
                ],
            )
        },
    ));
}

fn add_part5_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let b = push_any(GROUP_B);
    let x = any(GROUP_X);
    let y = any(GROUP_Y);

    rules.push(feasible_rule(
        p_op(MOD, vec![p_op(MUL, vec![x.clone(), y.clone()]), a.clone()]),
        |_| p_op(MULMOD, vec![any(GROUP_X), any(GROUP_Y), push_any(GROUP_A)]),
        |groups| is_power_of_two(group_const(groups, GROUP_A)),
    ));
    rules.push(feasible_rule(
        p_op(MOD, vec![p_op(ADD, vec![x.clone(), y.clone()]), a.clone()]),
        |_| p_op(ADDMOD, vec![any(GROUP_X), any(GROUP_Y), push_any(GROUP_A)]),
        |groups| is_power_of_two(group_const(groups, GROUP_A)),
    ));

    for bit in 0..256usize {
        let value = U256::one() << bit;
        rules.push(rule(
            p_op(MOD, vec![x.clone(), push_value(value)]),
            move |_| {
                p_op(
                    AND,
                    vec![
                        any(GROUP_X),
                        push_value(value.overflowing_sub(U256::one()).0),
                    ],
                )
            },
        ));
    }

    rules.push(feasible_rule(
        p_op(SHL, vec![a.clone(), x.clone()]),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) >= U256::from(256u16),
    ));
    rules.push(feasible_rule(
        p_op(SHR, vec![a.clone(), x.clone()]),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) >= U256::from(256u16),
    ));
    rules.push(feasible_rule(
        p_op(BYTE, vec![a.clone(), x.clone()]),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) >= U256::from(32u8),
    ));
    rules.push(feasible_rule(
        p_op(SIGNEXTEND, vec![a.clone(), x.clone()]),
        |_| any(GROUP_X),
        |groups| group_const(groups, GROUP_A) >= U256::from(31u8),
    ));
    rules.push(feasible_rule(
        p_op(
            AND,
            vec![a.clone(), p_op(SIGNEXTEND, vec![b.clone(), x.clone()])],
        ),
        |_| p_op(AND, vec![push_any(GROUP_A), any(GROUP_X)]),
        |groups| {
            let index = group_const(groups, GROUP_B);
            index < U256::from(31u8)
                && (group_const(groups, GROUP_A)
                    & low_bits_mask((index.low_u32() as usize + 1) * 8))
                    == group_const(groups, GROUP_A)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            AND,
            vec![p_op(SIGNEXTEND, vec![b.clone(), x.clone()]), a.clone()],
        ),
        |_| p_op(AND, vec![push_any(GROUP_A), any(GROUP_X)]),
        |groups| {
            let index = group_const(groups, GROUP_B);
            index < U256::from(31u8)
                && (group_const(groups, GROUP_A)
                    & low_bits_mask((index.low_u32() as usize + 1) * 8))
                    == group_const(groups, GROUP_A)
        },
    ));

    let mask160 = low_bits_mask(160);
    for name in [ADDRESS, CALLER, ORIGIN, COINBASE] {
        rules.push(rule(
            p_op(AND, vec![p_op(name, Vec::new()), push_value(mask160)]),
            move |_| p_op(name, Vec::new()),
        ));
        rules.push(rule(
            p_op(AND, vec![push_value(mask160), p_op(name, Vec::new())]),
            move |_| p_op(name, Vec::new()),
        ));
    }
}

fn add_part6_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    let y = any(GROUP_Y);
    for name in [EQ, LT, SLT, GT, SGT] {
        rules.push(rule(
            p_op(
                ISZERO,
                vec![p_op(ISZERO, vec![p_op(name, vec![x.clone(), y.clone()])])],
            ),
            move |_| p_op(name, vec![any(GROUP_X), any(GROUP_Y)]),
        ));
    }
    rules.push(rule(
        p_op(
            ISZERO,
            vec![p_op(ISZERO, vec![p_op(ISZERO, vec![x.clone()])])],
        ),
        |_| p_op(ISZERO, vec![any(GROUP_X)]),
    ));
    rules.push(rule(
        p_op(ISZERO, vec![p_op(XOR, vec![x.clone(), y.clone()])]),
        |_| p_op(EQ, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(p_op(ISZERO, vec![p_op(SUB, vec![x, y])]), |_| {
        p_op(EQ, vec![any(GROUP_X), any(GROUP_Y)])
    }));
}

fn add_part7_rules(rules: &mut Vec<SimplificationRule>) {
    for (name, combine) in [
        (ADD, wrapping_add as fn(U256, U256) -> U256),
        (MUL, wrapping_mul as fn(U256, U256) -> U256),
        (AND, (|a, b| a & b) as fn(U256, U256) -> U256),
        (OR, (|a, b| a | b) as fn(U256, U256) -> U256),
        (XOR, (|a, b| a ^ b) as fn(U256, U256) -> U256),
    ] {
        for op_xa in [
            p_op(name, vec![any(GROUP_X), push_any(GROUP_A)]),
            p_op(name, vec![push_any(GROUP_A), any(GROUP_X)]),
        ] {
            rules.push(rule(
                p_op(name, vec![op_xa.clone(), push_any(GROUP_B)]),
                move |groups| {
                    p_op(
                        name,
                        vec![
                            any(GROUP_X),
                            push_value(combine(
                                group_const(groups, GROUP_A),
                                group_const(groups, GROUP_B),
                            )),
                        ],
                    )
                },
            ));
            rules.push(rule(
                p_op(name, vec![op_xa.clone(), any(GROUP_Y)]),
                move |_| {
                    p_op(
                        name,
                        vec![
                            p_op(name, vec![any(GROUP_X), any(GROUP_Y)]),
                            push_any(GROUP_A),
                        ],
                    )
                },
            ));
            rules.push(rule(
                p_op(name, vec![push_any(GROUP_B), op_xa.clone()]),
                move |groups| {
                    p_op(
                        name,
                        vec![
                            any(GROUP_X),
                            push_value(combine(
                                group_const(groups, GROUP_A),
                                group_const(groups, GROUP_B),
                            )),
                        ],
                    )
                },
            ));
            rules.push(rule(p_op(name, vec![any(GROUP_Y), op_xa]), move |_| {
                p_op(
                    name,
                    vec![
                        p_op(name, vec![any(GROUP_Y), any(GROUP_X)]),
                        push_any(GROUP_A),
                    ],
                )
            }));
        }
    }

    rules.push(rule(
        p_op(
            SHL,
            vec![
                push_any(GROUP_B),
                p_op(SHL, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            if u256_sum_ge_256(a, b) {
                p_op(AND, vec![any(GROUP_X), push_u64(0)])
            } else {
                p_op(SHL, vec![push_value(a + b), any(GROUP_X)])
            }
        },
    ));
    rules.push(rule(
        p_op(
            SHR,
            vec![
                push_any(GROUP_B),
                p_op(SHR, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            if u256_sum_ge_256(a, b) {
                p_op(AND, vec![any(GROUP_X), push_u64(0)])
            } else {
                p_op(SHR, vec![push_value(a + b), any(GROUP_X)])
            }
        },
    ));
    rules.push(feasible_rule(
        p_op(
            SHR,
            vec![
                push_any(GROUP_B),
                p_op(SHL, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            let mask = (all_ones() << a.low_u32()) >> b.low_u32();
            if a > b {
                p_op(
                    AND,
                    vec![
                        p_op(SHL, vec![push_value(a - b), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else if b > a {
                p_op(
                    AND,
                    vec![
                        p_op(SHR, vec![push_value(b - a), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else {
                p_op(AND, vec![any(GROUP_X), push_value(mask)])
            }
        },
        |groups| {
            group_const(groups, GROUP_A) < U256::from(256u16)
                && group_const(groups, GROUP_B) < U256::from(256u16)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            SHL,
            vec![
                push_any(GROUP_B),
                p_op(SHR, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            let mask = (all_ones() >> a.low_u32()) << b.low_u32();
            if a > b {
                p_op(
                    AND,
                    vec![
                        p_op(SHR, vec![push_value(a - b), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else if b > a {
                p_op(
                    AND,
                    vec![
                        p_op(SHL, vec![push_value(b - a), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else {
                p_op(AND, vec![any(GROUP_X), push_value(mask)])
            }
        },
        |groups| {
            group_const(groups, GROUP_A) < U256::from(256u16)
                && group_const(groups, GROUP_B) < U256::from(256u16)
        },
    ));

    for shift_name in [SHL, SHR] {
        rules.push(feasible_rule(
            p_op(
                shift_name,
                vec![
                    push_any(GROUP_B),
                    p_op(AND, vec![any(GROUP_X), push_any(GROUP_A)]),
                ],
            ),
            move |groups| {
                let mask = if shift_name == SHL {
                    group_const(groups, GROUP_A) << group_const(groups, GROUP_B).low_u32()
                } else {
                    group_const(groups, GROUP_A) >> group_const(groups, GROUP_B).low_u32()
                };
                p_op(
                    AND,
                    vec![
                        p_op(shift_name, vec![push_any(GROUP_B), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            },
            |groups| group_const(groups, GROUP_B) < U256::from(256u16),
        ));
        rules.push(feasible_rule(
            p_op(
                shift_name,
                vec![
                    push_any(GROUP_B),
                    p_op(AND, vec![push_any(GROUP_A), any(GROUP_X)]),
                ],
            ),
            move |groups| {
                let mask = if shift_name == SHL {
                    group_const(groups, GROUP_A) << group_const(groups, GROUP_B).low_u32()
                } else {
                    group_const(groups, GROUP_A) >> group_const(groups, GROUP_B).low_u32()
                };
                p_op(
                    AND,
                    vec![
                        p_op(shift_name, vec![push_any(GROUP_B), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            },
            |groups| group_const(groups, GROUP_B) < U256::from(256u16),
        ));
    }

    for inner in [
        p_op(AND, vec![any(GROUP_X), push_any(GROUP_A)]),
        p_op(AND, vec![push_any(GROUP_A), any(GROUP_X)]),
    ] {
        for second in [
            p_op(OR, vec![inner.clone(), any(GROUP_Y)]),
            p_op(OR, vec![any(GROUP_Y), inner.clone()]),
        ] {
            rules.push(rule(
                p_op(AND, vec![second.clone(), push_any(GROUP_B)]),
                |groups| {
                    p_op(
                        OR,
                        vec![
                            p_op(
                                AND,
                                vec![
                                    any(GROUP_X),
                                    push_value(
                                        group_const(groups, GROUP_A) & group_const(groups, GROUP_B),
                                    ),
                                ],
                            ),
                            p_op(AND, vec![any(GROUP_Y), push_any(GROUP_B)]),
                        ],
                    )
                },
            ));
            rules.push(rule(p_op(AND, vec![push_any(GROUP_B), second]), |groups| {
                p_op(
                    OR,
                    vec![
                        p_op(
                            AND,
                            vec![
                                any(GROUP_X),
                                push_value(
                                    group_const(groups, GROUP_A) & group_const(groups, GROUP_B),
                                ),
                            ],
                        ),
                        p_op(AND, vec![any(GROUP_Y), push_any(GROUP_B)]),
                    ],
                )
            }));
        }
    }

    rules.push(rule(
        p_op(
            MUL,
            vec![any(GROUP_X), p_op(SHL, vec![any(GROUP_Y), push_u64(1)])],
        ),
        |_| p_op(SHL, vec![any(GROUP_Y), any(GROUP_X)]),
    ));
    rules.push(rule(
        p_op(
            MUL,
            vec![p_op(SHL, vec![any(GROUP_X), push_u64(1)]), any(GROUP_Y)],
        ),
        |_| p_op(SHL, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(
        p_op(
            DIV,
            vec![any(GROUP_X), p_op(SHL, vec![any(GROUP_Y), push_u64(1)])],
        ),
        |_| p_op(SHR, vec![any(GROUP_Y), any(GROUP_X)]),
    ));

    rules.push(feasible_rule(
        p_op(
            AND,
            vec![
                push_any(GROUP_A),
                p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |_| p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
        |groups| {
            let b = group_const(groups, GROUP_B);
            b <= U256::from(256u16)
                && (group_const(groups, GROUP_A) & full_mask_shr(b)) == full_mask_shr(b)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            AND,
            vec![
                p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
                push_any(GROUP_A),
            ],
        ),
        |_| p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
        |groups| {
            let b = group_const(groups, GROUP_B);
            b <= U256::from(256u16)
                && (group_const(groups, GROUP_A) & full_mask_shr(b)) == full_mask_shr(b)
        },
    ));
    rules.push(rule(
        p_op(
            AND,
            vec![
                p_op(SHL, vec![any(GROUP_Z), any(GROUP_X)]),
                p_op(SHL, vec![any(GROUP_Z), any(GROUP_Y)]),
            ],
        ),
        |_| {
            p_op(
                SHL,
                vec![any(GROUP_Z), p_op(AND, vec![any(GROUP_X), any(GROUP_Y)])],
            )
        },
    ));
    rules.push(feasible_rule(
        p_op(
            BYTE,
            vec![
                push_any(GROUP_A),
                p_op(SHL, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |groups| {
            p_op(
                BYTE,
                vec![
                    push_value(group_const(groups, GROUP_A) + (group_const(groups, GROUP_B) >> 3)),
                    any(GROUP_X),
                ],
            )
        },
        |groups| {
            let b = group_const(groups, GROUP_B);
            (b & U256::from(7u8)).is_zero()
                && group_const(groups, GROUP_A) <= U256::from(32u8)
                && b <= U256::from(256u16)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            BYTE,
            vec![
                push_any(GROUP_A),
                p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) < (group_const(groups, GROUP_B) >> 3),
    ));
    rules.push(feasible_rule(
        p_op(
            BYTE,
            vec![
                push_any(GROUP_A),
                p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |groups| {
            p_op(
                BYTE,
                vec![
                    push_value(group_const(groups, GROUP_A) - (group_const(groups, GROUP_B) >> 3)),
                    any(GROUP_X),
                ],
            )
        },
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            (b & U256::from(7u8)).is_zero()
                && a < U256::from(32u8)
                && b <= U256::from(256u16)
                && a >= (b >> 3)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            SHL,
            vec![
                push_any(GROUP_A),
                p_op(SIGNEXTEND, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            p_op(
                SIGNEXTEND,
                vec![
                    push_value((a >> 3) + b),
                    p_op(SHL, vec![push_any(GROUP_A), any(GROUP_X)]),
                ],
            )
        },
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            (a & U256::from(7u8)).is_zero() && a <= U256::from(256u16) && b <= U256::from(32u8)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            SIGNEXTEND,
            vec![
                push_any(GROUP_A),
                p_op(SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |_| p_op(SAR, vec![push_any(GROUP_B), any(GROUP_X)]),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            (b & U256::from(7u8)).is_zero()
                && b <= U256::from(256u16)
                && a <= U256::from(256u16)
                && U256::from((256u16 - b.low_u32() as u16) / 8) == a.overflowing_add(U256::one()).0
        },
    ));
}

fn add_part8_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let x = any(GROUP_X);
    let y = any(GROUP_Y);

    rules.push(rule(p_op(SUB, vec![x.clone(), a.clone()]), |groups| {
        p_op(
            ADD,
            vec![
                any(GROUP_X),
                push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
            ],
        )
    }));
    rules.push(rule(
        p_op(SUB, vec![p_op(ADD, vec![x.clone(), a.clone()]), y.clone()]),
        |_| {
            p_op(
                ADD,
                vec![
                    p_op(SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(SUB, vec![p_op(ADD, vec![a.clone(), x.clone()]), y.clone()]),
        |_| {
            p_op(
                ADD,
                vec![
                    p_op(SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(SUB, vec![x.clone(), p_op(ADD, vec![y.clone(), a.clone()])]),
        |groups| {
            p_op(
                ADD,
                vec![
                    p_op(SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(SUB, vec![x.clone(), p_op(ADD, vec![a.clone(), y.clone()])]),
        |groups| {
            p_op(
                ADD,
                vec![
                    p_op(SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(SUB, vec![p_op(SUB, vec![x.clone(), a.clone()]), y.clone()]),
        |_| {
            p_op(
                SUB,
                vec![
                    p_op(SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(SUB, vec![p_op(SUB, vec![a.clone(), x.clone()]), y.clone()]),
        |_| {
            p_op(
                SUB,
                vec![
                    push_any(GROUP_A),
                    p_op(ADD, vec![any(GROUP_X), any(GROUP_Y)]),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(SUB, vec![x.clone(), p_op(SUB, vec![y.clone(), a.clone()])]),
        |_| {
            p_op(
                ADD,
                vec![
                    p_op(SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(p_op(SUB, vec![x, p_op(SUB, vec![a, y])]), |groups| {
        p_op(
            ADD,
            vec![
                p_op(ADD, vec![any(GROUP_X), any(GROUP_Y)]),
                push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
            ],
        )
    }));
}

fn add_evm_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let x = any(GROUP_X);

    rules.push(gated_rule(
        p_op(BALANCE, vec![p_op(ADDRESS, Vec::new())]),
        |_| p_op(SELFBALANCE, Vec::new()),
        RuleGate::HasSelfBalance,
    ));
    rules.push(rule(p_op(EXP, vec![push_u64(0), x.clone()]), |_| {
        p_op(ISZERO, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(EXP, vec![push_u64(1), x.clone()]), |_| {
        push_u64(1)
    }));
    rules.push(gated_rule(
        p_op(EXP, vec![push_u64(2), x.clone()]),
        |_| p_op(SHL, vec![any(GROUP_X), push_u64(1)]),
        RuleGate::HasBitwiseShifting,
    ));
    rules.push(gated_feasible_rule(
        p_op(MUL, vec![a.clone(), x.clone()]),
        |groups| {
            p_op(
                SHL,
                vec![
                    push_value(U256::from(
                        binary_logarithm(group_const(groups, GROUP_A)).unwrap() as u64,
                    )),
                    any(GROUP_X),
                ],
            )
        },
        |groups| binary_logarithm(group_const(groups, GROUP_A)).is_some(),
        RuleGate::HasBitwiseShifting,
    ));
    rules.push(gated_feasible_rule(
        p_op(MUL, vec![x.clone(), a.clone()]),
        |groups| {
            p_op(
                SHL,
                vec![
                    push_value(U256::from(
                        binary_logarithm(group_const(groups, GROUP_A)).unwrap() as u64,
                    )),
                    any(GROUP_X),
                ],
            )
        },
        |groups| binary_logarithm(group_const(groups, GROUP_A)).is_some(),
        RuleGate::HasBitwiseShifting,
    ));
    rules.push(gated_feasible_rule(
        p_op(DIV, vec![x.clone(), a.clone()]),
        |groups| {
            p_op(
                SHR,
                vec![
                    push_value(U256::from(
                        binary_logarithm(group_const(groups, GROUP_A)).unwrap() as u64,
                    )),
                    any(GROUP_X),
                ],
            )
        },
        |groups| binary_logarithm(group_const(groups, GROUP_A)).is_some(),
        RuleGate::HasBitwiseShifting,
    ));
    rules.push(rule(p_op(EXP, vec![push_value(all_ones()), x]), |_| {
        p_op(
            SUB,
            vec![
                p_op(ISZERO, vec![p_op(AND, vec![any(GROUP_X), push_u64(1)])]),
                p_op(AND, vec![any(GROUP_X), push_u64(1)]),
            ],
        )
    }));
}
