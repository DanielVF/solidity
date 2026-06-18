use crate::bridge::ffi;
use std::fmt;

pub const OPTIMIZER_ERROR_PANIC: u8 = 255;

pub const STATEMENT_EXPRESSION: u8 = 0;
pub const STATEMENT_ASSIGNMENT: u8 = 1;
pub const STATEMENT_VARIABLE_DECLARATION: u8 = 2;
pub const STATEMENT_FUNCTION_DEFINITION: u8 = 3;
pub const STATEMENT_IF: u8 = 4;
pub const STATEMENT_SWITCH: u8 = 5;
pub const STATEMENT_FOR_LOOP: u8 = 6;
pub const STATEMENT_BREAK: u8 = 7;
pub const STATEMENT_CONTINUE: u8 = 8;
pub const STATEMENT_LEAVE: u8 = 9;
pub const STATEMENT_BLOCK: u8 = 10;

pub const EXPRESSION_FUNCTION_CALL: u8 = 0;
pub const EXPRESSION_IDENTIFIER: u8 = 1;
pub const EXPRESSION_LITERAL: u8 = 2;

pub const FUNCTION_NAME_IDENTIFIER: u8 = 0;
pub const FUNCTION_NAME_BUILTIN: u8 = 1;

pub const LITERAL_NUMBER: u8 = 0;
pub const LITERAL_BOOLEAN: u8 = 1;
pub const LITERAL_STRING: u8 = 2;

pub const EFFECT_NONE: u8 = 0;
pub const EFFECT_READ: u8 = 1;
pub const EFFECT_WRITE: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizerError {
    InvalidWire(String),
}

impl OptimizerError {
    pub fn code(&self) -> u8 {
        match self {
            Self::InvalidWire(_) => 1,
        }
    }
}

impl fmt::Display for OptimizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWire(message) => write!(f, "{message}"),
        }
    }
}

pub fn validate_request(request: &ffi::WireYulOptimizerRequest) -> Result<(), OptimizerError> {
    validate_index(request.root_block_id, request.blocks.len(), "root block")?;

    validate_index(
        request.settings.optimization_sequence_id,
        request.strings.len(),
        "optimization sequence string",
    )?;
    validate_index(
        request.settings.cleanup_sequence_id,
        request.strings.len(),
        "cleanup sequence string",
    )?;
    validate_index(
        request.object_context.object_name_id,
        request.strings.len(),
        "object name string",
    )?;

    for id in &request.reserved_identifier_ids {
        validate_index(*id, request.strings.len(), "reserved identifier string")?;
    }
    for id in &request.object_context.object_paths {
        validate_index(*id, request.strings.len(), "object path string")?;
    }
    for id in &request.object_context.data_paths {
        validate_index(*id, request.strings.len(), "data path string")?;
    }
    for builtin in &request.builtins {
        validate_index(
            builtin.name_id,
            request.strings.len(),
            "builtin name string",
        )?;
        validate_effect(builtin.other_state, "builtin other_state")?;
        validate_effect(builtin.storage, "builtin storage")?;
        validate_effect(builtin.memory, "builtin memory")?;
        validate_effect(builtin.transient_storage, "builtin transient_storage")?;
        for kind in &builtin.literal_argument_kinds {
            if !matches!(
                *kind,
                0xff | LITERAL_NUMBER | LITERAL_BOOLEAN | LITERAL_STRING
            ) {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid builtin literal argument kind: {kind}"
                )));
            }
        }
    }

    for name in &request.names {
        validate_index(name.name_id, request.strings.len(), "name string")?;
    }
    for identifier in &request.identifiers {
        validate_index(
            identifier.name_id,
            request.strings.len(),
            "identifier string",
        )?;
    }

    for block in &request.blocks {
        for statement_id in &block.statement_ids {
            validate_index(*statement_id, request.statements.len(), "block statement")?;
        }
    }

    for statement in &request.statements {
        validate_statement(statement, request)?;
    }

    for expression in &request.expressions {
        validate_expression(expression, request)?;
    }

    for case in &request.cases {
        validate_index(case.body_block_id, request.blocks.len(), "case body block")?;
        if case.has_value {
            validate_index(
                case.value_expression_id,
                request.expressions.len(),
                "case value expression",
            )?;
        }
    }

    Ok(())
}

