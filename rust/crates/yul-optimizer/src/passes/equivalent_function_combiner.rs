use crate::bridge::ffi;
use crate::wire::{
    OptimizerError, EXPRESSION_FUNCTION_CALL, EXPRESSION_IDENTIFIER, EXPRESSION_LITERAL,
    FUNCTION_NAME_BUILTIN, FUNCTION_NAME_IDENTIFIER, STATEMENT_ASSIGNMENT, STATEMENT_BLOCK,
    STATEMENT_BREAK, STATEMENT_CONTINUE, STATEMENT_EXPRESSION, STATEMENT_FOR_LOOP,
    STATEMENT_FUNCTION_DEFINITION, STATEMENT_IF, STATEMENT_LEAVE, STATEMENT_SWITCH,
    STATEMENT_VARIABLE_DECLARATION,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LiteralValueKey {
    Default,
    Numeric(Vec<u8>),
    Unlimited(Vec<u8>),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LiteralSortKey {
    Default,
    Literal { kind: u8, value: LiteralValueKey },
}

pub fn run(request: &mut ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    let duplicates = detect_duplicates(request)?;
    EquivalentFunctionCombiner {
        request,
        duplicates,
    }
    .visit_block_root()
}

struct EquivalentFunctionCombiner<'a> {
    request: &'a mut ffi::WireYulOptimizerRequest,
    duplicates: BTreeMap<u64, u64>,
}

impl EquivalentFunctionCombiner<'_> {
    fn visit_block_root(&mut self) -> Result<(), OptimizerError> {
        self.visit_block(self.request.root_block_id)
    }

    fn visit_block(&mut self, block_id: u64) -> Result<(), OptimizerError> {
        let statement_ids = self.request.blocks[block_id as usize].statement_ids.clone();
        for statement_id in statement_ids {
            self.visit_statement(statement_id)?;
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement_id: u64) -> Result<(), OptimizerError> {
        let statement = self.request.statements[statement_id as usize].clone();
        match statement.kind {
            STATEMENT_EXPRESSION => self.visit_expression(statement.expression_id),
            STATEMENT_ASSIGNMENT => self.visit_expression(statement.value_expression_id),
            STATEMENT_VARIABLE_DECLARATION => {
                if statement.has_value {
                    self.visit_expression(statement.value_expression_id)?;
                }
                Ok(())
            }
            STATEMENT_FUNCTION_DEFINITION => self.visit_block(statement.body_block_id),
            STATEMENT_IF => {
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_SWITCH => {
                self.visit_expression(statement.switch_expression_id)?;
                for case_id in statement.case_ids {
                    let switch_case = self.request.cases[case_id as usize].clone();
                    if switch_case.has_value {
                        self.visit_expression(switch_case.value_expression_id)?;
                    }
                    self.visit_block(switch_case.body_block_id)?;
                }
                Ok(())
            }
            STATEMENT_FOR_LOOP => {
                self.visit_block(statement.pre_block_id)?;
                self.visit_expression(statement.condition_expression_id)?;
                self.visit_block(statement.post_block_id)?;
                self.visit_block(statement.body_block_id)
            }
            STATEMENT_BLOCK => self.visit_block(statement.block_id),
            _ => Ok(()),
        }
    }

    fn visit_expression(&mut self, expression_id: u64) -> Result<(), OptimizerError> {
        let expression = self.request.expressions[expression_id as usize].clone();
        if expression.kind == EXPRESSION_FUNCTION_CALL {
            if expression.function_name_kind == FUNCTION_NAME_IDENTIFIER {
                if let Some(canonical_name_id) = self
                    .duplicates
                    .get(&expression.function_name_name_id)
                    .copied()
                {
                    self.request.expressions[expression_id as usize].function_name_name_id =
                        canonical_name_id;
                }
            }

            for argument_id in expression.argument_expression_ids.iter().rev() {
                self.visit_expression(*argument_id)?;
            }
        }
        Ok(())
    }
}

fn detect_duplicates(
    request: &ffi::WireYulOptimizerRequest,
) -> Result<BTreeMap<u64, u64>, OptimizerError> {
    let function_ids = function_definitions_for_detector(request, request.root_block_id)?;
    let mut unique_functions = Vec::new();
    let mut duplicates = BTreeMap::new();

    for function_id in function_ids {
        let function = &request.statements[function_id as usize];
        if let Some(canonical_function_id) = unique_functions
            .iter()
            .copied()
            .find(|candidate_id| functions_equal(request, function_id, *candidate_id))
        {
            duplicates.insert(
                function.name_id,
                request.statements[canonical_function_id as usize].name_id,
            );
        } else {
            unique_functions.push(function_id);
        }
    }

    Ok(duplicates)
}

fn function_definitions_for_detector(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
) -> Result<Vec<u64>, OptimizerError> {
    let mut output = Vec::new();
    collect_detector_function_definitions(request, block_id, &mut output)?;
    Ok(output)
}

fn collect_detector_function_definitions(
    request: &ffi::WireYulOptimizerRequest,
    block_id: u64,
    output: &mut Vec<u64>,
) -> Result<(), OptimizerError> {
    for statement_id in &request.blocks[block_id as usize].statement_ids {
        let statement = &request.statements[*statement_id as usize];
        match statement.kind {
            STATEMENT_FUNCTION_DEFINITION => output.push(*statement_id),
            STATEMENT_IF => {
                collect_detector_function_definitions(request, statement.body_block_id, output)?
            }
            STATEMENT_SWITCH => {
                for case_id in &statement.case_ids {
                    collect_detector_function_definitions(
                        request,
                        request.cases[*case_id as usize].body_block_id,
                        output,
                    )?;
                }
            }
            STATEMENT_FOR_LOOP => {
                collect_detector_function_definitions(request, statement.pre_block_id, output)?;
                collect_detector_function_definitions(request, statement.body_block_id, output)?;
                collect_detector_function_definitions(request, statement.post_block_id, output)?;
            }
            STATEMENT_BLOCK => {
                collect_detector_function_definitions(request, statement.block_id, output)?
            }
            _ => {}
        }
    }
    Ok(())
}

fn functions_equal(
    request: &ffi::WireYulOptimizerRequest,
    left_statement_id: u64,
    right_statement_id: u64,
) -> bool {
    let mut equality = SyntacticEquality::new(request);
    equality.function_definition_equal(left_statement_id, right_statement_id)
}

struct SyntacticEquality<'a> {
    request: &'a ffi::WireYulOptimizerRequest,
    ids_used: usize,
    identifiers_left: BTreeMap<u64, usize>,
    identifiers_right: BTreeMap<u64, usize>,
}

impl<'a> SyntacticEquality<'a> {
    fn new(request: &'a ffi::WireYulOptimizerRequest) -> Self {
        Self {
            request,
            ids_used: 0,
            identifiers_left: BTreeMap::new(),
            identifiers_right: BTreeMap::new(),
        }
    }

    fn function_definition_equal(&mut self, left_id: u64, right_id: u64) -> bool {
        let left = &self.request.statements[left_id as usize];
        let right = &self.request.statements[right_id as usize];
        if left.kind != STATEMENT_FUNCTION_DEFINITION || right.kind != STATEMENT_FUNCTION_DEFINITION
        {
            return false;
        }

        self.name_lists_equal(&left.parameter_ids, &right.parameter_ids)
            && self.name_lists_equal(&left.return_variable_ids, &right.return_variable_ids)
            && self.block_equal(left.body_block_id, right.body_block_id)
    }

    fn statement_equal(&mut self, left_id: u64, right_id: u64) -> bool {
        let left = &self.request.statements[left_id as usize];
        let right = &self.request.statements[right_id as usize];
        if left.kind != right.kind {
            return false;
        }

        match left.kind {
            STATEMENT_EXPRESSION => self.expression_equal(left.expression_id, right.expression_id),
            STATEMENT_ASSIGNMENT => {
                self.identifier_lists_equal(&left.variable_ids, &right.variable_ids)
                    && self.expression_equal(left.value_expression_id, right.value_expression_id)
            }
            STATEMENT_VARIABLE_DECLARATION => {
                left.has_value == right.has_value
                    && (!left.has_value
                        || self
                            .expression_equal(left.value_expression_id, right.value_expression_id))
                    && self.name_lists_equal(&left.variable_ids, &right.variable_ids)
            }
            STATEMENT_FUNCTION_DEFINITION => self.function_definition_equal(left_id, right_id),
            STATEMENT_IF => {
                self.expression_equal(left.condition_expression_id, right.condition_expression_id)
                    && self.block_equal(left.body_block_id, right.body_block_id)
            }
            STATEMENT_SWITCH => self.switch_equal(left, right),
            STATEMENT_FOR_LOOP => {
                self.block_equal(left.pre_block_id, right.pre_block_id)
                    && self.expression_equal(
                        left.condition_expression_id,
                        right.condition_expression_id,
                    )
                    && self.block_equal(left.body_block_id, right.body_block_id)
                    && self.block_equal(left.post_block_id, right.post_block_id)
            }
            STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => true,
            STATEMENT_BLOCK => self.block_equal(left.block_id, right.block_id),
            _ => false,
        }
    }

    fn switch_equal(&mut self, left: &ffi::WireStatement, right: &ffi::WireStatement) -> bool {
        if !self.expression_equal(left.switch_expression_id, right.switch_expression_id)
            || left.case_ids.len() != right.case_ids.len()
        {
            return false;
        }

        let mut left_cases = left.case_ids.clone();
        let mut right_cases = right.case_ids.clone();
        left_cases.sort_by_key(|case_id| self.case_literal_sort_key(*case_id));
        right_cases.sort_by_key(|case_id| self.case_literal_sort_key(*case_id));

        left_cases
            .into_iter()
            .zip(right_cases)
            .all(|(left_case, right_case)| self.case_equal(left_case, right_case))
    }

    fn case_equal(&mut self, left_id: u64, right_id: u64) -> bool {
        let left = &self.request.cases[left_id as usize];
        let right = &self.request.cases[right_id as usize];
        left.has_value == right.has_value
            && (!left.has_value
                || self
                    .literal_expression_equal(left.value_expression_id, right.value_expression_id))
            && self.block_equal(left.body_block_id, right.body_block_id)
    }

    fn block_equal(&mut self, left_id: u64, right_id: u64) -> bool {
        let left = &self.request.blocks[left_id as usize];
        let right = &self.request.blocks[right_id as usize];
        left.statement_ids.len() == right.statement_ids.len()
            && left.statement_ids.iter().zip(&right.statement_ids).all(
                |(left_statement, right_statement)| {
                    self.statement_equal(*left_statement, *right_statement)
                },
            )
    }

    fn expression_equal(&mut self, left_id: u64, right_id: u64) -> bool {
        let left = &self.request.expressions[left_id as usize];
        let right = &self.request.expressions[right_id as usize];
        if left.kind != right.kind {
            return false;
        }

        match left.kind {
            EXPRESSION_FUNCTION_CALL => {
                self.function_name_equal(left, right)
                    && left.argument_expression_ids.len() == right.argument_expression_ids.len()
                    && left
                        .argument_expression_ids
                        .iter()
                        .zip(&right.argument_expression_ids)
                        .all(|(left_argument, right_argument)| {
                            self.expression_equal(*left_argument, *right_argument)
                        })
            }
            EXPRESSION_IDENTIFIER => self.identifier_names_equal(left.name_id, right.name_id),
            EXPRESSION_LITERAL => self.literal_expression_equal(left_id, right_id),
            _ => false,
        }
    }

    fn function_name_equal(
        &mut self,
        left: &ffi::WireExpression,
        right: &ffi::WireExpression,
    ) -> bool {
        if left.function_name_kind != right.function_name_kind {
            return false;
        }

        match left.function_name_kind {
            FUNCTION_NAME_IDENTIFIER => {
                self.identifier_names_equal(left.function_name_name_id, right.function_name_name_id)
            }
            FUNCTION_NAME_BUILTIN => {
                left.function_name_builtin_handle == right.function_name_builtin_handle
            }
            _ => false,
        }
    }

    fn literal_expression_equal(&self, left_id: u64, right_id: u64) -> bool {
        self.literal_key(left_id) == self.literal_key(right_id)
    }

    fn identifier_lists_equal(&mut self, left: &[u64], right: &[u64]) -> bool {
        left.len() == right.len()
            && left.iter().zip(right).all(|(left_id, right_id)| {
                let left_name = self.request.identifiers[*left_id as usize].name_id;
                let right_name = self.request.identifiers[*right_id as usize].name_id;
                self.identifier_names_equal(left_name, right_name)
            })
    }

    fn name_lists_equal(&mut self, left: &[u64], right: &[u64]) -> bool {
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left_id, right_id)| self.visit_declaration(*left_id, *right_id))
    }

    fn visit_declaration(&mut self, left_name_id: u64, right_name_id: u64) -> bool {
        let id = self.ids_used;
        self.ids_used += 1;
        self.identifiers_left
            .insert(self.request.names[left_name_id as usize].name_id, id);
        self.identifiers_right
            .insert(self.request.names[right_name_id as usize].name_id, id);
        true
    }

    fn identifier_names_equal(&self, left_name: u64, right_name: u64) -> bool {
        let left_id = self.identifiers_left.get(&left_name);
        let right_id = self.identifiers_right.get(&right_name);
        match (left_id, right_id) {
            (None, None) => left_name == right_name,
            (Some(left_id), Some(right_id)) => left_id == right_id,
            _ => false,
        }
    }

    fn case_literal_sort_key(&self, case_id: u64) -> LiteralSortKey {
        let switch_case = &self.request.cases[case_id as usize];
        if switch_case.has_value {
            self.literal_sort_key(switch_case.value_expression_id)
        } else {
            LiteralSortKey::Default
        }
    }

    fn literal_sort_key(&self, expression_id: u64) -> LiteralSortKey {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_LITERAL {
            return LiteralSortKey::Default;
        }

        LiteralSortKey::Literal {
            kind: expression.literal_kind,
            value: self.literal_key(expression_id),
        }
    }

    fn literal_key(&self, expression_id: u64) -> LiteralValueKey {
        let expression = &self.request.expressions[expression_id as usize];
        if expression.kind != EXPRESSION_LITERAL {
            return LiteralValueKey::Default;
        }

        if expression.literal_unlimited {
            LiteralValueKey::Unlimited(
                self.request.strings[expression.literal_string_id as usize]
                    .bytes
                    .clone(),
            )
        } else {
            LiteralValueKey::Numeric(expression.literal_value.clone())
        }
    }
}
