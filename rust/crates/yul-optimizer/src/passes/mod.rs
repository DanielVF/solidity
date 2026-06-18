use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    LITERAL_STRING, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK, STATEMENT_BREAK, STATEMENT_CONTINUE,
    STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP, STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF,
    STATEMENT_LEAVE, STATEMENT_SWITCH, STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::BTreeSet;

mod block_flattener;
mod circular_references_pruner;
mod common_subexpression_eliminator;
mod conditional_simplifier;
mod conditional_unsimplifier;
mod control_flow_simplifier;
mod dead_code_eliminator;
mod equal_store_eliminator;
mod equivalent_function_combiner;
mod expression_inliner;
mod expression_joiner;
mod expression_simplifier;
mod expression_simplifier_rules;
mod expression_splitter;
mod for_loop_condition_into_body;
mod for_loop_condition_out_of_body;
mod for_loop_init_rewriter;
mod full_inliner;
mod function_grouper;
mod function_hoister;
mod function_specializer;
mod literal_rematerialiser;
mod load_resolver;
mod loop_invariant_code_motion;
mod rematerialiser;
mod ssa_reverser;
mod ssa_transform;
mod structural_simplifier;
mod unused_assign_eliminator;
mod unused_function_parameter_pruner;
mod unused_pruner;
mod unused_store_eliminator;
mod var_decl_initializer;

const MAX_ROUNDS: usize = 12;

pub fn run_pipeline(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let optimization_sequence =
        sequence_string(request, request.settings.optimization_sequence_id)?;
    let cleanup_sequence = sequence_string(request, request.settings.cleanup_sequence_id)?;
    let mut name_dispenser = NameDispenser::new(request);

    run_sequence(request, &mut name_dispenser, b"hgfo", false)?;
    run_sequence(request, &mut name_dispenser, &optimization_sequence, false)?;
    run_sequence(request, &mut name_dispenser, b"g", false)?;
    run_sequence(request, &mut name_dispenser, &cleanup_sequence, false)?;
    run_sequence(request, &mut name_dispenser, b"g", false)
}

pub(super) struct NameDispenser {
    used_names: BTreeSet<Vec<u8>>,
    counter: usize,
}

impl NameDispenser {
    fn new(request: &ffi::WireYulOptimizerRequest) -> Self {
        let mut used_names = BTreeSet::new();

        for name in &request.names {
            if let Some(string) = request.strings.get(name.name_id as usize) {
                used_names.insert(string.bytes.clone());
            }
        }
        for statement in &request.statements {
            if statement.kind == STATEMENT_FUNCTION_DEFINITION {
                if let Some(string) = request.strings.get(statement.name_id as usize) {
                    used_names.insert(string.bytes.clone());
                }
            }
        }
        for name_id in &request.reserved_identifier_ids {
            if let Some(string) = request.strings.get(*name_id as usize) {
                used_names.insert(string.bytes.clone());
            }
        }
        for builtin in &request.builtins {
            if let Some(string) = request.strings.get(builtin.name_id as usize) {
                used_names.insert(string.bytes.clone());
            }
        }

        Self {
            used_names,
            counter: 0,
        }
    }

    pub(super) fn new_name(&mut self, request: &mut ffi::WireYulOptimizerRequest) -> u64 {
        self.new_name_with_hint(request, Vec::new())
    }

    pub(super) fn new_name_like(
        &mut self,
        request: &mut ffi::WireYulOptimizerRequest,
        name_id: u64,
    ) -> u64 {
        let hint = request
            .strings
            .get(name_id as usize)
            .map(|string| string.bytes.clone())
            .unwrap_or_else(|| b"_".to_vec());
        self.new_name_with_hint(request, hint)
    }

