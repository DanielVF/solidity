use crate::bridge::ffi;
use crate::item;
use crate::opcode::{self, op, Effect, EvmVersion};
use crate::u256;
use crate::wire;
use primitive_types::{U256, U512};
use sha3::{Digest, Keccak256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

type Id = u32;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExpressionKey {
    item: ExpressionItemKey,
    arguments: Vec<Id>,
    sequence_number: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ExpressionItemKey {
    Operation {
        opcode: u8,
        verbatim_arguments: u64,
        verbatim_return_values: u64,
    },
    Data {
        kind: u8,
        data: [u8; wire::U256_BYTES],
        verbatim_arguments: u64,
        verbatim_return_values: u64,
    },
    DataBytes {
        kind: u8,
        data: Vec<u8>,
        verbatim_arguments: u64,
        verbatim_return_values: u64,
    },
    Verbatim {
        data: Vec<u8>,
        verbatim_data: Vec<u8>,
        arguments: u64,
        return_values: u64,
    },
}

#[derive(Debug, Clone)]
struct Expression {
    id: Id,
    item: ffi::WireAssemblyItem,
    arguments: Vec<Id>,
    sequence_number: u32,
}

#[derive(Debug, Clone, Copy)]
struct ExpressionView<'a> {
    id: Id,
    item: &'a ffi::WireAssemblyItem,
    arguments: &'a [Id],
}

impl<'a> From<&'a Expression> for ExpressionView<'a> {
    fn from(expression: &'a Expression) -> Self {
        Self {
            id: expression.id,
            item: &expression.item,
            arguments: &expression.arguments,
        }
    }
}

type MatchGroup = u8;

const GROUP_A: MatchGroup = 1;
const GROUP_B: MatchGroup = 2;
const GROUP_C: MatchGroup = 3;
const GROUP_X: MatchGroup = 5;
const GROUP_Y: MatchGroup = 6;
const GROUP_Z: MatchGroup = 7;
const MATCH_GROUP_CAPACITY: usize = GROUP_Z as usize + 1;

#[derive(Debug, Clone, Copy)]
struct MatchedExpression {
    id: Id,
    constant: Option<U256>,
}

#[derive(Debug, Clone)]
struct MatchGroups {
    entries: [Option<MatchedExpression>; MATCH_GROUP_CAPACITY],
}

impl MatchGroups {
    fn new() -> Self {
        Self {
            entries: [None; MATCH_GROUP_CAPACITY],
        }
    }

    fn get(&self, group: MatchGroup) -> Option<&MatchedExpression> {
        self.entries.get(group as usize).and_then(Option::as_ref)
    }

    fn insert(&mut self, group: MatchGroup, expression: MatchedExpression) {
        debug_assert!((group as usize) < MATCH_GROUP_CAPACITY);
        self.entries[group as usize] = Some(expression);
    }
}

#[derive(Debug, Clone)]
enum PatternKind {
    Any,
    PushAny,
    PushValue {
        value: U256,
        bytes: [u8; wire::U256_BYTES],
    },
    Operation(u8),
}

#[derive(Debug, Clone)]
struct Pattern {
    kind: PatternKind,
    arguments: Vec<Pattern>,
    match_group: Option<MatchGroup>,
}

impl Pattern {
    fn any(group: MatchGroup) -> Self {
        Self {
            kind: PatternKind::Any,
            arguments: Vec::new(),
            match_group: Some(group),
        }
    }

    fn push_any(group: MatchGroup) -> Self {
        Self {
            kind: PatternKind::PushAny,
            arguments: Vec::new(),
            match_group: Some(group),
        }
    }

    fn push_value(value: U256) -> Self {
        let mut bytes = [0u8; wire::U256_BYTES];
        value.to_big_endian(&mut bytes);
        Self {
            kind: PatternKind::PushValue { value, bytes },
            arguments: Vec::new(),
            match_group: None,
        }
    }

    fn operation(opcode: u8, arguments: Vec<Pattern>) -> Self {
        Self {
            kind: PatternKind::Operation(opcode),
            arguments,
            match_group: None,
        }
    }

    fn root_opcode(&self) -> u8 {
        match &self.kind {
            PatternKind::Operation(opcode) => *opcode,
            _ => panic!("simplification rule root must be an operation"),
        }
    }

    fn matches(
        &self,
        expression: ExpressionView<'_>,
        classes: &ExpressionClasses,
        groups: &mut MatchGroups,
    ) -> bool {
        if !self.matches_base_item(expression.item) {
            return false;
        }
        if let Some(group) = self.match_group {
            if let Some(existing) = groups.get(group) {
                if existing.id != expression.id {
                    return false;
                }
            } else {
                groups.insert(
                    group,
                    MatchedExpression {
                        id: expression.id,
                        constant: push_constant(expression.item),
                    },
                );
            }
        }
        if !self.arguments.is_empty() {
            if expression.arguments.len() != self.arguments.len() {
                return false;
            }
            for (pattern, argument) in self.arguments.iter().zip(expression.arguments) {
                if !pattern.matches(classes.representative(*argument).into(), classes, groups) {
                    return false;
                }
            }
        }
        true
    }

    fn matches_base_item(&self, item: &ffi::WireAssemblyItem) -> bool {
        match &self.kind {
            PatternKind::Any => true,
            PatternKind::PushAny => item.kind == wire::KIND_PUSH,
            PatternKind::PushValue { bytes, .. } => {
                item.kind == wire::KIND_PUSH && item.data.as_slice() == &bytes[..]
            }
            PatternKind::Operation(opcode) => {
                item.kind == wire::KIND_OPERATION && item.opcode == *opcode
            }
        }
    }
}

struct SimplificationRule {
    root_opcode: u8,
    pattern: Pattern,
    action: Box<dyn Fn(&MatchGroups) -> Pattern + Send + Sync>,
    feasible: Option<Box<dyn Fn(&MatchGroups) -> bool + Send + Sync>>,
}

#[derive(Debug, Clone)]
struct ExpressionClasses {
    representatives: Vec<Expression>,
    expressions: HashMap<ExpressionKey, Id>,
}

impl ExpressionClasses {
    fn with_capacity(item_capacity_hint: usize) -> Self {
        Self {
            representatives: Vec::new(),
            expressions: HashMap::with_capacity(expression_capacity(item_capacity_hint)),
        }
    }

    fn representative(&self, id: Id) -> &Expression {
        &self.representatives[id as usize]
    }

    fn find_item(&mut self, item: ffi::WireAssemblyItem) -> Id {
        self.find(item, Vec::new(), 0)
    }

    fn find(
        &mut self,
        item: ffi::WireAssemblyItem,
        mut arguments: Vec<Id>,
        sequence_number: u32,
    ) -> Id {
        if is_commutative_item(&item) {
            arguments.sort_unstable();
        }

        let key = expression_key(&item, &arguments, sequence_number);
        if is_deterministic_item(&item) {
            if let Some(id) = self.expressions.get(&key).copied() {
                return id;
            }
        }

        let simplified = self.try_simplify(&item, &arguments, sequence_number);
        let id = if let Some(id) = simplified {
            id
        } else {
            let id = self.representatives.len() as Id;
            self.representatives.push(Expression {
                id,
                item: item.clone(),
                arguments: arguments.clone(),
                sequence_number,
            });
            id
        };

        self.expressions.insert(key, id);
        id
    }

    fn new_class(&mut self, debug_data_id: u64) -> Id {
        let id = self.representatives.len() as Id;
        let value = (U256::one() << 255) + U256::from(id);
        let item = ffi::WireAssemblyItem {
            kind: wire::KIND_UNDEFINED,
            opcode: 0,
            data: u256::to_be_bytes(value),
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
        };
        let expression = Expression {
            id,
            item: item.clone(),
            arguments: Vec::new(),
            sequence_number: 0,
        };
        self.expressions
            .insert(expression_key(&item, &[], 0), expression.id);
        self.representatives.push(expression);
        id
    }

    fn known_constant(&self, id: Id) -> Option<U256> {
        let expression = self.representative(id);
        if expression.item.kind == wire::KIND_PUSH && expression.arguments.is_empty() {
            u256::from_be_bytes(&expression.item.data)
        } else {
            None
        }
    }

    fn known_zero(&self, id: Id) -> bool {
        self.known_constant(id).is_some_and(|value| value.is_zero())
    }

    fn known_non_zero(&mut self, id: Id) -> bool {
        if self
            .known_constant(id)
            .is_some_and(|value| !value.is_zero())
        {
            return true;
        }
        let debug_data_id = self.representative(id).item.debug_data_id;
        let iszero = self.find(item::operation(op::ISZERO, debug_data_id), vec![id], 0);
        self.known_zero(iszero)
    }

    fn known_to_be_different(&mut self, a: Id, b: Id) -> bool {
        let debug_data_id = self.representative(a).item.debug_data_id;
        let diff = self.find(item::operation(op::SUB, debug_data_id), vec![a, b], 0);
        self.known_non_zero(diff)
    }

    fn known_to_be_different_by_32(&mut self, a: Id, b: Id) -> bool {
        let debug_data_id = self.representative(a).item.debug_data_id;
        let diff = self.find(item::operation(op::SUB, debug_data_id), vec![a, b], 0);
        self.known_constant(diff)
            .is_some_and(|value| value.overflowing_add(U256::from(31u8)).0 > U256::from(62u8))
    }

    fn try_simplify(
        &mut self,
        item: &ffi::WireAssemblyItem,
        arguments: &[Id],
        sequence_number: u32,
    ) -> Option<Id> {
        if item.kind != wire::KIND_OPERATION || !opcode::is_deterministic(item.opcode) {
            return None;
        }

        self.simplify_by_rule_list(item, arguments, sequence_number)
    }

    fn simplify_by_rule_list(
        &mut self,
        item: &ffi::WireAssemblyItem,
        arguments: &[Id],
        _sequence_number: u32,
    ) -> Option<Id> {
        let expression = ExpressionView {
            id: Id::MAX,
            item,
            arguments,
        };

        for rule in simplification_rules() {
            if rule.root_opcode != item.opcode {
                continue;
            }
            let mut groups = MatchGroups::new();
            if !rule.pattern.matches(expression, self, &mut groups) {
                continue;
            }
            if rule
                .feasible
                .as_ref()
                .is_some_and(|feasible| !feasible(&groups))
            {
                continue;
            }
            let replacement = (rule.action)(&groups);
            return Some(self.rebuild_pattern(&replacement, item.debug_data_id, &groups));
        }
        None
    }