fn validate_statement(
    statement: &ffi::WireStatement,
    request: &ffi::WireYulOptimizerRequest,
) -> Result<(), OptimizerError> {
    match statement.kind {
        STATEMENT_EXPRESSION => {
            validate_index(
                statement.expression_id,
                request.expressions.len(),
                "expression statement expression",
            )?;
        }
        STATEMENT_ASSIGNMENT => {
            validate_ids(
                &statement.variable_ids,
                request.identifiers.len(),
                "assignment variable",
            )?;
            validate_index(
                statement.value_expression_id,
                request.expressions.len(),
                "assignment value expression",
            )?;
        }
        STATEMENT_VARIABLE_DECLARATION => {
            validate_ids(
                &statement.variable_ids,
                request.names.len(),
                "variable declaration name",
            )?;
            if statement.has_value {
                validate_index(
                    statement.value_expression_id,
                    request.expressions.len(),
                    "variable declaration value expression",
                )?;
            }
        }
        STATEMENT_FUNCTION_DEFINITION => {
            validate_index(statement.name_id, request.strings.len(), "function name")?;
            validate_ids(
                &statement.parameter_ids,
                request.names.len(),
                "function parameter",
            )?;
            validate_ids(
                &statement.return_variable_ids,
                request.names.len(),
                "function return variable",
            )?;
            validate_index(
                statement.body_block_id,
                request.blocks.len(),
                "function body block",
            )?;
        }
        STATEMENT_IF => {
            validate_index(
                statement.condition_expression_id,
                request.expressions.len(),
                "if condition expression",
            )?;
            validate_index(
                statement.body_block_id,
                request.blocks.len(),
                "if body block",
            )?;
        }
        STATEMENT_SWITCH => {
            validate_index(
                statement.switch_expression_id,
                request.expressions.len(),
                "switch expression",
            )?;
            validate_ids(&statement.case_ids, request.cases.len(), "switch case")?;
        }
        STATEMENT_FOR_LOOP => {
            validate_index(
                statement.pre_block_id,
                request.blocks.len(),
                "for pre block",
            )?;
            validate_index(
                statement.condition_expression_id,
                request.expressions.len(),
                "for condition expression",
            )?;
            validate_index(
                statement.post_block_id,
                request.blocks.len(),
                "for post block",
            )?;
            validate_index(
                statement.body_block_id,
                request.blocks.len(),
                "for body block",
            )?;
        }
        STATEMENT_BREAK | STATEMENT_CONTINUE | STATEMENT_LEAVE => {}
        STATEMENT_BLOCK => {
            validate_index(statement.block_id, request.blocks.len(), "nested block")?;
        }
        _ => {
            return Err(OptimizerError::InvalidWire(format!(
                "invalid statement kind: {}",
                statement.kind
            )));
        }
    }
    Ok(())
}

fn validate_expression(
    expression: &ffi::WireExpression,
    request: &ffi::WireYulOptimizerRequest,
) -> Result<(), OptimizerError> {
    match expression.kind {
        EXPRESSION_FUNCTION_CALL => {
            match expression.function_name_kind {
                FUNCTION_NAME_IDENTIFIER => validate_index(
                    expression.function_name_name_id,
                    request.strings.len(),
                    "function call name",
                )?,
                FUNCTION_NAME_BUILTIN => {}
                _ => {
                    return Err(OptimizerError::InvalidWire(format!(
                        "invalid function name kind: {}",
                        expression.function_name_kind
                    )));
                }
            }
            validate_ids(
                &expression.argument_expression_ids,
                request.expressions.len(),
                "function call argument",
            )?;
        }
        EXPRESSION_IDENTIFIER => {
            validate_index(
                expression.name_id,
                request.strings.len(),
                "identifier expression name",
            )?;
        }
        EXPRESSION_LITERAL => {
            if !matches!(
                expression.literal_kind,
                LITERAL_NUMBER | LITERAL_BOOLEAN | LITERAL_STRING
            ) {
                return Err(OptimizerError::InvalidWire(format!(
                    "invalid literal kind: {}",
                    expression.literal_kind
                )));
            }
            if expression.literal_unlimited {
                validate_index(
                    expression.literal_string_id,
                    request.strings.len(),
                    "unlimited literal string",
                )?;
            } else if expression.literal_value.len() != 32 {
                return Err(OptimizerError::InvalidWire(format!(
                    "literal numeric value must be 32 bytes, got {}",
                    expression.literal_value.len()
                )));
            }
            if expression.has_literal_hint {
                validate_index(
                    expression.literal_hint_id,
                    request.strings.len(),
                    "literal hint string",
                )?;
            }
        }
        _ => {
            return Err(OptimizerError::InvalidWire(format!(
                "invalid expression kind: {}",
                expression.kind
            )));
        }
    }
    Ok(())
}

fn validate_effect(value: u8, field: &str) -> Result<(), OptimizerError> {
    if matches!(value, EFFECT_NONE | EFFECT_READ | EFFECT_WRITE) {
        Ok(())
    } else {
        Err(OptimizerError::InvalidWire(format!(
            "invalid {field} side-effect value: {value}"
        )))
    }
}

fn validate_ids(ids: &[u64], len: usize, description: &str) -> Result<(), OptimizerError> {
    for id in ids {
        validate_index(*id, len, description)?;
    }
    Ok(())
}

fn validate_index(id: u64, len: usize, description: &str) -> Result<(), OptimizerError> {
    if id as usize >= len {
        Err(OptimizerError::InvalidWire(format!(
            "{description} index out of range: {id} >= {len}"
        )))
    } else {
        Ok(())
    }
}