    fn new_name_with_hint(
        &mut self,
        request: &mut ffi::WireYulOptimizerRequest,
        hint: Vec<u8>,
    ) -> u64 {
        if !self.illegal_name(&hint) {
            return self.push_used_name(request, hint);
        }

        loop {
            self.counter += 1;
            let mut candidate = hint.clone();
            candidate.push(b'_');
            candidate.extend(self.counter.to_string().bytes());
            if !self.illegal_name(&candidate) {
                return self.push_used_name(request, candidate);
            }
        }
    }

    fn illegal_name(&self, name: &[u8]) -> bool {
        name.is_empty() || self.used_names.contains(name)
    }

    fn push_used_name(&mut self, request: &mut ffi::WireYulOptimizerRequest, name: Vec<u8>) -> u64 {
        self.used_names.insert(name.clone());
        let string_id = request.strings.len() as u64;
        request.strings.push(ffi::WireString { bytes: name });
        string_id
    }
}

fn sequence_string(
    request: &ffi::WireYulOptimizerRequest,
    string_id: u64,
) -> Result<Vec<u8>, OptimizerError> {
    Ok(request
        .strings
        .get(string_id as usize)
        .ok_or_else(|| {
            OptimizerError::InvalidWire(format!("invalid optimizer sequence string: {string_id}"))
        })?
        .bytes
        .clone())
}

fn run_sequence(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut NameDispenser,
    sequence: &[u8],
    repeat_until_stable: bool,
) -> Result<(), OptimizerError> {
    validate_sequence(sequence)?;
    let subsequences = split_sequence(sequence)?;
    let mut code_size = if repeat_until_stable {
        code_size_including_functions(request)?
    } else {
        0
    };

    for _ in 0..MAX_ROUNDS {
        for subsequence in &subsequences {
            if subsequence.repeat {
                run_sequence(request, name_dispenser, subsequence.bytes, true)?;
            } else {
                for abbreviation in subsequence.bytes {
                    if is_ignored(*abbreviation) {
                        continue;
                    }
                    run_step(request, name_dispenser, *abbreviation)?;
                }
            }
        }

        if !repeat_until_stable {
            break;
        }

        let new_size = code_size_including_functions(request)?;
        if new_size == code_size {
            break;
        }
        code_size = new_size;
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct Subsequence<'a> {
    bytes: &'a [u8],
    repeat: bool,
}

fn split_sequence(sequence: &[u8]) -> Result<Vec<Subsequence<'_>>, OptimizerError> {
    let mut subsequences = Vec::new();
    let mut tail = sequence;

    while !tail.is_empty() {
        let prefix_end = tail
            .iter()
            .position(|byte| *byte == b'[')
            .unwrap_or(tail.len());
        if prefix_end > 0 {
            subsequences.push(Subsequence {
                bytes: &tail[..prefix_end],
                repeat: false,
            });
        }
        tail = &tail[prefix_end..];

        if tail.is_empty() {
            break;
        }

        let content_end = matching_bracket(tail)?;
        if content_end > 1 {
            subsequences.push(Subsequence {
                bytes: &tail[1..content_end],
                repeat: true,
            });
        }
        tail = &tail[content_end + 1..];
    }

    Ok(subsequences)
}

fn matching_bracket(sequence: &[u8]) -> Result<usize, OptimizerError> {
    if sequence.first() != Some(&b'[') {
        return Err(OptimizerError::InvalidWire(
            "internal sequence parser error".to_string(),
        ));
    }

    let mut nesting_level = 1usize;
    for (index, byte) in sequence.iter().enumerate().skip(1) {
        match *byte {
            b'[' => nesting_level += 1,
            b']' => {
                nesting_level -= 1;
                if nesting_level == 0 {
                    return Ok(index);
                }
            }
            _ => {}
        }
    }

    Err(OptimizerError::InvalidWire(
        "unbalanced optimizer sequence brackets".to_string(),
    ))
}