    fn rebuild_pattern(
        &mut self,
        pattern: &Pattern,
        debug_data_id: u64,
        groups: &MatchGroups,
    ) -> Id {
        if let Some(group) = pattern.match_group {
            return groups
                .get(group)
                .expect("matched group must exist before rebuilding")
                .id;
        }

        let arguments = pattern
            .arguments
            .iter()
            .map(|argument| self.rebuild_pattern(argument, debug_data_id, groups))
            .collect::<Vec<_>>();
        match &pattern.kind {
            PatternKind::Any | PatternKind::PushAny => {
                panic!("unmatched wildcard cannot be rebuilt")
            }
            PatternKind::PushValue { value, .. } => {
                self.find_item(item::push_value(*value, debug_data_id))
            }
            PatternKind::Operation(opcode) => {
                self.find(item::operation(*opcode, debug_data_id), arguments, 0)
            }
        }
    }
}

fn simplification_rules() -> &'static [SimplificationRule] {
    static RULES: std::sync::OnceLock<Vec<SimplificationRule>> = std::sync::OnceLock::new();
    RULES.get_or_init(build_simplification_rules).as_slice()
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

    rules
}

fn rule<F>(pattern: Pattern, action: F) -> SimplificationRule
where
    F: Fn(&MatchGroups) -> Pattern + Send + Sync + 'static,
{
    SimplificationRule {
        root_opcode: pattern.root_opcode(),
        pattern,
        action: Box::new(action),
        feasible: None,
    }
}

fn feasible_rule<F, G>(pattern: Pattern, action: F, feasible: G) -> SimplificationRule
where
    F: Fn(&MatchGroups) -> Pattern + Send + Sync + 'static,
    G: Fn(&MatchGroups) -> bool + Send + Sync + 'static,
{
    SimplificationRule {
        root_opcode: pattern.root_opcode(),
        pattern,
        action: Box::new(action),
        feasible: Some(Box::new(feasible)),
    }
}

fn any(group: MatchGroup) -> Pattern {
    Pattern::any(group)
}

fn push_any(group: MatchGroup) -> Pattern {
    Pattern::push_any(group)
}

fn push_value(value: U256) -> Pattern {
    Pattern::push_value(value)
}

fn push_u64(value: u64) -> Pattern {
    Pattern::push_value(U256::from(value))
}

fn p_op(opcode: u8, arguments: Vec<Pattern>) -> Pattern {
    Pattern::operation(opcode, arguments)
}

fn push_constant(item: &ffi::WireAssemblyItem) -> Option<U256> {
    (item.kind == wire::KIND_PUSH)
        .then(|| u256::from_be_bytes(&item.data))
        .flatten()
}

fn group_const(groups: &MatchGroups, group: MatchGroup) -> U256 {
    groups
        .get(group)
        .and_then(|matched| matched.constant)
        .expect("constant match group must contain a PUSH value")
}