fn validate_sequence(sequence: &[u8]) -> Result<(), OptimizerError> {
    let mut nesting_level = 0isize;
    let mut colon_delimiters = 0usize;

    for abbreviation in sequence {
        match *abbreviation {
            b' ' | b'\n' => {}
            b'[' => nesting_level += 1,
            b']' => {
                nesting_level -= 1;
                if nesting_level < 0 {
                    return Err(OptimizerError::InvalidWire(
                        "unbalanced optimizer sequence brackets".to_string(),
                    ));
                }
            }
            b':' => {
                colon_delimiters += 1;
                if nesting_level != 0 {
                    return Err(OptimizerError::InvalidWire(
                        "cleanup sequence delimiter cannot be placed inside brackets".to_string(),
                    ));
                }
                if colon_delimiters > 1 {
                    return Err(OptimizerError::InvalidWire(
                        "too many cleanup sequence delimiters".to_string(),
                    ));
                }
            }
            byte if pass_name(byte).is_some() => {}
            byte => {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid optimizer step abbreviation: {}",
                    byte as char
                )));
            }
        }
    }

    if nesting_level != 0 {
        return Err(OptimizerError::InvalidWire(
            "unbalanced optimizer sequence brackets".to_string(),
        ));
    }

    Ok(())
}

fn is_ignored(abbreviation: u8) -> bool {
    matches!(abbreviation, b' ' | b'\n')
}

fn pass_name(abbreviation: u8) -> Option<&'static str> {
    Some(match abbreviation {
        b'f' => "BlockFlattener",
        b'l' => "CircularReferencesPruner",
        b'c' => "CommonSubexpressionEliminator",
        b'C' => "ConditionalSimplifier",
        b'U' => "ConditionalUnsimplifier",
        b'n' => "ControlFlowSimplifier",
        b'D' => "DeadCodeEliminator",
        b'E' => "EqualStoreEliminator",
        b'v' => "EquivalentFunctionCombiner",
        b'e' => "ExpressionInliner",
        b'j' => "ExpressionJoiner",
        b's' => "ExpressionSimplifier",
        b'x' => "ExpressionSplitter",
        b'I' => "ForLoopConditionIntoBody",
        b'O' => "ForLoopConditionOutOfBody",
        b'o' => "ForLoopInitRewriter",
        b'i' => "FullInliner",
        b'g' => "FunctionGrouper",
        b'h' => "FunctionHoister",
        b'F' => "FunctionSpecializer",
        b'T' => "LiteralRematerialiser",
        b'L' => "LoadResolver",
        b'M' => "LoopInvariantCodeMotion",
        b'r' => "UnusedAssignEliminator",
        b'S' => "UnusedStoreEliminator",
        b'm' => "Rematerialiser",
        b'V' => "SSAReverser",
        b'a' => "SSATransform",
        b't' => "StructuralSimplifier",
        b'p' => "UnusedFunctionParameterPruner",
        b'u' => "UnusedPruner",
        b'd' => "VarDeclInitializer",
        _ => return None,
    })
}

fn run_step(
    request: &mut ffi::WireYulOptimizerRequest,
    name_dispenser: &mut NameDispenser,
    abbreviation: u8,
) -> Result<(), OptimizerError> {
    match abbreviation {
        b'f' => block_flattener::run(request),
        b'l' => circular_references_pruner::run(request),
        b'c' => common_subexpression_eliminator::run(request),
        b'C' => conditional_simplifier::run(request),
        b'U' => conditional_unsimplifier::run(request),
        b'n' => control_flow_simplifier::run(request),
        b'D' => dead_code_eliminator::run(request),
        b'E' => equal_store_eliminator::run(request),
        b'v' => equivalent_function_combiner::run(request),
        b'e' => expression_inliner::run(request),
        b'j' => expression_joiner::run(request),
        b's' => expression_simplifier::run(request),
        b'x' => expression_splitter::run(request, name_dispenser),
        b'I' => for_loop_condition_into_body::run(request),
        b'O' => for_loop_condition_out_of_body::run(request),
        b'o' => for_loop_init_rewriter::run(request),
        b'i' => full_inliner::run(request, name_dispenser),
        b'g' => function_grouper::run(request),
        b'h' => function_hoister::run(request),
        b'F' => function_specializer::run(request, name_dispenser),
        b'T' => literal_rematerialiser::run(request),
        b'L' => load_resolver::run(request),
        b'M' => loop_invariant_code_motion::run(request),
        b'r' => unused_assign_eliminator::run(request),
        b'S' => unused_store_eliminator::run(request),
        b'm' => rematerialiser::run(request),
        b'V' => ssa_reverser::run(request),
        b'a' => ssa_transform::run(request, name_dispenser),
        b't' => structural_simplifier::run(request),
        b'p' => unused_function_parameter_pruner::run(request, name_dispenser),
        b'u' => unused_pruner::run(request),
        b'd' => var_decl_initializer::run(request),
        _ => Err(OptimizerError::InvalidWire(format!(
            "invalid optimizer step abbreviation: {}",
            abbreviation as char
        ))),
    }
}

fn code_size_including_functions(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<usize, OptimizerError> {
    code_size_block(request, request.root_block_id)
}

fn code_size_block(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<usize, OptimizerError> {
    let mut size = 0;
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        size += code_size_statement(request, *statement_id)?;
    }
    Ok(size)
}

fn code_size_statement(
    request: &ffi::WireYulOptimizerRequest,
    statement_id: u64,
) -> Result<usize, OptimizerError> {
    let statement = &request.statements[statement_id as usize];
    let size = match statement.kind {
        STATEMENT_EXPRESSION => code_size_expression(request, statement.expression_id)?,
        STATEMENT_ASSIGNMENT => code_size_expression(request, statement.value_expression_id)?,
        STATEMENT_VARIABLE_DECLARATION => {
            if statement.has_value {
                code_size_expression(request, statement.value_expression_id)?
            } else {
                0
            }
        }
        STATEMENT_FUNCTION_DEFINITION => 1 + code_size_block(request, statement.body_block_id)?,
        STATEMENT_IF => {
            2 + code_size_expression(request, statement.condition_expression_id)?
                + code_size_block(request, statement.body_block_id)?
        }
        STATEMENT_SWITCH => {
            let mut size = 1 + statement.case_ids.len() * 2;
            size += code_size_expression(request, statement.switch_expression_id)?;
            for case_id in &statement.case_ids {
                let switch_case = &request.cases[*case_id as usize];
                if switch_case.has_value {
                    size += code_size_expression(request, switch_case.value_expression_id)?;
                }
                size += code_size_block(request, switch_case.body_block_id)?;
            }
            size
        }
        STATEMENT_FOR_LOOP => {
            3 + code_size_block(request, statement.pre_block_id)?
                + code_size_expression(request, statement.condition_expression_id)?
                + code_size_block(request, statement.body_block_id)?
                + code_size_block(request, statement.post_block_id)?
        }
        STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => 2,
        STATEMENT_BLOCK => code_size_block(request, statement.block_id)?,
        _ => {
            return Err(OptimizerError::InvalidWire(format!(
                "invalid statement kind: {}",
                statement.kind
            )));
        }
    };
    Ok(size)
}

fn code_size_expression(
    request: &ffi::WireYulOptimizerRequest,
    expression_id: u64,
) -> Result<usize, OptimizerError> {
    let expression = &request.expressions[expression_id as usize];
    let size = match expression.kind {
        EXPRESSION_FUNCTION_CALL => {
            let mut size = 1;
            for argument_id in &expression.argument_expression_ids {
                size += code_size_expression(request, *argument_id)?;
            }
            size
        }
        EXPRESSION_IDENTIFIER => 0,
        EXPRESSION_LITERAL => {
            if expression.literal_kind != LITERAL_STRING
                && !expression.literal_unlimited
                && expression.literal_value.iter().all(|byte| *byte == 0)
            {
                0
            } else {
                1
            }
        }
        _ => {
            return Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            )));
        }
    };
    Ok(size)
}