fn all_ones() -> U256 {
    u256::max_value()
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

fn add_part1_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let b = push_any(GROUP_B);
    let c = push_any(GROUP_C);

    rules.push(rule(p_op(op::ADD, vec![a.clone(), b.clone()]), |groups| {
        push_value(wrapping_add(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::MUL, vec![a.clone(), b.clone()]), |groups| {
        push_value(wrapping_mul(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::SUB, vec![a.clone(), b.clone()]), |groups| {
        push_value(wrapping_sub(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::DIV, vec![a.clone(), b.clone()]), |groups| {
        let divisor = group_const(groups, GROUP_B);
        push_value(if divisor.is_zero() {
            U256::zero()
        } else {
            group_const(groups, GROUP_A) / divisor
        })
    }));
    rules.push(rule(p_op(op::SDIV, vec![a.clone(), b.clone()]), |groups| {
        push_value(u256::signed_div(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::MOD, vec![a.clone(), b.clone()]), |groups| {
        let divisor = group_const(groups, GROUP_B);
        push_value(if divisor.is_zero() {
            U256::zero()
        } else {
            group_const(groups, GROUP_A) % divisor
        })
    }));
    rules.push(rule(p_op(op::SMOD, vec![a.clone(), b.clone()]), |groups| {
        push_value(u256::signed_mod(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::EXP, vec![a.clone(), b.clone()]), |groups| {
        push_value(
            group_const(groups, GROUP_A)
                .overflowing_pow(group_const(groups, GROUP_B))
                .0,
        )
    }));
    rules.push(rule(p_op(op::NOT, vec![a.clone()]), |groups| {
        push_value(!group_const(groups, GROUP_A))
    }));
    rules.push(rule(p_op(op::LT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            group_const(groups, GROUP_A) < group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::GT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            group_const(groups, GROUP_A) > group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::SLT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            u256::signed_cmp(group_const(groups, GROUP_A), group_const(groups, GROUP_B)).is_lt(),
        ))
    }));
    rules.push(rule(p_op(op::SGT, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            u256::signed_cmp(group_const(groups, GROUP_A), group_const(groups, GROUP_B)).is_gt(),
        ))
    }));
    rules.push(rule(p_op(op::EQ, vec![a.clone(), b.clone()]), |groups| {
        push_value(bool_word(
            group_const(groups, GROUP_A) == group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(p_op(op::ISZERO, vec![a.clone()]), |groups| {
        push_value(bool_word(group_const(groups, GROUP_A).is_zero()))
    }));
    rules.push(rule(p_op(op::AND, vec![a.clone(), b.clone()]), |groups| {
        push_value(group_const(groups, GROUP_A) & group_const(groups, GROUP_B))
    }));
    rules.push(rule(p_op(op::OR, vec![a.clone(), b.clone()]), |groups| {
        push_value(group_const(groups, GROUP_A) | group_const(groups, GROUP_B))
    }));
    rules.push(rule(p_op(op::XOR, vec![a.clone(), b.clone()]), |groups| {
        push_value(group_const(groups, GROUP_A) ^ group_const(groups, GROUP_B))
    }));
    rules.push(rule(p_op(op::BYTE, vec![a.clone(), b.clone()]), |groups| {
        push_value(u256::byte(
            group_const(groups, GROUP_A),
            group_const(groups, GROUP_B),
        ))
    }));
    rules.push(rule(
        p_op(op::ADDMOD, vec![a.clone(), b.clone(), c.clone()]),
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
        p_op(op::MULMOD, vec![a.clone(), b.clone(), c.clone()]),
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
        p_op(op::SIGNEXTEND, vec![a.clone(), b.clone()]),
        |groups| {
            push_value(u256::signextend(
                group_const(groups, GROUP_A),
                group_const(groups, GROUP_B),
            ))
        },
    ));
    rules.push(rule(p_op(op::SHL, vec![a.clone(), b.clone()]), |groups| {
        let shift = group_const(groups, GROUP_A);
        push_value(if shift >= U256::from(256u16) {
            U256::zero()
        } else {
            group_const(groups, GROUP_B) << shift.low_u32()
        })
    }));
    rules.push(rule(p_op(op::SHR, vec![a, b]), |groups| {
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

    rules.push(rule(p_op(op::ADD, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::ADD, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::SUB, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::SUB, vec![max.clone(), x.clone()]), |_| {
        p_op(op::NOT, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(op::MUL, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::MUL, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::MUL, vec![x.clone(), one.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::MUL, vec![one.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::MUL, vec![x.clone(), max.clone()]), |_| {
        p_op(op::SUB, vec![push_u64(0), any(GROUP_X)])
    }));
    rules.push(rule(p_op(op::MUL, vec![max.clone(), x.clone()]), |_| {
        p_op(op::SUB, vec![push_u64(0), any(GROUP_X)])
    }));
    rules.push(rule(p_op(op::DIV, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::DIV, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::DIV, vec![x.clone(), one.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::SDIV, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::SDIV, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::SDIV, vec![x.clone(), one.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::AND, vec![x.clone(), max.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::AND, vec![max.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::AND, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::AND, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::OR, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::OR, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::OR, vec![x.clone(), max.clone()]), |_| {
        push_value(all_ones())
    }));
    rules.push(rule(p_op(op::OR, vec![max.clone(), x.clone()]), |_| {
        push_value(all_ones())
    }));
    rules.push(rule(p_op(op::XOR, vec![x.clone(), zero.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::XOR, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::MOD, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::MOD, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::EQ, vec![x.clone(), zero.clone()]), |_| {
        p_op(op::ISZERO, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(op::EQ, vec![zero.clone(), x.clone()]), |_| {
        p_op(op::ISZERO, vec![any(GROUP_X)])
    }));
    rules.push(rule(p_op(op::SHL, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::SHR, vec![zero.clone(), x.clone()]), |_| {
        any(GROUP_X)
    }));
    rules.push(rule(p_op(op::SHL, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::SHR, vec![x.clone(), zero.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::GT, vec![x.clone(), zero.clone()]), |_| {
        p_op(op::ISZERO, vec![p_op(op::ISZERO, vec![any(GROUP_X)])])
    }));
    rules.push(rule(p_op(op::LT, vec![zero.clone(), x.clone()]), |_| {
        p_op(op::ISZERO, vec![p_op(op::ISZERO, vec![any(GROUP_X)])])
    }));
    rules.push(rule(p_op(op::GT, vec![x.clone(), max.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::LT, vec![max, x.clone()]), |_| push_u64(0)));
    rules.push(rule(p_op(op::GT, vec![zero.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::LT, vec![x.clone(), zero]), |_| push_u64(0)));
    rules.push(rule(
        p_op(
            op::AND,
            vec![p_op(op::BYTE, vec![x.clone(), y.clone()]), push_u64(0xff)],
        ),
        |_| p_op(op::BYTE, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(p_op(op::BYTE, vec![push_u64(31), x]), |_| {
        p_op(op::AND, vec![any(GROUP_X), push_u64(0xff)])
    }));
}

fn add_part3_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    for opcode in [op::AND, op::OR] {
        rules.push(rule(p_op(opcode, vec![x.clone(), x.clone()]), move |_| {
            any(GROUP_X)
        }));
    }
    rules.push(rule(p_op(op::XOR, vec![x.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::SUB, vec![x.clone(), x.clone()]), |_| {
        push_u64(0)
    }));
    rules.push(rule(p_op(op::EQ, vec![x.clone(), x.clone()]), |_| {
        push_u64(1)
    }));
    for opcode in [op::LT, op::SLT, op::GT, op::SGT, op::MOD] {
        rules.push(rule(p_op(opcode, vec![x.clone(), x.clone()]), |_| {
            push_u64(0)
        }));
    }
}

fn add_part4_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    let y = any(GROUP_Y);

    rules.push(rule(
        p_op(op::NOT, vec![p_op(op::NOT, vec![x.clone()])]),
        |_| any(GROUP_X),
    ));
    rules.push(rule(
        p_op(
            op::XOR,
            vec![x.clone(), p_op(op::XOR, vec![x.clone(), y.clone()])],
        ),
        |_| any(GROUP_Y),
    ));
    rules.push(rule(
        p_op(
            op::XOR,
            vec![x.clone(), p_op(op::XOR, vec![y.clone(), x.clone()])],
        ),
        |_| any(GROUP_Y),
    ));
    rules.push(rule(
        p_op(
            op::XOR,
            vec![p_op(op::XOR, vec![x.clone(), y.clone()]), x.clone()],
        ),
        |_| any(GROUP_Y),
    ));
    rules.push(rule(
        p_op(
            op::XOR,
            vec![p_op(op::XOR, vec![y.clone(), x.clone()]), x.clone()],
        ),
        |_| any(GROUP_Y),
    ));

    for pattern in [
        p_op(
            op::OR,
            vec![x.clone(), p_op(op::AND, vec![x.clone(), y.clone()])],
        ),
        p_op(
            op::OR,
            vec![x.clone(), p_op(op::AND, vec![y.clone(), x.clone()])],
        ),
        p_op(
            op::OR,
            vec![p_op(op::AND, vec![x.clone(), y.clone()]), x.clone()],
        ),
        p_op(
            op::OR,
            vec![p_op(op::AND, vec![y.clone(), x.clone()]), x.clone()],
        ),
        p_op(
            op::AND,
            vec![x.clone(), p_op(op::OR, vec![x.clone(), y.clone()])],
        ),
        p_op(
            op::AND,
            vec![x.clone(), p_op(op::OR, vec![y.clone(), x.clone()])],
        ),
        p_op(
            op::AND,
            vec![p_op(op::OR, vec![x.clone(), y.clone()]), x.clone()],
        ),
        p_op(
            op::AND,
            vec![p_op(op::OR, vec![y.clone(), x.clone()]), x.clone()],
        ),
    ] {
        rules.push(rule(pattern, |_| any(GROUP_X)));
    }

    rules.push(rule(
        p_op(op::AND, vec![x.clone(), p_op(op::NOT, vec![x.clone()])]),
        |_| push_u64(0),
    ));
    rules.push(rule(
        p_op(op::AND, vec![p_op(op::NOT, vec![x.clone()]), x.clone()]),
        |_| push_u64(0),
    ));
    rules.push(rule(
        p_op(op::OR, vec![x.clone(), p_op(op::NOT, vec![x.clone()])]),
        |_| push_value(all_ones()),
    ));
    rules.push(rule(
        p_op(op::OR, vec![p_op(op::NOT, vec![x]), any(GROUP_X)]),
        |_| push_value(all_ones()),
    ));
}

fn add_part4_5_rules(rules: &mut Vec<SimplificationRule>) {
    let a = push_any(GROUP_A);
    let b = push_any(GROUP_B);
    let x = any(GROUP_X);
    let y = any(GROUP_Y);

    for pattern in [
        p_op(
            op::AND,
            vec![p_op(op::AND, vec![x.clone(), y.clone()]), y.clone()],
        ),
        p_op(
            op::AND,
            vec![y.clone(), p_op(op::AND, vec![x.clone(), y.clone()])],
        ),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(op::AND, vec![any(GROUP_X), any(GROUP_Y)])
        }));
    }
    for pattern in [
        p_op(
            op::AND,
            vec![p_op(op::AND, vec![y.clone(), x.clone()]), y.clone()],
        ),
        p_op(
            op::AND,
            vec![y.clone(), p_op(op::AND, vec![y.clone(), x.clone()])],
        ),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(op::AND, vec![any(GROUP_Y), any(GROUP_X)])
        }));
    }
    for pattern in [
        p_op(
            op::OR,
            vec![p_op(op::OR, vec![x.clone(), y.clone()]), y.clone()],
        ),
        p_op(
            op::OR,
            vec![y.clone(), p_op(op::OR, vec![x.clone(), y.clone()])],
        ),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(op::OR, vec![any(GROUP_X), any(GROUP_Y)])
        }));
    }
    for pattern in [
        p_op(
            op::OR,
            vec![p_op(op::OR, vec![y.clone(), x.clone()]), y.clone()],
        ),
        p_op(op::OR, vec![y, p_op(op::OR, vec![any(GROUP_Y), x.clone()])]),
    ] {
        rules.push(rule(pattern, |_| {
            p_op(op::OR, vec![any(GROUP_Y), any(GROUP_X)])
        }));
    }
    rules.push(rule(
        p_op(
            op::SIGNEXTEND,
            vec![
                x.clone(),
                p_op(op::SIGNEXTEND, vec![x.clone(), any(GROUP_Y)]),
            ],
        ),
        |_| p_op(op::SIGNEXTEND, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(
        p_op(
            op::SIGNEXTEND,
            vec![a, p_op(op::SIGNEXTEND, vec![b, any(GROUP_X)])],
        ),
        |groups| {
            p_op(
                op::SIGNEXTEND,
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

    for bit in 0..256usize {
        let value = U256::one() << bit;
        rules.push(rule(
            p_op(op::MOD, vec![x.clone(), push_value(value)]),
            move |_| {
                p_op(
                    op::AND,
                    vec![
                        any(GROUP_X),
                        push_value(value.overflowing_sub(U256::one()).0),
                    ],
                )
            },
        ));
    }

    rules.push(feasible_rule(
        p_op(op::SHL, vec![a.clone(), x.clone()]),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) >= U256::from(256u16),
    ));
    rules.push(feasible_rule(
        p_op(op::SHR, vec![a.clone(), x.clone()]),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) >= U256::from(256u16),
    ));
    rules.push(feasible_rule(
        p_op(op::BYTE, vec![a.clone(), x.clone()]),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) >= U256::from(32u8),
    ));
    rules.push(feasible_rule(
        p_op(op::SIGNEXTEND, vec![a.clone(), x.clone()]),
        |_| any(GROUP_X),
        |groups| group_const(groups, GROUP_A) >= U256::from(31u8),
    ));

    rules.push(feasible_rule(
        p_op(
            op::AND,
            vec![a.clone(), p_op(op::SIGNEXTEND, vec![b.clone(), x.clone()])],
        ),
        |_| p_op(op::AND, vec![push_any(GROUP_A), any(GROUP_X)]),
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
            op::AND,
            vec![p_op(op::SIGNEXTEND, vec![b.clone(), x.clone()]), a.clone()],
        ),
        |_| p_op(op::AND, vec![push_any(GROUP_A), any(GROUP_X)]),
        |groups| {
            let index = group_const(groups, GROUP_B);
            index < U256::from(31u8)
                && (group_const(groups, GROUP_A)
                    & low_bits_mask((index.low_u32() as usize + 1) * 8))
                    == group_const(groups, GROUP_A)
        },
    ));

    let mask160 = low_bits_mask(160);
    for opcode in [op::ADDRESS, op::CALLER, op::ORIGIN, op::COINBASE] {
        rules.push(rule(
            p_op(op::AND, vec![p_op(opcode, Vec::new()), push_value(mask160)]),
            move |_| p_op(opcode, Vec::new()),
        ));
        rules.push(rule(
            p_op(op::AND, vec![push_value(mask160), p_op(opcode, Vec::new())]),
            move |_| p_op(opcode, Vec::new()),
        ));
    }
}

fn add_part6_rules(rules: &mut Vec<SimplificationRule>) {
    let x = any(GROUP_X);
    let y = any(GROUP_Y);
    for opcode in [op::EQ, op::LT, op::SLT, op::GT, op::SGT] {
        rules.push(rule(
            p_op(
                op::ISZERO,
                vec![p_op(
                    op::ISZERO,
                    vec![p_op(opcode, vec![x.clone(), y.clone()])],
                )],
            ),
            move |_| p_op(opcode, vec![any(GROUP_X), any(GROUP_Y)]),
        ));
    }
    rules.push(rule(
        p_op(
            op::ISZERO,
            vec![p_op(op::ISZERO, vec![p_op(op::ISZERO, vec![x.clone()])])],
        ),
        |_| p_op(op::ISZERO, vec![any(GROUP_X)]),
    ));
    rules.push(rule(
        p_op(op::ISZERO, vec![p_op(op::XOR, vec![x.clone(), y.clone()])]),
        |_| p_op(op::EQ, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(
        p_op(op::ISZERO, vec![p_op(op::SUB, vec![x, y])]),
        |_| p_op(op::EQ, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
}

fn add_part7_rules(rules: &mut Vec<SimplificationRule>) {
    for (opcode, combine) in [
        (op::ADD, wrapping_add as fn(U256, U256) -> U256),
        (op::MUL, wrapping_mul as fn(U256, U256) -> U256),
        (op::AND, (|a, b| a & b) as fn(U256, U256) -> U256),
        (op::OR, (|a, b| a | b) as fn(U256, U256) -> U256),
        (op::XOR, (|a, b| a ^ b) as fn(U256, U256) -> U256),
    ] {
        for op_xa in [
            p_op(opcode, vec![any(GROUP_X), push_any(GROUP_A)]),
            p_op(opcode, vec![push_any(GROUP_A), any(GROUP_X)]),
        ] {
            rules.push(rule(
                p_op(opcode, vec![op_xa.clone(), push_any(GROUP_B)]),
                move |groups| {
                    p_op(
                        opcode,
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
                p_op(opcode, vec![op_xa.clone(), any(GROUP_Y)]),
                move |_| {
                    p_op(
                        opcode,
                        vec![
                            p_op(opcode, vec![any(GROUP_X), any(GROUP_Y)]),
                            push_any(GROUP_A),
                        ],
                    )
                },
            ));
            rules.push(rule(
                p_op(opcode, vec![push_any(GROUP_B), op_xa.clone()]),
                move |groups| {
                    p_op(
                        opcode,
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
            rules.push(rule(p_op(opcode, vec![any(GROUP_Y), op_xa]), move |_| {
                p_op(
                    opcode,
                    vec![
                        p_op(opcode, vec![any(GROUP_Y), any(GROUP_X)]),
                        push_any(GROUP_A),
                    ],
                )
            }));
        }
    }

    rules.push(rule(
        p_op(
            op::SHL,
            vec![
                push_any(GROUP_B),
                p_op(op::SHL, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            if u256_sum_ge_256(a, b) {
                p_op(op::AND, vec![any(GROUP_X), push_u64(0)])
            } else {
                p_op(op::SHL, vec![push_value(a + b), any(GROUP_X)])
            }
        },
    ));
    rules.push(rule(
        p_op(
            op::SHR,
            vec![
                push_any(GROUP_B),
                p_op(op::SHR, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            if u256_sum_ge_256(a, b) {
                p_op(op::AND, vec![any(GROUP_X), push_u64(0)])
            } else {
                p_op(op::SHR, vec![push_value(a + b), any(GROUP_X)])
            }
        },
    ));

    rules.push(feasible_rule(
        p_op(
            op::SHR,
            vec![
                push_any(GROUP_B),
                p_op(op::SHL, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            let mask = (all_ones() << a.low_u32()) >> b.low_u32();
            if a > b {
                p_op(
                    op::AND,
                    vec![
                        p_op(op::SHL, vec![push_value(a - b), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else if b > a {
                p_op(
                    op::AND,
                    vec![
                        p_op(op::SHR, vec![push_value(b - a), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else {
                p_op(op::AND, vec![any(GROUP_X), push_value(mask)])
            }
        },
        |groups| {
            group_const(groups, GROUP_A) < U256::from(256u16)
                && group_const(groups, GROUP_B) < U256::from(256u16)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            op::SHL,
            vec![
                push_any(GROUP_B),
                p_op(op::SHR, vec![push_any(GROUP_A), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            let mask = (all_ones() >> a.low_u32()) << b.low_u32();
            if a > b {
                p_op(
                    op::AND,
                    vec![
                        p_op(op::SHR, vec![push_value(a - b), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else if b > a {
                p_op(
                    op::AND,
                    vec![
                        p_op(op::SHL, vec![push_value(b - a), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            } else {
                p_op(op::AND, vec![any(GROUP_X), push_value(mask)])
            }
        },
        |groups| {
            group_const(groups, GROUP_A) < U256::from(256u16)
                && group_const(groups, GROUP_B) < U256::from(256u16)
        },
    ));

    for shift_opcode in [op::SHL, op::SHR] {
        rules.push(feasible_rule(
            p_op(
                shift_opcode,
                vec![
                    push_any(GROUP_B),
                    p_op(op::AND, vec![any(GROUP_X), push_any(GROUP_A)]),
                ],
            ),
            move |groups| {
                let mask = if shift_opcode == op::SHL {
                    group_const(groups, GROUP_A) << group_const(groups, GROUP_B).low_u32()
                } else {
                    group_const(groups, GROUP_A) >> group_const(groups, GROUP_B).low_u32()
                };
                p_op(
                    op::AND,
                    vec![
                        p_op(shift_opcode, vec![push_any(GROUP_B), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            },
            |groups| group_const(groups, GROUP_B) < U256::from(256u16),
        ));
        rules.push(feasible_rule(
            p_op(
                shift_opcode,
                vec![
                    push_any(GROUP_B),
                    p_op(op::AND, vec![push_any(GROUP_A), any(GROUP_X)]),
                ],
            ),
            move |groups| {
                let mask = if shift_opcode == op::SHL {
                    group_const(groups, GROUP_A) << group_const(groups, GROUP_B).low_u32()
                } else {
                    group_const(groups, GROUP_A) >> group_const(groups, GROUP_B).low_u32()
                };
                p_op(
                    op::AND,
                    vec![
                        p_op(shift_opcode, vec![push_any(GROUP_B), any(GROUP_X)]),
                        push_value(mask),
                    ],
                )
            },
            |groups| group_const(groups, GROUP_B) < U256::from(256u16),
        ));
    }

    for inner in [
        p_op(op::AND, vec![any(GROUP_X), push_any(GROUP_A)]),
        p_op(op::AND, vec![push_any(GROUP_A), any(GROUP_X)]),
    ] {
        for second in [
            p_op(op::OR, vec![inner.clone(), any(GROUP_Y)]),
            p_op(op::OR, vec![any(GROUP_Y), inner.clone()]),
        ] {
            rules.push(rule(
                p_op(op::AND, vec![second.clone(), push_any(GROUP_B)]),
                |groups| {
                    p_op(
                        op::OR,
                        vec![
                            p_op(
                                op::AND,
                                vec![
                                    any(GROUP_X),
                                    push_value(
                                        group_const(groups, GROUP_A) & group_const(groups, GROUP_B),
                                    ),
                                ],
                            ),
                            p_op(op::AND, vec![any(GROUP_Y), push_any(GROUP_B)]),
                        ],
                    )
                },
            ));
            rules.push(rule(
                p_op(op::AND, vec![push_any(GROUP_B), second]),
                |groups| {
                    p_op(
                        op::OR,
                        vec![
                            p_op(
                                op::AND,
                                vec![
                                    any(GROUP_X),
                                    push_value(
                                        group_const(groups, GROUP_A) & group_const(groups, GROUP_B),
                                    ),
                                ],
                            ),
                            p_op(op::AND, vec![any(GROUP_Y), push_any(GROUP_B)]),
                        ],
                    )
                },
            ));
        }
    }

    rules.push(rule(
        p_op(
            op::MUL,
            vec![any(GROUP_X), p_op(op::SHL, vec![any(GROUP_Y), push_u64(1)])],
        ),
        |_| p_op(op::SHL, vec![any(GROUP_Y), any(GROUP_X)]),
    ));
    rules.push(rule(
        p_op(
            op::MUL,
            vec![p_op(op::SHL, vec![any(GROUP_X), push_u64(1)]), any(GROUP_Y)],
        ),
        |_| p_op(op::SHL, vec![any(GROUP_X), any(GROUP_Y)]),
    ));
    rules.push(rule(
        p_op(
            op::DIV,
            vec![any(GROUP_X), p_op(op::SHL, vec![any(GROUP_Y), push_u64(1)])],
        ),
        |_| p_op(op::SHR, vec![any(GROUP_Y), any(GROUP_X)]),
    ));

    rules.push(feasible_rule(
        p_op(
            op::AND,
            vec![
                push_any(GROUP_A),
                p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |_| p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
        |groups| {
            let b = group_const(groups, GROUP_B);
            b <= U256::from(256u16)
                && (group_const(groups, GROUP_A) & full_mask_shr(b)) == full_mask_shr(b)
        },
    ));
    rules.push(feasible_rule(
        p_op(
            op::AND,
            vec![
                p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
                push_any(GROUP_A),
            ],
        ),
        |_| p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
        |groups| {
            let b = group_const(groups, GROUP_B);
            b <= U256::from(256u16)
                && (group_const(groups, GROUP_A) & full_mask_shr(b)) == full_mask_shr(b)
        },
    ));
    rules.push(rule(
        p_op(
            op::AND,
            vec![
                p_op(op::SHL, vec![any(GROUP_Z), any(GROUP_X)]),
                p_op(op::SHL, vec![any(GROUP_Z), any(GROUP_Y)]),
            ],
        ),
        |_| {
            p_op(
                op::SHL,
                vec![
                    any(GROUP_Z),
                    p_op(op::AND, vec![any(GROUP_X), any(GROUP_Y)]),
                ],
            )
        },
    ));

    rules.push(feasible_rule(
        p_op(
            op::BYTE,
            vec![
                push_any(GROUP_A),
                p_op(op::SHL, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |groups| {
            p_op(
                op::BYTE,
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
            op::BYTE,
            vec![
                push_any(GROUP_A),
                p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |_| push_u64(0),
        |groups| group_const(groups, GROUP_A) < (group_const(groups, GROUP_B) >> 3),
    ));
    rules.push(feasible_rule(
        p_op(
            op::BYTE,
            vec![
                push_any(GROUP_A),
                p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |groups| {
            p_op(
                op::BYTE,
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
            op::SHL,
            vec![
                push_any(GROUP_A),
                p_op(op::SIGNEXTEND, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |groups| {
            let a = group_const(groups, GROUP_A);
            let b = group_const(groups, GROUP_B);
            p_op(
                op::SIGNEXTEND,
                vec![
                    push_value((a >> 3) + b),
                    p_op(op::SHL, vec![push_any(GROUP_A), any(GROUP_X)]),
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
            op::SIGNEXTEND,
            vec![
                push_any(GROUP_A),
                p_op(op::SHR, vec![push_any(GROUP_B), any(GROUP_X)]),
            ],
        ),
        |_| p_op(op::SAR, vec![push_any(GROUP_B), any(GROUP_X)]),
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

    rules.push(rule(p_op(op::SUB, vec![x.clone(), a.clone()]), |groups| {
        p_op(
            op::ADD,
            vec![
                any(GROUP_X),
                push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
            ],
        )
    }));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![p_op(op::ADD, vec![x.clone(), a.clone()]), y.clone()],
        ),
        |_| {
            p_op(
                op::ADD,
                vec![
                    p_op(op::SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![p_op(op::ADD, vec![a.clone(), x.clone()]), y.clone()],
        ),
        |_| {
            p_op(
                op::ADD,
                vec![
                    p_op(op::SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![x.clone(), p_op(op::ADD, vec![y.clone(), a.clone()])],
        ),
        |groups| {
            p_op(
                op::ADD,
                vec![
                    p_op(op::SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![x.clone(), p_op(op::ADD, vec![a.clone(), y.clone()])],
        ),
        |groups| {
            p_op(
                op::ADD,
                vec![
                    p_op(op::SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![p_op(op::SUB, vec![x.clone(), a.clone()]), y.clone()],
        ),
        |_| {
            p_op(
                op::SUB,
                vec![
                    p_op(op::SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![p_op(op::SUB, vec![a.clone(), x.clone()]), y.clone()],
        ),
        |_| {
            p_op(
                op::SUB,
                vec![
                    push_any(GROUP_A),
                    p_op(op::ADD, vec![any(GROUP_X), any(GROUP_Y)]),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(
            op::SUB,
            vec![x.clone(), p_op(op::SUB, vec![y.clone(), a.clone()])],
        ),
        |_| {
            p_op(
                op::ADD,
                vec![
                    p_op(op::SUB, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_any(GROUP_A),
                ],
            )
        },
    ));
    rules.push(rule(
        p_op(op::SUB, vec![x, p_op(op::SUB, vec![a, y])]),
        |groups| {
            p_op(
                op::ADD,
                vec![
                    p_op(op::ADD, vec![any(GROUP_X), any(GROUP_Y)]),
                    push_value(U256::zero().overflowing_sub(group_const(groups, GROUP_A)).0),
                ],
            )
        },
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StoreTarget {
    Memory,
    Storage,
}

#[derive(Debug, Clone, Copy)]
struct StoreOperation {
    target: StoreTarget,
    slot: Id,
    sequence_number: u32,
    expression: Id,
}

#[derive(Debug, Clone)]
struct KnownState {
    stack_height: i32,
    stack_elements: BTreeMap<i32, Id>,
    sequence_number: u32,
    storage_content: BTreeMap<Id, Id>,
    memory_content: BTreeMap<Id, Id>,
    known_keccak256_hashes: BTreeMap<(Vec<Id>, u32), Id>,
    expression_classes: Rc<RefCell<ExpressionClasses>>,
}

impl KnownState {
    fn with_expression_capacity(item_capacity_hint: usize) -> Self {
        Self {
            stack_height: 0,
            stack_elements: BTreeMap::new(),
            sequence_number: 1,
            storage_content: BTreeMap::new(),
            memory_content: BTreeMap::new(),
            known_keccak256_hashes: BTreeMap::new(),
            expression_classes: Rc::new(RefCell::new(ExpressionClasses::with_capacity(
                item_capacity_hint,
            ))),
        }
    }

    fn feed_item(&mut self, item: &ffi::WireAssemblyItem) -> Option<StoreOperation> {
        if item.kind == wire::KIND_TAG {
            return None;
        }

        if item.kind == wire::KIND_ASSIGN_IMMUTABLE {
            self.feed_item(&item::operation(op::POP, item.debug_data_id));
            return self.feed_item(&item::operation(op::POP, item.debug_data_id));
        }

        if item.kind == wire::KIND_VERBATIM_BYTECODE {
            self.sequence_number += 2;
            self.reset_memory();
            self.reset_known_keccak256_hashes();
            self.reset_storage();
            let cutoff = self.stack_height - item::arguments(item) as i32;
            self.stack_elements = self
                .stack_elements
                .iter()
                .filter_map(|(height, id)| (*height <= cutoff).then_some((*height, *id)))
                .collect();
            self.stack_height += item::deposit(item);
            for offset in 0..item::return_values(item) {
                let id = self
                    .expression_classes
                    .borrow_mut()
                    .new_class(item.debug_data_id);
                self.set_stack_element(self.stack_height - offset as i32, id);
            }
            return None;
        }

        if item.kind != wire::KIND_OPERATION {
            let id = if item.has_pushed_value {
                let pushed_value = u256::from_be_bytes(&item.pushed_value).unwrap_or_default();
                self.expression_classes
                    .borrow_mut()
                    .find_item(item::push_value(pushed_value, item.debug_data_id))
            } else {
                self.expression_classes.borrow_mut().find_item(item.clone())
            };
            self.stack_height += 1;
            self.set_stack_element(self.stack_height, id);
            return None;
        }

        let info = opcode::instruction_info(item.opcode);
        let mut store_operation = None;

        if opcode::is_dup(item.opcode) {
            let depth = opcode::dup_number(item.opcode) as i32;
            let id = self.stack_element(self.stack_height - (depth - 1), item.debug_data_id);
            self.set_stack_element(self.stack_height + 1, id);
        } else if opcode::is_swap(item.opcode) {
            let depth = opcode::swap_number(item.opcode) as i32;
            self.swap_stack_elements(
                self.stack_height,
                self.stack_height - 1 - (depth - 1),
                item.debug_data_id,
            );
        } else if item.opcode != op::POP {
            let arguments = (0..info.args)
                .map(|index| {
                    self.stack_element(self.stack_height - index as i32, item.debug_data_id)
                })
                .collect::<Vec<_>>();
            match item.opcode {
                op::SSTORE => {
                    store_operation =
                        self.store_in_storage(arguments[0], arguments[1], item.debug_data_id);
                }
                op::SLOAD => {
                    let id = self.load_from_storage(arguments[0], item.debug_data_id);
                    self.set_stack_element(self.stack_height + item::deposit(item), id);
                }
                op::MSTORE => {
                    store_operation =
                        self.store_in_memory(arguments[0], arguments[1], item.debug_data_id);
                }
                op::MLOAD => {
                    let id = self.load_from_memory(arguments[0], item.debug_data_id);
                    self.set_stack_element(self.stack_height + item::deposit(item), id);
                }
                op::KECCAK256 => {
                    let id = self.apply_keccak256(arguments[0], arguments[1], item.debug_data_id);
                    self.set_stack_element(self.stack_height + item::deposit(item), id);
                }
                _ => {
                    let invalidates_memory = opcode::memory_effect(item.opcode) == Effect::Write;
                    let invalidates_storage = opcode::storage_effect(item.opcode) == Effect::Write;
                    if invalidates_memory {
                        self.reset_memory();
                        self.reset_known_keccak256_hashes();
                    }
                    if invalidates_storage {
                        self.reset_storage();
                    }
                    if invalidates_memory || invalidates_storage {
                        self.sequence_number += 2;
                    }
                    if info.ret == 1 {
                        let id =
                            self.expression_classes
                                .borrow_mut()
                                .find(item.clone(), arguments, 0);
                        self.set_stack_element(self.stack_height + item::deposit(item), id);
                    }
                }
            }
        }

        let new_height = self.stack_height + item::deposit(item);
        self.stack_elements = self
            .stack_elements
            .iter()
            .filter_map(|(height, id)| (*height <= new_height).then_some((*height, *id)))
            .collect();
        self.stack_height = new_height;
        store_operation
    }

    fn stack_element(&mut self, stack_height: i32, debug_data_id: u64) -> Id {
        if let Some(id) = self.stack_elements.get(&stack_height).copied() {
            return id;
        }
        let id = self
            .expression_classes
            .borrow_mut()
            .find_item(undefined_stack_item(stack_height, debug_data_id));
        self.stack_elements.insert(stack_height, id);
        id
    }

    fn set_stack_element(&mut self, stack_height: i32, id: Id) {
        self.stack_elements.insert(stack_height, id);
    }

    fn swap_stack_elements(&mut self, a: i32, b: i32, debug_data_id: u64) {
        if a == b {
            return;
        }
        let a_id = self.stack_element(a, debug_data_id);
        let b_id = self.stack_element(b, debug_data_id);
        self.stack_elements.insert(a, b_id);
        self.stack_elements.insert(b, a_id);
    }

    fn reset_storage(&mut self) {
        self.storage_content.clear();
    }

    fn reset_memory(&mut self) {
        self.memory_content.clear();
    }

    fn reset_known_keccak256_hashes(&mut self) {
        self.known_keccak256_hashes.clear();
    }

    fn store_in_storage(
        &mut self,
        slot: Id,
        value: Id,
        debug_data_id: u64,
    ) -> Option<StoreOperation> {
        if self.storage_content.get(&slot) == Some(&value) {
            return None;
        }
        self.sequence_number += 1;
        let previous = std::mem::take(&mut self.storage_content);
        self.storage_content = previous
            .into_iter()
            .filter(|(known_slot, known_value)| {
                self.expression_classes
                    .borrow_mut()
                    .known_to_be_different(*known_slot, slot)
                    || *known_value == value
            })
            .collect();
        let expression = self.expression_classes.borrow_mut().find(
            item::operation(op::SSTORE, debug_data_id),
            vec![slot, value],
            self.sequence_number,
        );
        let operation = StoreOperation {
            target: StoreTarget::Storage,
            slot,
            sequence_number: self.sequence_number,
            expression,
        };
        self.storage_content.insert(slot, value);
        self.sequence_number += 1;
        Some(operation)
    }

    fn load_from_storage(&mut self, slot: Id, debug_data_id: u64) -> Id {
        if let Some(id) = self.storage_content.get(&slot).copied() {
            return id;
        }
        let id = self.expression_classes.borrow_mut().find(
            item::operation(op::SLOAD, debug_data_id),
            vec![slot],
            self.sequence_number,
        );
        self.storage_content.insert(slot, id);
        id
    }

    fn store_in_memory(
        &mut self,
        slot: Id,
        value: Id,
        debug_data_id: u64,
    ) -> Option<StoreOperation> {
        if self.memory_content.get(&slot) == Some(&value) {
            return None;
        }
        self.sequence_number += 1;
        let previous = std::mem::take(&mut self.memory_content);
        self.memory_content = previous
            .into_iter()
            .filter(|(known_slot, _)| {
                self.expression_classes
                    .borrow_mut()
                    .known_to_be_different_by_32(*known_slot, slot)
            })
            .collect();
        let expression = self.expression_classes.borrow_mut().find(
            item::operation(op::MSTORE, debug_data_id),
            vec![slot, value],
            self.sequence_number,
        );
        let operation = StoreOperation {
            target: StoreTarget::Memory,
            slot,
            sequence_number: self.sequence_number,
            expression,
        };
        self.memory_content.insert(slot, value);
        self.sequence_number += 1;
        Some(operation)
    }

    fn load_from_memory(&mut self, slot: Id, debug_data_id: u64) -> Id {
        if let Some(id) = self.memory_content.get(&slot).copied() {
            return id;
        }
        let id = self.expression_classes.borrow_mut().find(
            item::operation(op::MLOAD, debug_data_id),
            vec![slot],
            self.sequence_number,
        );
        self.memory_content.insert(slot, id);
        id
    }

    fn apply_keccak256(&mut self, start: Id, length: Id, debug_data_id: u64) -> Id {
        let keccak_item = item::operation(op::KECCAK256, debug_data_id);
        let Some(length_value) = self.expression_classes.borrow().known_constant(length) else {
            return self.expression_classes.borrow_mut().find(
                keccak_item,
                vec![start, length],
                self.sequence_number,
            );
        };
        if length_value > U256::from(128u16) {
            return self.expression_classes.borrow_mut().find(
                keccak_item,
                vec![start, length],
                self.sequence_number,
            );
        }

        let length_u32 = length_value.low_u32();
        let mut arguments = Vec::new();
        for offset in (0..length_u32).step_by(32) {
            let offset_id = self
                .expression_classes
                .borrow_mut()
                .find_item(item::push_value(U256::from(offset), debug_data_id));
            let slot = self.expression_classes.borrow_mut().find(
                item::operation(op::ADD, debug_data_id),
                vec![start, offset_id],
                0,
            );
            arguments.push(self.load_from_memory(slot, debug_data_id));
        }

        if let Some(id) = self
            .known_keccak256_hashes
            .get(&(arguments.clone(), length_u32))
            .copied()
        {
            return id;
        }

        let id = if arguments.iter().all(|argument| {
            self.expression_classes
                .borrow()
                .known_constant(*argument)
                .is_some()
        }) {
            let mut data = Vec::new();
            for argument in &arguments {
                data.extend(u256::to_be_bytes(
                    self.expression_classes
                        .borrow()
                        .known_constant(*argument)
                        .expect("checked above"),
                ));
            }
            data.truncate(length_u32 as usize);
            let hash = Keccak256::digest(data);
            self.expression_classes
                .borrow_mut()
                .find_item(item::push_value(
                    U256::from_big_endian(&hash),
                    debug_data_id,
                ))
        } else {
            self.expression_classes.borrow_mut().find(
                keccak_item,
                vec![start, length],
                self.sequence_number,
            )
        };
        self.known_keccak256_hashes
            .insert((arguments, length_u32), id);
        id
    }
}

#[derive(Debug)]
enum CseError {
    ItemNotAvailable,
    StackTooDeep,
    InvalidState,
}

struct CommonSubexpressionEliminator {
    initial_state: KnownState,
    state: KnownState,
    store_operations: Vec<StoreOperation>,
    breaking_item: Option<ffi::WireAssemblyItem>,
    evm_version: EvmVersion,
}

impl CommonSubexpressionEliminator {
    fn new(evm_version: EvmVersion, item_capacity_hint: usize) -> Self {
        let state = KnownState::with_expression_capacity(item_capacity_hint);
        Self {
            initial_state: state.clone(),
            state,
            store_operations: Vec::new(),
            breaking_item: None,
            evm_version,
        }
    }

    fn feed_items(
        &mut self,
        items: &[ffi::WireAssemblyItem],
        mut index: usize,
        msize_important: bool,
    ) -> usize {
        let mut chunk_size = 0usize;
        while index < items.len()
            && !breaks_cse_analysis_block(&items[index], msize_important)
            && chunk_size < 2000
        {
            if let Some(operation) = self.state.feed_item(&items[index]) {
                self.store_operations.push(operation);
            }
            index += 1;
            chunk_size += 1;
        }
        if index < items.len() && chunk_size < 2000 {
            self.breaking_item = Some(items[index].clone());
            index += 1;
        }
        index
    }

    fn optimized_items(mut self) -> Result<Vec<ffi::WireAssemblyItem>, CseError> {
        self.optimize_breaking_item();

        let mut next_initial_state = self.state.clone();
        if let Some(breaking_item) = &self.breaking_item {
            next_initial_state.feed_item(breaking_item);
        }
        let _next_state = next_initial_state.clone();

        let mut initial_stack = BTreeMap::new();
        let mut target_stack = BTreeMap::new();
        let mut min_height = self.state.stack_height + 1;
        if let Some(first_height) = self.state.stack_elements.keys().next().copied() {
            min_height = std::cmp::min(min_height, first_height);
        }
        for height in min_height..=self.initial_state.stack_height {
            let id = self
                .initial_state
                .stack_elements
                .get(&height)
                .copied()
                .unwrap_or_else(|| {
                    self.state
                        .expression_classes
                        .borrow_mut()
                        .find_item(undefined_stack_item(height, 0))
                });
            initial_stack.insert(height, id);
        }
        for height in min_height..=self.state.stack_height {
            let id = self.state.stack_element(height, 0);
            target_stack.insert(height, id);
        }

        let mut items = CSECodeGenerator::new(
            self.state.expression_classes.clone(),
            self.store_operations,
            self.evm_version,
        )
        .generate_code(
            self.initial_state.sequence_number,
            self.initial_state.stack_height,
            initial_stack,
            target_stack,
        )?;
        if let Some(breaking_item) = self.breaking_item {
            items.push(breaking_item);
        }
        Ok(items)
    }

    fn optimize_breaking_item(&mut self) {
        let Some(breaking_item) = self.breaking_item.clone() else {
            return;
        };
        let debug_data_id = breaking_item.debug_data_id;
        if item::is_operation(&breaking_item, op::JUMPI) {
            let condition = self
                .state
                .stack_element(self.state.stack_height - 1, debug_data_id);
            if self
                .state
                .expression_classes
                .borrow_mut()
                .known_non_zero(condition)
            {
                self.state
                    .feed_item(&item::operation(op::SWAP1, debug_data_id));
                self.state
                    .feed_item(&item::operation(op::POP, debug_data_id));
                let mut jump = item::operation(op::JUMP, debug_data_id);
                jump.jump_type = breaking_item.jump_type;
                self.breaking_item = Some(jump);
            } else if self.state.expression_classes.borrow().known_zero(condition) {
                let pop = item::operation(op::POP, debug_data_id);
                self.state.feed_item(&pop);
                self.state.feed_item(&pop);
                self.breaking_item = None;
            }
        } else if item::is_operation(&breaking_item, op::RETURN) {
            let size = self
                .state
                .stack_element(self.state.stack_height - 1, debug_data_id);
            if self.state.expression_classes.borrow().known_zero(size) {
                let pop = item::operation(op::POP, debug_data_id);
                self.state.feed_item(&pop);
                self.state.feed_item(&pop);
                self.breaking_item = Some(item::operation(op::STOP, debug_data_id));
            }
        }
    }
}

struct CSECodeGenerator {
    generated_items: Vec<ffi::WireAssemblyItem>,
    stack_height: i32,
    needed_by: HashMap<Id, Vec<Id>>,
    dependencies_added: HashSet<Id>,
    stack: BTreeMap<i32, Id>,
    class_positions: BTreeMap<Id, BTreeSet<i32>>,
    expression_classes: Rc<RefCell<ExpressionClasses>>,
    store_operations: BTreeMap<(StoreTarget, Id), Vec<StoreOperation>>,
    final_classes: BTreeSet<Id>,
    target_stack: BTreeMap<i32, Id>,
    evm_version: EvmVersion,
}

impl CSECodeGenerator {
    fn new(
        expression_classes: Rc<RefCell<ExpressionClasses>>,
        store_operations: Vec<StoreOperation>,
        evm_version: EvmVersion,
    ) -> Self {
        let mut grouped = BTreeMap::<(StoreTarget, Id), Vec<StoreOperation>>::new();
        for operation in store_operations {
            grouped
                .entry((operation.target, operation.slot))
                .or_default()
                .push(operation);
        }
        Self {
            generated_items: Vec::new(),
            stack_height: 0,
            needed_by: HashMap::new(),
            dependencies_added: HashSet::new(),
            stack: BTreeMap::new(),
            class_positions: BTreeMap::new(),
            expression_classes,
            store_operations: grouped,
            final_classes: BTreeSet::new(),
            target_stack: BTreeMap::new(),
            evm_version,
        }
    }

    fn generate_code(
        mut self,
        initial_sequence_number: u32,
        initial_stack_height: i32,
        initial_stack: BTreeMap<i32, Id>,
        target_stack: BTreeMap<i32, Id>,
    ) -> Result<Vec<ffi::WireAssemblyItem>, CseError> {
        self.stack_height = initial_stack_height;
        self.stack = initial_stack.clone();
        self.target_stack = target_stack;
        for (position, id) in &self.stack {
            self.class_positions
                .entry(*id)
                .or_default()
                .insert(*position);
        }

        let latest_store_expressions = self
            .store_operations
            .values()
            .filter_map(|operations| operations.last().map(|operation| operation.expression))
            .collect::<Vec<_>>();
        for expression in latest_store_expressions {
            self.add_dependencies(expression)?;
        }

        let target_ids = self.target_stack.values().copied().collect::<Vec<_>>();
        for id in target_ids {
            self.final_classes.insert(id);
            self.add_dependencies(id)?;
        }

        let mut sequenced = BTreeSet::<(u32, Id)>::new();
        for (needed, needed_by) in &self.needed_by {
            for by in needed_by {
                for id in [*needed, *by] {
                    let sequence = self
                        .expression_classes
                        .borrow()
                        .representative(id)
                        .sequence_number;
                    if sequence != 0 {
                        if sequence < initial_sequence_number {
                            return Err(CseError::StackTooDeep);
                        }
                        sequenced.insert((sequence, id));
                    }
                }
            }
        }

        for (_, id) in sequenced {
            if !self.class_positions.contains_key(&id) {
                self.generate_class_element(id, true)?;
            }
        }

        let targets = self
            .target_stack
            .iter()
            .map(|(position, id)| (*position, *id))
            .collect::<Vec<_>>();
        for (target_position, target_id) in targets {
            if self.stack.get(&target_position) == Some(&target_id) {
                continue;
            }
            self.generate_class_element(target_id, false)?;
            if self
                .class_positions
                .get(&target_id)
                .is_some_and(|positions| positions.contains(&target_position))
            {
                continue;
            }
            let debug_data_id = self
                .expression_classes
                .borrow()
                .representative(target_id)
                .item
                .debug_data_id;
            let position = self.class_element_position(target_id)?;
            if position < target_position {
                self.append_dup(position, debug_data_id)?;
            } else {
                self.append_or_remove_swap(position, debug_data_id)?;
            }
            self.append_or_remove_swap(target_position, debug_data_id)?;
        }

        while self.remove_stack_top_if_possible()? {}

        let final_height = if let Some(last) = self.target_stack.keys().next_back() {
            *last
        } else if let Some(first) = initial_stack.keys().next() {
            *first - 1
        } else {
            initial_stack_height
        };
        if final_height != self.stack_height {
            return Err(CseError::InvalidState);
        }
        Ok(self.generated_items)
    }

    fn add_dependencies(&mut self, id: Id) -> Result<(), CseError> {
        if self.class_positions.contains_key(&id) || !self.dependencies_added.insert(id) {
            return Ok(());
        }
        let expression = self.expression_classes.borrow().representative(id).clone();
        if expression.item.kind == wire::KIND_UNDEFINED {
            return Err(CseError::ItemNotAvailable);
        }
        for argument in &expression.arguments {
            self.add_dependencies(*argument)?;
            self.add_needed_by(*argument, id);
        }

        if expression.item.kind == wire::KIND_OPERATION
            && matches!(
                expression.item.opcode,
                op::SLOAD | op::MLOAD | op::KECCAK256
            )
        {
            let target = if expression.item.opcode == op::SLOAD {
                StoreTarget::Storage
            } else {
                StoreTarget::Memory
            };
            let slot_to_load_from = expression.arguments[0];
            let mut store_dependencies = Vec::new();
            for ((store_target, slot), store_operations) in &self.store_operations {
                if *store_target != target {
                    continue;
                }
                if store_operations
                    .first()
                    .is_some_and(|operation| operation.sequence_number > expression.sequence_number)
                {
                    continue;
                }
                let independent = match expression.item.opcode {
                    op::SLOAD => self
                        .expression_classes
                        .borrow_mut()
                        .known_to_be_different(*slot, slot_to_load_from),
                    op::MLOAD => self
                        .expression_classes
                        .borrow_mut()
                        .known_to_be_different_by_32(*slot, slot_to_load_from),
                    op::KECCAK256 => {
                        let length = expression.arguments[1];
                        let offset_to_start = self.expression_classes.borrow_mut().find(
                            item::operation(op::SUB, expression.item.debug_data_id),
                            vec![*slot, slot_to_load_from],
                            0,
                        );
                        let offset = self
                            .expression_classes
                            .borrow()
                            .known_constant(offset_to_start);
                        let length_constant =
                            self.expression_classes.borrow().known_constant(length);
                        if length_constant == Some(U256::zero()) {
                            true
                        } else if let Some(offset) = offset {
                            if u256::signed_cmp(offset, u256::wrapping_neg(U256::from(32u8)))
                                .is_le()
                            {
                                true
                            } else {
                                length_constant.is_some_and(|length| {
                                    u256::signed_cmp(offset, U256::zero()).is_ge()
                                        && offset >= length
                                })
                            }
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if independent {
                    continue;
                }
                let mut latest_store = store_operations[0].expression;
                for operation in store_operations.iter().skip(1) {
                    if operation.sequence_number < expression.sequence_number {
                        latest_store = operation.expression;
                    }
                }
                store_dependencies.push(latest_store);
            }
            for latest_store in store_dependencies {
                self.add_dependencies(latest_store)?;
                self.add_needed_by(latest_store, id);
            }
        }
        Ok(())
    }

    fn add_needed_by(&mut self, needed: Id, by: Id) {
        self.needed_by.entry(needed).or_default().push(by);
    }

    fn generate_class_element(&mut self, id: Id, allow_sequenced: bool) -> Result<(), CseError> {
        if self.class_positions.values().any(|positions| {
            positions
                .iter()
                .any(|position| *position > self.stack_height)
        }) {
            return Err(CseError::InvalidState);
        }
        self.remove_stack_top_if_possible()?;
        if let Some(positions) = self.class_positions.get(&id) {
            if positions.is_empty() {
                return Err(CseError::InvalidState);
            }
            return Ok(());
        }

        let expression = self.expression_classes.borrow().representative(id).clone();
        if !allow_sequenced && expression.sequence_number != 0 {
            return Err(CseError::InvalidState);
        }
        if expression.item.kind == wire::KIND_UNDEFINED {
            return Err(CseError::ItemNotAvailable);
        }

        for argument in expression.arguments.iter().rev() {
            self.generate_class_element(*argument, false)?;
        }

        let debug_data_id = expression.item.debug_data_id;
        match expression.arguments.as_slice() {
            [] => {}
            [a] => {
                if self.can_be_removed(*a, Some(id), None)? {
                    self.append_or_remove_swap(self.class_element_position(*a)?, debug_data_id)?;
                } else {
                    self.append_dup(self.class_element_position(*a)?, debug_data_id)?;
                }
            }
            [a, b] => {
                if self.can_be_removed(*b, Some(id), None)? {
                    self.append_or_remove_swap(self.class_element_position(*b)?, debug_data_id)?;
                    if a == b {
                        self.append_dup(self.stack_height, debug_data_id)?;
                    } else if self.can_be_removed(*a, Some(id), None)? {
                        self.append_or_remove_swap(self.stack_height - 1, debug_data_id)?;
                        self.append_or_remove_swap(
                            self.class_element_position(*a)?,
                            debug_data_id,
                        )?;
                    } else {
                        self.append_dup(self.class_element_position(*a)?, debug_data_id)?;
                    }
                } else if a == b {
                    self.append_dup(self.class_element_position(*a)?, debug_data_id)?;
                    self.append_dup(self.stack_height, debug_data_id)?;
                } else if self.can_be_removed(*a, Some(id), None)? {
                    self.append_or_remove_swap(self.class_element_position(*a)?, debug_data_id)?;
                    self.append_dup(self.class_element_position(*b)?, debug_data_id)?;
                    self.append_or_remove_swap(self.stack_height - 1, debug_data_id)?;
                } else {
                    self.append_dup(self.class_element_position(*b)?, debug_data_id)?;
                    self.append_dup(self.class_element_position(*a)?, debug_data_id)?;
                }
            }
            _ => return Err(CseError::InvalidState),
        }

        for (index, argument) in expression.arguments.iter().enumerate() {
            if self.stack.get(&(self.stack_height - index as i32)) != Some(argument) {
                return Err(CseError::InvalidState);
            }
        }

        while expression.item.kind == wire::KIND_OPERATION
            && opcode::is_commutative(expression.item.opcode)
            && self
                .generated_items
                .last()
                .is_some_and(|last| item::is_operation(last, op::SWAP1))
        {
            self.append_or_remove_swap(self.stack_height - 1, debug_data_id)?;
        }

        for index in 0..expression.arguments.len() {
            let position = self.stack_height - index as i32;
            if let Some(class_id) = self.stack.remove(&position) {
                if let Some(positions) = self.class_positions.get_mut(&class_id) {
                    positions.remove(&position);
                }
            }
        }

        self.append_item(expression.item.clone());
        if expression.item.kind != wire::KIND_OPERATION
            || opcode::instruction_info(expression.item.opcode).ret == 1
        {
            self.stack.insert(self.stack_height, id);
            self.class_positions
                .entry(id)
                .or_default()
                .insert(self.stack_height);
        } else if opcode::instruction_info(expression.item.opcode).ret == 0 {
            self.class_positions.entry(id).or_default();
        } else {
            return Err(CseError::InvalidState);
        }
        Ok(())
    }

    fn class_element_position(&self, id: Id) -> Result<i32, CseError> {
        self.class_positions
            .get(&id)
            .and_then(|positions| positions.iter().next_back().copied())
            .ok_or(CseError::InvalidState)
    }

    fn can_be_removed(
        &self,
        element: Id,
        result: Option<Id>,
        from_position: Option<i32>,
    ) -> Result<bool, CseError> {
        let from_position = if let Some(position) = from_position {
            position
        } else {
            self.class_element_position(element)?
        };
        let positions = self
            .class_positions
            .get(&element)
            .ok_or(CseError::InvalidState)?;
        let have_copy = positions.len() > 1;
        if self.final_classes.contains(&element) {
            Ok(have_copy && (self.target_stack.get(&from_position).copied() != Some(element)))
        } else if !have_copy {
            if let Some(needed_by) = self.needed_by.get(&element) {
                for needed_by in needed_by {
                    if Some(*needed_by) != result && !self.class_positions.contains_key(needed_by) {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        } else {
            Ok(true)
        }
    }

    fn remove_stack_top_if_possible(&mut self) -> Result<bool, CseError> {
        if self.stack.is_empty() {
            return Ok(false);
        };
        let Some(top) = self.stack.get(&self.stack_height).copied() else {
            return Err(CseError::InvalidState);
        };
        if !self.can_be_removed(top, None, Some(self.stack_height))? {
            return Ok(false);
        }
        if let Some(positions) = self.class_positions.get_mut(&top) {
            positions.remove(&self.stack_height);
        }
        self.stack.remove(&self.stack_height);
        self.append_item(item::operation(op::POP, 0));
        Ok(true)
    }

    fn append_dup(&mut self, from_position: i32, debug_data_id: u64) -> Result<(), CseError> {
        let instruction_num = 1 + self.stack_height - from_position;
        if instruction_num < 1
            || instruction_num as usize > self.evm_version.reachable_stack_depth()
        {
            return Err(CseError::StackTooDeep);
        }
        let id = self
            .stack
            .get(&from_position)
            .copied()
            .ok_or(CseError::InvalidState)?;
        self.append_item(item::operation(
            opcode::dup_instruction(instruction_num as usize),
            debug_data_id,
        ));
        self.stack.insert(self.stack_height, id);
        self.class_positions
            .entry(id)
            .or_default()
            .insert(self.stack_height);
        Ok(())
    }

    fn append_or_remove_swap(
        &mut self,
        from_position: i32,
        debug_data_id: u64,
    ) -> Result<(), CseError> {
        if from_position == self.stack_height {
            return Ok(());
        }
        let instruction_num = self.stack_height - from_position;
        if instruction_num < 1
            || instruction_num as usize > self.evm_version.reachable_stack_depth()
        {
            return Err(CseError::StackTooDeep);
        }
        let opcode = opcode::swap_instruction(instruction_num as usize);
        self.append_item(item::operation(opcode, debug_data_id));

        let top_id = self
            .stack
            .get(&self.stack_height)
            .copied()
            .ok_or(CseError::InvalidState)?;
        let from_id = self
            .stack
            .get(&from_position)
            .copied()
            .ok_or(CseError::InvalidState)?;
        if top_id != from_id {
            if let Some(positions) = self.class_positions.get_mut(&top_id) {
                positions.remove(&self.stack_height);
                positions.insert(from_position);
            }
            if let Some(positions) = self.class_positions.get_mut(&from_id) {
                positions.remove(&from_position);
                positions.insert(self.stack_height);
            }
            self.stack.insert(self.stack_height, from_id);
            self.stack.insert(from_position, top_id);
        }

        if self.generated_items.len() >= 2
            && item::is_swap(self.generated_items.last().expect("checked"))
            && item::item_eq(
                self.generated_items.last().expect("checked"),
                &self.generated_items[self.generated_items.len() - 2],
            )
        {
            self.generated_items.pop();
            self.generated_items.pop();
        }
        Ok(())
    }

    fn append_item(&mut self, item: ffi::WireAssemblyItem) {
        self.stack_height += item::deposit(&item);
        self.generated_items.push(item);
    }
}

pub fn optimise(
    items: &mut Vec<ffi::WireAssemblyItem>,
    evm_version: EvmVersion,
) -> Result<bool, wire::OptimizerError> {
    let uses_msize = items.iter().any(|item| {
        item::is_operation(item, op::MSIZE) || item.kind == wire::KIND_VERBATIM_BYTECODE
    });
    let mut optimised_items = Vec::with_capacity(items.len());
    let mut changed = false;
    let mut index = 0usize;

    while index < items.len() {
        let start = index;
        let mut eliminator =
            CommonSubexpressionEliminator::new(evm_version, (items.len() - index).min(2000));
        index = eliminator.feed_items(items, index, uses_msize);
        let original = &items[start..index];
        match eliminator.optimized_items() {
            Ok(optimised_chunk) if optimised_chunk.len() < original.len() => {
                optimised_items.extend(optimised_chunk);
                changed = true;
            }
            Err(CseError::InvalidState) => {
                return Err(wire::OptimizerError::InvalidState(
                    "CSE optimizer reached an invalid state.".to_string(),
                ));
            }
            _ => optimised_items.extend_from_slice(original),
        }
    }

    if changed {
        *items = optimised_items;
    }
    Ok(changed)
}

fn bool_word(value: bool) -> U256 {
    if value {
        U256::one()
    } else {
        U256::zero()
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
    U512::from_big_endian(&u256::to_be_bytes(value))
}

fn u512_to_u256(value: U512) -> U256 {
    let mut bytes = [0u8; 64];
    value.to_big_endian(&mut bytes);
    U256::from_big_endian(&bytes[32..])
}

fn low_bits_mask(bits: usize) -> U256 {
    (U256::one() << bits) - U256::one()
}

fn undefined_stack_item(stack_height: i32, debug_data_id: u64) -> ffi::WireAssemblyItem {
    let value = if stack_height >= 0 {
        U256::from(stack_height as u32)
    } else {
        u256::wrapping_neg(U256::from(-(stack_height as i64) as u64))
    };
    ffi::WireAssemblyItem {
        kind: wire::KIND_UNDEFINED,
        opcode: 0,
        data: u256::to_be_bytes(value),
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

fn is_commutative_item(item: &ffi::WireAssemblyItem) -> bool {
    item.kind == wire::KIND_OPERATION && opcode::is_commutative(item.opcode)
}

fn is_deterministic_item(item: &ffi::WireAssemblyItem) -> bool {
    item.kind != wire::KIND_OPERATION || opcode::is_deterministic(item.opcode)
}

fn expression_capacity(item_capacity_hint: usize) -> usize {
    item_capacity_hint.saturating_mul(4).max(64)
}

fn expression_key(
    item: &ffi::WireAssemblyItem,
    arguments: &[Id],
    sequence_number: u32,
) -> ExpressionKey {
    ExpressionKey {
        item: expression_item_key(item),
        arguments: arguments.to_vec(),
        sequence_number,
    }
}

fn expression_item_key(item: &ffi::WireAssemblyItem) -> ExpressionItemKey {
    match item.kind {
        wire::KIND_OPERATION => ExpressionItemKey::Operation {
            opcode: item.opcode,
            verbatim_arguments: item.verbatim_arguments,
            verbatim_return_values: item.verbatim_return_values,
        },
        wire::KIND_VERBATIM_BYTECODE => ExpressionItemKey::Verbatim {
            data: item.data.clone(),
            verbatim_data: item.verbatim_data.clone(),
            arguments: item.verbatim_arguments,
            return_values: item.verbatim_return_values,
        },
        kind => match item.data.as_slice().try_into() {
            Ok(data) => ExpressionItemKey::Data {
                kind,
                data,
                verbatim_arguments: item.verbatim_arguments,
                verbatim_return_values: item.verbatim_return_values,
            },
            Err(_) => ExpressionItemKey::DataBytes {
                kind,
                data: item.data.clone(),
                verbatim_arguments: item.verbatim_arguments,
                verbatim_return_values: item.verbatim_return_values,
            },
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addmod_does_not_wrap_before_modulo() {
        assert_eq!(
            addmod(!U256::zero(), U256::one(), U256::from(7u8)),
            U256::from(2u8)
        );
    }

    #[test]
    fn mulmod_does_not_wrap_before_modulo() {
        assert_eq!(
            mulmod(U256::one() << 255, U256::from(2u8), U256::from(7u8)),
            U256::from(2u8)
        );
    }

    #[test]
    fn mul_minus_one_takes_precedence_over_shifted_one_rule() {
        let mut classes = ExpressionClasses::with_capacity(0);
        let shift = classes.new_class(0);
        let one = classes.find_item(item::push_value(U256::one(), 0));
        let shifted_one = classes.find(item::operation(op::SHL, 0), vec![shift, one], 0);
        let minus_one = classes.find_item(item::push_value(u256::max_value(), 0));

        let result = classes.find(item::operation(op::MUL, 0), vec![minus_one, shifted_one], 0);
        let expression = classes.representative(result);

        assert_eq!(expression.item.kind, wire::KIND_OPERATION);
        assert_eq!(expression.item.opcode, op::SUB);
        assert_eq!(expression.arguments[1], shifted_one);
    }

    #[test]
    fn mul_associative_constant_precedes_shifted_one_rule() {
        let mut classes = ExpressionClasses::with_capacity(0);
        let value = classes.new_class(0);
        let shift = classes.new_class(0);
        let one = classes.find_item(item::push_value(U256::one(), 0));
        let three = classes.find_item(item::push_value(U256::from(3u8), 0));
        let shifted_one = classes.find(item::operation(op::SHL, 0), vec![shift, one], 0);
        let scaled_value = classes.find(item::operation(op::MUL, 0), vec![value, three], 0);

        let result = classes.find(
            item::operation(op::MUL, 0),
            vec![scaled_value, shifted_one],
            0,
        );
        let expression = classes.representative(result);

        assert_eq!(expression.item.kind, wire::KIND_OPERATION);
        assert_eq!(expression.item.opcode, op::MUL);
        assert!(expression
            .arguments
            .iter()
            .any(|argument| classes.known_constant(*argument) == Some(U256::from(3u8))));

        let shifted_value = expression
            .arguments
            .iter()
            .copied()
            .find(|argument| classes.known_constant(*argument).is_none())
            .expect("MUL result should retain a non-constant argument");
        let shifted_value_expression = classes.representative(shifted_value);
        assert_eq!(shifted_value_expression.item.kind, wire::KIND_OPERATION);
        assert_eq!(shifted_value_expression.item.opcode, op::SHL);
        assert_eq!(shifted_value_expression.arguments, vec![shift, value]);
    }

    #[test]
    fn and_byte_mask_rule_requires_cpp_argument_order() {
        let mut classes = ExpressionClasses::with_capacity(0);
        let mask = classes.find_item(item::push_value(U256::from(0xffu16), 0));
        let index = classes.new_class(0);
        let value = classes.new_class(0);
        let byte = classes.find(item::operation(op::BYTE, 0), vec![index, value], 0);

        let result = classes.find(item::operation(op::AND, 0), vec![byte, mask], 0);
        let expression = classes.representative(result);

        assert_eq!(expression.item.kind, wire::KIND_OPERATION);
        assert_eq!(expression.item.opcode, op::AND);
        assert_eq!(expression.arguments, vec![mask, byte]);
    }

    #[test]
    fn nested_signextend_precedes_large_index_rule() {
        let mut classes = ExpressionClasses::with_capacity(0);
        let value = classes.new_class(0);
        let zero = classes.find_item(item::push_value(U256::zero(), 0));
        let thirty_one = classes.find_item(item::push_value(U256::from(31u8), 0));
        let inner = classes.find(item::operation(op::SIGNEXTEND, 0), vec![zero, value], 0);

        let result = classes.find(
            item::operation(op::SIGNEXTEND, 0),
            vec![thirty_one, inner],
            0,
        );

        assert_eq!(result, inner);
    }
}
