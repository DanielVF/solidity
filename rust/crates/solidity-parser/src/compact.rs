use crate::bridge::ffi;
use crate::parser::{
    ParsedFunctionHeaderParserResult, ParsedModifierInvocationResult, ParsedTypeDefinitionResult,
    ParsedTypeNameResult, ParsedVariableDeclaration, RustExpressionData, RustExpressionResult,
    RustStatementData, RustStatementResult, RustTryCatchClauseResult,
};
use langutil::token;
use std::collections::HashMap;

pub type CompactNodeId = i64;

const WIRE_COMPACT_PAYLOAD_NONE: u8 = 0;
const WIRE_COMPACT_PAYLOAD_TOKEN: u8 = 1;
const WIRE_COMPACT_PAYLOAD_LIST: u8 = 2;
const WIRE_COMPACT_PAYLOAD_UNARY: u8 = 3;
const WIRE_COMPACT_PAYLOAD_BINARY: u8 = 4;
const WIRE_COMPACT_PAYLOAD_TERNARY: u8 = 5;
const WIRE_COMPACT_PAYLOAD_OPTIONAL_CHILD: u8 = 6;
const WIRE_COMPACT_PAYLOAD_CALL: u8 = 7;
const WIRE_COMPACT_PAYLOAD_DECLARATION: u8 = 8;
const INVALID_COMPACT_NODE_REF: u32 = u32::MAX;

pub struct CompactParserHandle {
    legacy_wire: Option<ffi::WireParserResult>,
    arena: ffi::WireCompactParseOutput,
}

pub fn parse_compact() -> Box<CompactParserHandle> {
    let arena = crate::parser::parse_compact_output().into_wire();
    Box::new(CompactParserHandle {
        legacy_wire: None,
        arena,
    })
}

pub fn parse_compact_with_legacy() -> Box<CompactParserHandle> {
    let mut legacy_wire = crate::parser::parse();
    let arena = CompactParseOutput::from_wire_parser_result(&legacy_wire).into_wire();
    clear_compact_owned_metadata_from_legacy_wire(&mut legacy_wire);
    Box::new(CompactParserHandle {
        legacy_wire: Some(legacy_wire),
        arena,
    })
}

pub fn compact_parser_legacy_wire_result(handle: &CompactParserHandle) -> &ffi::WireParserResult {
    handle
        .legacy_wire
        .as_ref()
        .expect("compact parser handle does not retain a legacy wire result")
}

pub fn compact_parser_arena(handle: &CompactParserHandle) -> &ffi::WireCompactParseOutput {
    &handle.arena
}

fn clear_compact_owned_metadata_from_legacy_wire(wire: &mut ffi::WireParserResult) {
    wire.error_message.clear();
    wire.source_unit.text.bytes.clear();
    wire.source_unit_nodes.clear();
    wire.source_unit_contracts.clear();
    wire.source_unit_pragmas.clear();
    wire.source_unit_imports.clear();
    wire.source_unit_enums.clear();
    wire.source_unit_structs.clear();
    wire.source_unit_events.clear();
    wire.source_unit_errors.clear();
    wire.source_unit_functions.clear();
    wire.source_unit_using_directives.clear();
    wire.source_unit_variable_declarations.clear();
    wire.source_unit_user_defined_value_types.clear();
    wire.errors.clear();
    wire.warnings.clear();
    wire.license.bytes.clear();
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CompactNodeRef(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactChildRange {
    pub start: u32,
    pub len: u32,
}

impl CompactChildRange {
    pub const fn empty() -> Self {
        Self { start: 0, len: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactTextRef {
    pub start: u32,
    pub len: u32,
}

impl CompactTextRef {
    pub const fn empty() -> Self {
        Self { start: 0, len: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactSpan {
    pub start: i32,
    pub end: i32,
    pub source_id: i32,
}

impl From<&ffi::WireSourceLocation> for CompactSpan {
    fn from(location: &ffi::WireSourceLocation) -> Self {
        Self {
            start: location.start as i32,
            end: location.end as i32,
            source_id: location.source_id as i32,
        }
    }
}

pub(crate) struct CompactVariableDeclarationParts<'a> {
    pub(crate) variable_declaration: &'a ffi::WireAstNode,
    pub(crate) type_name: &'a ffi::WireAstNode,
    pub(crate) type_expression: &'a ffi::WireAstNode,
    pub(crate) type_expression_detail: CompactExpressionDetailRef<'a>,
    pub(crate) documentation: &'a ffi::WireAstNode,
    pub(crate) overrides: &'a ffi::WireAstNode,
    pub(crate) override_paths: &'a [ffi::WireAstNode],
    pub(crate) override_path_details: &'a [ffi::WireIdentifierPathResult],
    pub(crate) value: &'a ffi::WireAstNode,
    pub(crate) value_detail: CompactExpressionDetailRef<'a>,
    pub(crate) type_name_elementary_token: u32,
    pub(crate) type_name_elementary_first_number: u32,
    pub(crate) type_name_elementary_second_number: u32,
    pub(crate) type_name_has_state_mutability: bool,
    pub(crate) type_name_state_mutability: u8,
    pub(crate) type_name_user_defined_path_node: &'a ffi::WireAstNode,
    pub(crate) type_name_user_defined_path: &'a [ffi::WireString],
    pub(crate) type_name_user_defined_path_locations: &'a [ffi::WireSourceLocation],
    pub(crate) type_name_array_base_types: &'a [ffi::WireAstNode],
    pub(crate) type_name_array_lengths: &'a [ffi::WireAstNode],
    pub(crate) type_name_array_length_details: CompactExpressionDetailsRef<'a>,
    pub(crate) type_name_function_parameters: &'a ffi::WireAstNode,
    pub(crate) type_name_function_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) type_name_function_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) type_name_function_return_parameters: &'a ffi::WireAstNode,
    pub(crate) type_name_function_return_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) type_name_function_return_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) type_name_function_visibility: u8,
    pub(crate) type_name_function_state_mutability: u8,
    pub(crate) type_name_mapping_key_type: &'a ffi::WireAstNode,
    pub(crate) type_name_mapping_key_elementary_token: u32,
    pub(crate) type_name_mapping_key_elementary_first_number: u32,
    pub(crate) type_name_mapping_key_elementary_second_number: u32,
    pub(crate) type_name_mapping_key_user_defined_path_node: &'a ffi::WireAstNode,
    pub(crate) type_name_mapping_key_user_defined_path: &'a [ffi::WireString],
    pub(crate) type_name_mapping_key_user_defined_path_locations: &'a [ffi::WireSourceLocation],
    pub(crate) type_name_mapping_key_name: &'a ffi::WireString,
    pub(crate) type_name_mapping_key_name_location: &'a ffi::WireSourceLocation,
    pub(crate) type_name_mapping_value_type: &'a ffi::WireAstNode,
    pub(crate) type_name_mapping_value_elementary_token: u32,
    pub(crate) type_name_mapping_value_elementary_first_number: u32,
    pub(crate) type_name_mapping_value_elementary_second_number: u32,
    pub(crate) type_name_mapping_value_has_state_mutability: bool,
    pub(crate) type_name_mapping_value_state_mutability: u8,
    pub(crate) type_name_mapping_value_user_defined_path_node: &'a ffi::WireAstNode,
    pub(crate) type_name_mapping_value_user_defined_path: &'a [ffi::WireString],
    pub(crate) type_name_mapping_value_user_defined_path_locations: &'a [ffi::WireSourceLocation],
    pub(crate) type_name_mapping_value_array_base_types: &'a [ffi::WireAstNode],
    pub(crate) type_name_mapping_value_array_lengths: &'a [ffi::WireAstNode],
    pub(crate) type_name_mapping_value_array_length_details: CompactExpressionDetailsRef<'a>,
    pub(crate) type_name_mapping_value_function_parameters: &'a ffi::WireAstNode,
    pub(crate) type_name_mapping_value_function_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) type_name_mapping_value_function_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) type_name_mapping_value_function_return_parameters: &'a ffi::WireAstNode,
    pub(crate) type_name_mapping_value_function_return_parameter_declarations:
        &'a [ffi::WireAstNode],
    pub(crate) type_name_mapping_value_function_return_parameter_details:
        CompactVariableDetailsRef<'a>,
    pub(crate) type_name_mapping_value_function_visibility: u8,
    pub(crate) type_name_mapping_value_function_state_mutability: u8,
    pub(crate) type_name_mapping_value_name: &'a ffi::WireString,
    pub(crate) type_name_mapping_value_name_location: &'a ffi::WireSourceLocation,
    pub(crate) type_name_mapping_details: &'a [ffi::WireMappingTypeName],
    pub(crate) name: &'a ffi::WireString,
    pub(crate) name_location: &'a ffi::WireSourceLocation,
    pub(crate) visibility: u8,
    pub(crate) mutability: u8,
    pub(crate) variable_location: u8,
    pub(crate) indexed: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum CompactExpressionDetailRef<'a> {
    Wire(&'a ffi::WireExpressionResult),
    Rust(&'a RustExpressionResult),
}

#[derive(Clone, Copy)]
pub(crate) enum CompactExpressionDetailsRef<'a> {
    Wire(&'a [ffi::WireExpressionResult]),
    Rust(&'a [RustExpressionResult]),
}

#[derive(Clone, Copy)]
pub(crate) enum CompactVariableDetailsRef<'a> {
    Wire(&'a [ffi::WireVariableDeclarationResult]),
    Parsed(&'a [ParsedVariableDeclaration]),
}

pub(crate) struct CompactTypeNameParts<'a> {
    pub(crate) type_name: &'a ffi::WireAstNode,
    pub(crate) array_base_type: &'a ffi::WireAstNode,
    pub(crate) array_length: &'a ffi::WireAstNode,
    pub(crate) array_base_types: &'a [ffi::WireAstNode],
    pub(crate) array_lengths: &'a [ffi::WireAstNode],
    pub(crate) array_length_details: CompactExpressionDetailsRef<'a>,
    pub(crate) elementary_type_token: u32,
    pub(crate) elementary_type_first_number: u32,
    pub(crate) elementary_type_second_number: u32,
    pub(crate) has_state_mutability: bool,
    pub(crate) state_mutability: u8,
    pub(crate) user_defined_path_node: &'a ffi::WireAstNode,
    pub(crate) user_defined_path: &'a [ffi::WireString],
    pub(crate) user_defined_path_locations: &'a [ffi::WireSourceLocation],
    pub(crate) function_parameters: &'a ffi::WireAstNode,
    pub(crate) function_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) function_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) function_return_parameters: &'a ffi::WireAstNode,
    pub(crate) function_return_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) function_return_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) function_visibility: u8,
    pub(crate) function_state_mutability: u8,
    pub(crate) mapping_key_type: &'a ffi::WireAstNode,
    pub(crate) mapping_key_elementary_token: u32,
    pub(crate) mapping_key_elementary_first_number: u32,
    pub(crate) mapping_key_elementary_second_number: u32,
    pub(crate) mapping_key_user_defined_path_node: &'a ffi::WireAstNode,
    pub(crate) mapping_key_user_defined_path: &'a [ffi::WireString],
    pub(crate) mapping_key_user_defined_path_locations: &'a [ffi::WireSourceLocation],
    pub(crate) mapping_key_name: &'a ffi::WireString,
    pub(crate) mapping_key_name_location: &'a ffi::WireSourceLocation,
    pub(crate) mapping_value_type: &'a ffi::WireAstNode,
    pub(crate) mapping_value_elementary_token: u32,
    pub(crate) mapping_value_elementary_first_number: u32,
    pub(crate) mapping_value_elementary_second_number: u32,
    pub(crate) mapping_value_has_state_mutability: bool,
    pub(crate) mapping_value_state_mutability: u8,
    pub(crate) mapping_value_user_defined_path_node: &'a ffi::WireAstNode,
    pub(crate) mapping_value_user_defined_path: &'a [ffi::WireString],
    pub(crate) mapping_value_user_defined_path_locations: &'a [ffi::WireSourceLocation],
    pub(crate) mapping_value_array_base_types: &'a [ffi::WireAstNode],
    pub(crate) mapping_value_array_lengths: &'a [ffi::WireAstNode],
    pub(crate) mapping_value_array_length_details: CompactExpressionDetailsRef<'a>,
    pub(crate) mapping_value_function_parameters: &'a ffi::WireAstNode,
    pub(crate) mapping_value_function_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) mapping_value_function_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) mapping_value_function_return_parameters: &'a ffi::WireAstNode,
    pub(crate) mapping_value_function_return_parameter_declarations: &'a [ffi::WireAstNode],
    pub(crate) mapping_value_function_return_parameter_details: CompactVariableDetailsRef<'a>,
    pub(crate) mapping_value_function_visibility: u8,
    pub(crate) mapping_value_function_state_mutability: u8,
    pub(crate) mapping_value_name: &'a ffi::WireString,
    pub(crate) mapping_value_name_location: &'a ffi::WireSourceLocation,
    pub(crate) mapping_details: &'a [ffi::WireMappingTypeName],
}

impl<'a> From<&'a ffi::WireTypeNameResult> for CompactTypeNameParts<'a> {
    fn from(type_name: &'a ffi::WireTypeNameResult) -> Self {
        Self {
            type_name: &type_name.type_name,
            array_base_type: &type_name.array_base_type,
            array_length: &type_name.array_length,
            array_base_types: &type_name.array_base_types,
            array_lengths: &type_name.array_lengths,
            array_length_details: CompactExpressionDetailsRef::Wire(
                &type_name.array_length_details,
            ),
            elementary_type_token: type_name.elementary_type_token,
            elementary_type_first_number: type_name.elementary_type_first_number,
            elementary_type_second_number: type_name.elementary_type_second_number,
            has_state_mutability: type_name.has_state_mutability,
            state_mutability: type_name.state_mutability,
            user_defined_path_node: &type_name.user_defined_path_node,
            user_defined_path: &type_name.user_defined_path,
            user_defined_path_locations: &type_name.user_defined_path_locations,
            function_parameters: &type_name.function_parameters,
            function_parameter_declarations: &type_name.function_parameter_declarations,
            function_parameter_details: CompactVariableDetailsRef::Wire(
                &type_name.function_parameter_details,
            ),
            function_return_parameters: &type_name.function_return_parameters,
            function_return_parameter_declarations: &type_name
                .function_return_parameter_declarations,
            function_return_parameter_details: CompactVariableDetailsRef::Wire(
                &type_name.function_return_parameter_details,
            ),
            function_visibility: type_name.function_visibility,
            function_state_mutability: type_name.function_state_mutability,
            mapping_key_type: &type_name.mapping_key_type,
            mapping_key_elementary_token: type_name.mapping_key_elementary_token,
            mapping_key_elementary_first_number: type_name.mapping_key_elementary_first_number,
            mapping_key_elementary_second_number: type_name.mapping_key_elementary_second_number,
            mapping_key_user_defined_path_node: &type_name.mapping_key_user_defined_path_node,
            mapping_key_user_defined_path: &type_name.mapping_key_user_defined_path,
            mapping_key_user_defined_path_locations: &type_name
                .mapping_key_user_defined_path_locations,
            mapping_key_name: &type_name.mapping_key_name,
            mapping_key_name_location: &type_name.mapping_key_name_location,
            mapping_value_type: &type_name.mapping_value_type,
            mapping_value_elementary_token: type_name.mapping_value_elementary_token,
            mapping_value_elementary_first_number: type_name.mapping_value_elementary_first_number,
            mapping_value_elementary_second_number: type_name
                .mapping_value_elementary_second_number,
            mapping_value_has_state_mutability: type_name.mapping_value_has_state_mutability,
            mapping_value_state_mutability: type_name.mapping_value_state_mutability,
            mapping_value_user_defined_path_node: &type_name.mapping_value_user_defined_path_node,
            mapping_value_user_defined_path: &type_name.mapping_value_user_defined_path,
            mapping_value_user_defined_path_locations: &type_name
                .mapping_value_user_defined_path_locations,
            mapping_value_array_base_types: &type_name.mapping_value_array_base_types,
            mapping_value_array_lengths: &type_name.mapping_value_array_lengths,
            mapping_value_array_length_details: CompactExpressionDetailsRef::Wire(
                &type_name.mapping_value_array_length_details,
            ),
            mapping_value_function_parameters: &type_name.mapping_value_function_parameters,
            mapping_value_function_parameter_declarations: &type_name
                .mapping_value_function_parameter_declarations,
            mapping_value_function_parameter_details: CompactVariableDetailsRef::Wire(
                &type_name.mapping_value_function_parameter_details,
            ),
            mapping_value_function_return_parameters: &type_name
                .mapping_value_function_return_parameters,
            mapping_value_function_return_parameter_declarations: &type_name
                .mapping_value_function_return_parameter_declarations,
            mapping_value_function_return_parameter_details: CompactVariableDetailsRef::Wire(
                &type_name.mapping_value_function_return_parameter_details,
            ),
            mapping_value_function_visibility: type_name.mapping_value_function_visibility,
            mapping_value_function_state_mutability: type_name
                .mapping_value_function_state_mutability,
            mapping_value_name: &type_name.mapping_value_name,
            mapping_value_name_location: &type_name.mapping_value_name_location,
            mapping_details: &type_name.mapping_details,
        }
    }
}

impl<'a> From<&'a ffi::WireVariableDeclarationResult> for CompactVariableDeclarationParts<'a> {
    fn from(variable: &'a ffi::WireVariableDeclarationResult) -> Self {
        Self {
            variable_declaration: &variable.variable_declaration,
            type_name: &variable.type_name,
            type_expression: &variable.type_expression,
            type_expression_detail: CompactExpressionDetailRef::Wire(
                &variable.type_expression_detail,
            ),
            documentation: &variable.documentation,
            overrides: &variable.overrides,
            override_paths: &variable.override_paths,
            override_path_details: &variable.override_path_details,
            value: &variable.value,
            value_detail: CompactExpressionDetailRef::Wire(&variable.value_detail),
            type_name_elementary_token: variable.type_name_elementary_token,
            type_name_elementary_first_number: variable.type_name_elementary_first_number,
            type_name_elementary_second_number: variable.type_name_elementary_second_number,
            type_name_has_state_mutability: variable.type_name_has_state_mutability,
            type_name_state_mutability: variable.type_name_state_mutability,
            type_name_user_defined_path_node: &variable.type_name_user_defined_path_node,
            type_name_user_defined_path: &variable.type_name_user_defined_path,
            type_name_user_defined_path_locations: &variable.type_name_user_defined_path_locations,
            type_name_array_base_types: &variable.type_name_array_base_types,
            type_name_array_lengths: &variable.type_name_array_lengths,
            type_name_array_length_details: CompactExpressionDetailsRef::Wire(
                &variable.type_name_array_length_details,
            ),
            type_name_function_parameters: &variable.type_name_function_parameters,
            type_name_function_parameter_declarations: &variable
                .type_name_function_parameter_declarations,
            type_name_function_parameter_details: CompactVariableDetailsRef::Wire(
                &variable.type_name_function_parameter_details,
            ),
            type_name_function_return_parameters: &variable.type_name_function_return_parameters,
            type_name_function_return_parameter_declarations: &variable
                .type_name_function_return_parameter_declarations,
            type_name_function_return_parameter_details: CompactVariableDetailsRef::Wire(
                &variable.type_name_function_return_parameter_details,
            ),
            type_name_function_visibility: variable.type_name_function_visibility,
            type_name_function_state_mutability: variable.type_name_function_state_mutability,
            type_name_mapping_key_type: &variable.type_name_mapping_key_type,
            type_name_mapping_key_elementary_token: variable.type_name_mapping_key_elementary_token,
            type_name_mapping_key_elementary_first_number: variable
                .type_name_mapping_key_elementary_first_number,
            type_name_mapping_key_elementary_second_number: variable
                .type_name_mapping_key_elementary_second_number,
            type_name_mapping_key_user_defined_path_node: &variable
                .type_name_mapping_key_user_defined_path_node,
            type_name_mapping_key_user_defined_path: &variable
                .type_name_mapping_key_user_defined_path,
            type_name_mapping_key_user_defined_path_locations: &variable
                .type_name_mapping_key_user_defined_path_locations,
            type_name_mapping_key_name: &variable.type_name_mapping_key_name,
            type_name_mapping_key_name_location: &variable.type_name_mapping_key_name_location,
            type_name_mapping_value_type: &variable.type_name_mapping_value_type,
            type_name_mapping_value_elementary_token: variable
                .type_name_mapping_value_elementary_token,
            type_name_mapping_value_elementary_first_number: variable
                .type_name_mapping_value_elementary_first_number,
            type_name_mapping_value_elementary_second_number: variable
                .type_name_mapping_value_elementary_second_number,
            type_name_mapping_value_has_state_mutability: variable
                .type_name_mapping_value_has_state_mutability,
            type_name_mapping_value_state_mutability: variable
                .type_name_mapping_value_state_mutability,
            type_name_mapping_value_user_defined_path_node: &variable
                .type_name_mapping_value_user_defined_path_node,
            type_name_mapping_value_user_defined_path: &variable
                .type_name_mapping_value_user_defined_path,
            type_name_mapping_value_user_defined_path_locations: &variable
                .type_name_mapping_value_user_defined_path_locations,
            type_name_mapping_value_array_base_types: &variable
                .type_name_mapping_value_array_base_types,
            type_name_mapping_value_array_lengths: &variable.type_name_mapping_value_array_lengths,
            type_name_mapping_value_array_length_details: CompactExpressionDetailsRef::Wire(
                &variable.type_name_mapping_value_array_length_details,
            ),
            type_name_mapping_value_function_parameters: &variable
                .type_name_mapping_value_function_parameters,
            type_name_mapping_value_function_parameter_declarations: &variable
                .type_name_mapping_value_function_parameter_declarations,
            type_name_mapping_value_function_parameter_details: CompactVariableDetailsRef::Wire(
                &variable.type_name_mapping_value_function_parameter_details,
            ),
            type_name_mapping_value_function_return_parameters: &variable
                .type_name_mapping_value_function_return_parameters,
            type_name_mapping_value_function_return_parameter_declarations: &variable
                .type_name_mapping_value_function_return_parameter_declarations,
            type_name_mapping_value_function_return_parameter_details:
                CompactVariableDetailsRef::Wire(
                    &variable.type_name_mapping_value_function_return_parameter_details,
                ),
            type_name_mapping_value_function_visibility: variable
                .type_name_mapping_value_function_visibility,
            type_name_mapping_value_function_state_mutability: variable
                .type_name_mapping_value_function_state_mutability,
            type_name_mapping_value_name: &variable.type_name_mapping_value_name,
            type_name_mapping_value_name_location: &variable.type_name_mapping_value_name_location,
            type_name_mapping_details: &variable.type_name_mapping_details,
            name: &variable.name,
            name_location: &variable.name_location,
            visibility: variable.visibility,
            mutability: variable.mutability,
            variable_location: variable.variable_location,
            indexed: variable.indexed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactPayload {
    None,
    Token {
        token: u32,
        first_number: u32,
        second_number: u32,
    },
    List {
        children: CompactChildRange,
    },
    Unary {
        child: CompactNodeRef,
        prefix: bool,
    },
    Binary {
        left: CompactNodeRef,
        right: CompactNodeRef,
    },
    Ternary {
        condition: CompactNodeRef,
        true_expression: CompactNodeRef,
        false_expression: CompactNodeRef,
    },
    OptionalChild {
        child: Option<CompactNodeRef>,
    },
    Call {
        callee: CompactNodeRef,
        arguments: CompactChildRange,
        parameter_names: CompactChildRange,
    },
    Declaration {
        type_name: Option<CompactNodeRef>,
        value: Option<CompactNodeRef>,
    },
}

impl Default for CompactPayload {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompactNode {
    pub id: CompactNodeId,
    pub kind: u8,
    pub span: CompactSpan,
    pub text: CompactTextRef,
    pub payload: CompactPayload,
}

#[derive(Clone, Debug, Default)]
pub struct CompactDiagnostic {
    pub error_id: u32,
    pub message: CompactTextRef,
    pub span: CompactSpan,
    pub secondary_locations: CompactSecondaryRange,
    pub syntax: bool,
    pub fatal: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactSecondaryRange {
    pub start: u32,
    pub len: u32,
}

#[derive(Clone, Debug, Default)]
pub struct CompactSecondaryLocation {
    pub message: CompactTextRef,
    pub span: CompactSpan,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactRefRange {
    pub start: u32,
    pub len: u32,
}

#[derive(Clone, Debug, Default)]
pub struct CompactNameLocation {
    pub text: CompactTextRef,
    pub span: CompactSpan,
}

#[derive(Clone, Debug, Default)]
pub struct CompactTokenLiteral {
    pub token: u32,
    pub literal: CompactTextRef,
}

#[derive(Clone, Debug)]
pub struct CompactImportAlias {
    pub symbol: Option<CompactNodeRef>,
    pub has_alias: bool,
    pub alias: CompactTextRef,
    pub span: CompactSpan,
}

#[derive(Clone, Debug)]
pub struct CompactPragmaDirectiveDetail {
    pub pragma_directive: Option<CompactNodeRef>,
    pub token_literals: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactImportDirectiveDetail {
    pub import_directive: Option<CompactNodeRef>,
    pub path: CompactTextRef,
    pub unit_alias: CompactTextRef,
    pub unit_alias_location: CompactSpan,
    pub symbol_aliases: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactUsingOperator {
    pub present: bool,
    pub token: u32,
}

#[derive(Clone, Debug)]
pub struct CompactContractDefinitionDetail {
    pub contract_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub documentation: Option<CompactNodeRef>,
    pub base_contracts: CompactRefRange,
    pub sub_nodes: CompactRefRange,
    pub contract_kind: u8,
    pub is_abstract: bool,
    pub storage_layout_specifier: Option<CompactNodeRef>,
    pub storage_layout_base_slot_expression: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactInheritanceSpecifierDetail {
    pub inheritance_specifier: Option<CompactNodeRef>,
    pub base_name: Option<CompactNodeRef>,
    pub base_name_path: CompactRefRange,
    pub has_arguments: bool,
    pub arguments: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactUsingDirectiveDetail {
    pub using_directive: Option<CompactNodeRef>,
    pub functions: CompactRefRange,
    pub operators: CompactRefRange,
    pub uses_braces: bool,
    pub type_name: Option<CompactNodeRef>,
    pub global: bool,
}

#[derive(Clone, Debug)]
pub struct CompactIdentifierPathDetail {
    pub identifier_path: Option<CompactNodeRef>,
    pub path: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactModifierInvocationDetail {
    pub modifier_invocation: Option<CompactNodeRef>,
    pub modifier_name: Option<CompactNodeRef>,
    pub modifier_name_path: CompactRefRange,
    pub has_arguments: bool,
    pub arguments: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactFunctionDefinitionDetail {
    pub function_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub visibility: u8,
    pub state_mutability: u8,
    pub is_free_function: bool,
    pub kind: u32,
    pub is_virtual: bool,
    pub documentation: Option<CompactNodeRef>,
    pub overrides: Option<CompactNodeRef>,
    pub override_paths: CompactRefRange,
    pub parameters: Option<CompactNodeRef>,
    pub parameter_declarations: CompactRefRange,
    pub modifiers: CompactRefRange,
    pub return_parameters: Option<CompactNodeRef>,
    pub return_parameter_declarations: CompactRefRange,
    pub block: Option<CompactNodeRef>,
    pub block_unchecked: bool,
    pub block_statements: CompactRefRange,
    pub experimental_return_expression: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactModifierDefinitionDetail {
    pub modifier_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub documentation: Option<CompactNodeRef>,
    pub parameters: Option<CompactNodeRef>,
    pub parameter_declarations: CompactRefRange,
    pub is_virtual: bool,
    pub overrides: Option<CompactNodeRef>,
    pub override_paths: CompactRefRange,
    pub block: Option<CompactNodeRef>,
    pub block_unchecked: bool,
    pub block_statements: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactEnumValueDetail {
    pub enum_value: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub documentation: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactEnumDefinitionDetail {
    pub enum_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub members: CompactRefRange,
    pub documentation: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactStructDefinitionDetail {
    pub struct_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub members: CompactRefRange,
    pub documentation: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactEventDefinitionDetail {
    pub event_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub documentation: Option<CompactNodeRef>,
    pub parameters: Option<CompactNodeRef>,
    pub parameter_declarations: CompactRefRange,
    pub anonymous: bool,
}

#[derive(Clone, Debug)]
pub struct CompactErrorDefinitionDetail {
    pub error_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub documentation: Option<CompactNodeRef>,
    pub parameters: Option<CompactNodeRef>,
    pub parameter_declarations: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactUserDefinedValueTypeDefinitionDetail {
    pub user_defined_value_type_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub type_name: Option<CompactNodeRef>,
    pub type_name_elementary_token: u32,
    pub type_name_elementary_first_number: u32,
    pub type_name_elementary_second_number: u32,
    pub type_name_has_state_mutability: bool,
    pub type_name_state_mutability: u8,
}

#[derive(Clone, Debug)]
pub struct CompactForAllQuantifierDetail {
    pub for_all_quantifier: Option<CompactNodeRef>,
    pub type_variable_declarations: Option<CompactNodeRef>,
    pub type_variable_declaration_parameters: CompactRefRange,
    pub quantified_function: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactTypeDefinitionDetail {
    pub type_definition: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub arguments: Option<CompactNodeRef>,
    pub argument_parameters: CompactRefRange,
    pub expression: Option<CompactNodeRef>,
    pub has_builtin_name_parameter: bool,
    pub builtin_name_parameter: CompactTextRef,
    pub builtin_name_parameter_location: CompactSpan,
}

#[derive(Clone, Debug)]
pub struct CompactTypeClassNameDetail {
    pub type_class_name: Option<CompactNodeRef>,
    pub is_builtin: bool,
    pub builtin_token: u32,
    pub identifier_path: Option<CompactNodeRef>,
}

#[derive(Clone, Debug)]
pub struct CompactTypeClassDefinitionDetail {
    pub type_class_definition: Option<CompactNodeRef>,
    pub type_variable: Option<CompactNodeRef>,
    pub type_variable_name: CompactTextRef,
    pub type_variable_name_location: CompactSpan,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub documentation: Option<CompactNodeRef>,
    pub sub_nodes: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactTypeClassInstantiationDetail {
    pub type_class_instantiation: Option<CompactNodeRef>,
    pub type_constructor: Option<CompactNodeRef>,
    pub argument_sorts: Option<CompactNodeRef>,
    pub argument_sort_parameters: CompactRefRange,
    pub type_class_name: Option<CompactNodeRef>,
    pub sub_nodes: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactMappingTypeName {
    pub mapping: Option<CompactNodeRef>,
    pub key_type: Option<CompactNodeRef>,
    pub key_elementary_token: u32,
    pub key_elementary_first_number: u32,
    pub key_elementary_second_number: u32,
    pub key_user_defined_path_node: Option<CompactNodeRef>,
    pub key_user_defined_path: CompactRefRange,
    pub key_name: CompactTextRef,
    pub key_name_location: CompactSpan,
    pub value_type: Option<CompactNodeRef>,
    pub value_elementary_token: u32,
    pub value_elementary_first_number: u32,
    pub value_elementary_second_number: u32,
    pub value_has_state_mutability: bool,
    pub value_state_mutability: u8,
    pub value_user_defined_path_node: Option<CompactNodeRef>,
    pub value_user_defined_path: CompactRefRange,
    pub value_array_base_types: CompactRefRange,
    pub value_array_lengths: CompactRefRange,
    pub value_function_parameters: Option<CompactNodeRef>,
    pub value_function_parameter_declarations: CompactRefRange,
    pub value_function_return_parameters: Option<CompactNodeRef>,
    pub value_function_return_parameter_declarations: CompactRefRange,
    pub value_function_visibility: u8,
    pub value_function_state_mutability: u8,
    pub value_name: CompactTextRef,
    pub value_name_location: CompactSpan,
}

#[derive(Clone, Debug)]
pub struct CompactTypeNameDetail {
    pub type_name: Option<CompactNodeRef>,
    pub elementary_token: u32,
    pub elementary_first_number: u32,
    pub elementary_second_number: u32,
    pub has_state_mutability: bool,
    pub state_mutability: u8,
    pub user_defined_path_node: Option<CompactNodeRef>,
    pub user_defined_path: CompactRefRange,
    pub array_base_types: CompactRefRange,
    pub array_lengths: CompactRefRange,
    pub function_parameters: Option<CompactNodeRef>,
    pub function_parameter_declarations: CompactRefRange,
    pub function_return_parameters: Option<CompactNodeRef>,
    pub function_return_parameter_declarations: CompactRefRange,
    pub function_visibility: u8,
    pub function_state_mutability: u8,
    pub mapping_key_type: Option<CompactNodeRef>,
    pub mapping_key_elementary_token: u32,
    pub mapping_key_elementary_first_number: u32,
    pub mapping_key_elementary_second_number: u32,
    pub mapping_key_user_defined_path_node: Option<CompactNodeRef>,
    pub mapping_key_user_defined_path: CompactRefRange,
    pub mapping_key_name: CompactTextRef,
    pub mapping_key_name_location: CompactSpan,
    pub mapping_value_type: Option<CompactNodeRef>,
    pub mapping_value_elementary_token: u32,
    pub mapping_value_elementary_first_number: u32,
    pub mapping_value_elementary_second_number: u32,
    pub mapping_value_has_state_mutability: bool,
    pub mapping_value_state_mutability: u8,
    pub mapping_value_user_defined_path_node: Option<CompactNodeRef>,
    pub mapping_value_user_defined_path: CompactRefRange,
    pub mapping_value_array_base_types: CompactRefRange,
    pub mapping_value_array_lengths: CompactRefRange,
    pub mapping_value_function_parameters: Option<CompactNodeRef>,
    pub mapping_value_function_parameter_declarations: CompactRefRange,
    pub mapping_value_function_return_parameters: Option<CompactNodeRef>,
    pub mapping_value_function_return_parameter_declarations: CompactRefRange,
    pub mapping_value_function_visibility: u8,
    pub mapping_value_function_state_mutability: u8,
    pub mapping_value_name: CompactTextRef,
    pub mapping_value_name_location: CompactSpan,
    pub mapping_details: CompactRefRange,
}

#[derive(Clone, Debug)]
pub struct CompactVariableDeclarationDetail {
    pub variable_declaration: Option<CompactNodeRef>,
    pub type_name: Option<CompactNodeRef>,
    pub type_expression: Option<CompactNodeRef>,
    pub documentation: Option<CompactNodeRef>,
    pub overrides: Option<CompactNodeRef>,
    pub override_paths: CompactRefRange,
    pub value: Option<CompactNodeRef>,
    pub name: CompactTextRef,
    pub name_location: CompactSpan,
    pub visibility: u8,
    pub mutability: u8,
    pub variable_location: u8,
    pub indexed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CompactExpressionDetail {
    pub expression: Option<CompactNodeRef>,
    pub left_expression: Option<CompactNodeRef>,
    pub right_expression: Option<CompactNodeRef>,
    pub condition_expression: Option<CompactNodeRef>,
    pub true_expression: Option<CompactNodeRef>,
    pub false_expression: Option<CompactNodeRef>,
    pub sub_expression: Option<CompactNodeRef>,
    pub base_expression: Option<CompactNodeRef>,
    pub base_expression_type: Option<CompactNodeRef>,
    pub index_expression: Option<CompactNodeRef>,
    pub end_index_expression: Option<CompactNodeRef>,
    pub type_name: Option<CompactNodeRef>,
    pub expression_type: Option<CompactNodeRef>,
    pub arguments: CompactRefRange,
    pub argument_names: CompactRefRange,
    pub components: CompactRefRange,
    pub member_name_location: CompactSpan,
    pub is_prefix_operation: bool,
    pub is_inline_array: bool,
    pub literal_token: u32,
    pub literal_subdenomination: u32,
}

#[derive(Clone, Debug)]
pub struct CompactTryCatchClauseDetail {
    pub try_catch_clause: Option<CompactNodeRef>,
    pub error_name: CompactTextRef,
    pub error_parameters: Option<CompactNodeRef>,
    pub error_parameter_declarations: CompactRefRange,
    pub block: Option<CompactNodeRef>,
    pub block_unchecked: bool,
    pub block_statements: CompactRefRange,
}

#[derive(Clone, Debug, Default)]
pub struct CompactStatementDetail {
    pub statement: Option<CompactNodeRef>,
    pub block_unchecked: bool,
    pub block_statements: CompactRefRange,
    pub inline_assembly_flags: CompactRefRange,
    pub inline_assembly_block_location: CompactSpan,
    pub condition_expression: Option<CompactNodeRef>,
    pub true_body: Option<CompactNodeRef>,
    pub false_body: Option<CompactNodeRef>,
    pub body: Option<CompactNodeRef>,
    pub is_do_while: bool,
    pub external_call: Option<CompactNodeRef>,
    pub clauses: CompactRefRange,
    pub clause_error_parameters: CompactRefRange,
    pub clause_blocks: CompactRefRange,
    pub init_expression: Option<CompactNodeRef>,
    pub loop_expression: Option<CompactNodeRef>,
    pub event_call: Option<CompactNodeRef>,
    pub event_call_callee: Option<CompactNodeRef>,
    pub event_call_arguments: CompactRefRange,
    pub event_call_parameter_names: CompactRefRange,
    pub error_call: Option<CompactNodeRef>,
    pub error_call_callee: Option<CompactNodeRef>,
    pub error_call_arguments: CompactRefRange,
    pub error_call_parameter_names: CompactRefRange,
    pub expression: Option<CompactNodeRef>,
    pub variables: CompactRefRange,
    pub initial_value: Option<CompactNodeRef>,
}

#[derive(Clone, Debug, Default)]
pub struct CompactParseOutput {
    pub ok: bool,
    pub error_code: u8,
    pub error_message: CompactTextRef,
    pub root: Option<CompactNodeRef>,
    pub nodes: Vec<CompactNode>,
    pub children: Vec<CompactNodeRef>,
    pub text: Vec<u8>,
    pub secondary_locations: Vec<CompactSecondaryLocation>,
    pub ref_items: Vec<CompactNodeRef>,
    pub name_locations: Vec<CompactNameLocation>,
    pub token_literals: Vec<CompactTokenLiteral>,
    pub import_aliases: Vec<CompactImportAlias>,
    pub using_operators: Vec<CompactUsingOperator>,
    pub pragma_directive_details: Vec<CompactPragmaDirectiveDetail>,
    pub import_directive_details: Vec<CompactImportDirectiveDetail>,
    pub contract_definition_details: Vec<CompactContractDefinitionDetail>,
    pub inheritance_specifier_details: Vec<CompactInheritanceSpecifierDetail>,
    pub using_directive_details: Vec<CompactUsingDirectiveDetail>,
    pub identifier_path_details: Vec<CompactIdentifierPathDetail>,
    pub modifier_invocation_details: Vec<CompactModifierInvocationDetail>,
    pub function_definition_details: Vec<CompactFunctionDefinitionDetail>,
    pub modifier_definition_details: Vec<CompactModifierDefinitionDetail>,
    pub enum_value_details: Vec<CompactEnumValueDetail>,
    pub enum_definition_details: Vec<CompactEnumDefinitionDetail>,
    pub struct_definition_details: Vec<CompactStructDefinitionDetail>,
    pub event_definition_details: Vec<CompactEventDefinitionDetail>,
    pub error_definition_details: Vec<CompactErrorDefinitionDetail>,
    pub user_defined_value_type_definition_details:
        Vec<CompactUserDefinedValueTypeDefinitionDetail>,
    pub for_all_quantifier_details: Vec<CompactForAllQuantifierDetail>,
    pub type_definition_details: Vec<CompactTypeDefinitionDetail>,
    pub type_class_name_details: Vec<CompactTypeClassNameDetail>,
    pub type_class_definition_details: Vec<CompactTypeClassDefinitionDetail>,
    pub type_class_instantiation_details: Vec<CompactTypeClassInstantiationDetail>,
    pub mapping_type_names: Vec<CompactMappingTypeName>,
    pub type_name_details: Vec<CompactTypeNameDetail>,
    pub variable_declaration_details: Vec<CompactVariableDeclarationDetail>,
    pub expression_details: Vec<CompactExpressionDetail>,
    pub statement_details: Vec<CompactStatementDetail>,
    pub try_catch_clause_details: Vec<CompactTryCatchClauseDetail>,
    pub diagnostics: Vec<CompactDiagnostic>,
    pub warnings: Vec<CompactDiagnostic>,
    pub has_license: bool,
    pub max_id: CompactNodeId,
    pub experimental_solidity: bool,
    pub license: CompactTextRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactAdapterError {
    MissingRoot,
    InvalidNode(CompactNodeRef),
    UnsupportedNodeKind(u8),
    InvalidChildRange(CompactChildRange),
}

pub trait CompactOutputAdapter {
    type Output;

    fn lower(&mut self, output: &CompactParseOutput) -> Result<Self::Output, CompactAdapterError>;
}

impl CompactParseOutput {
    pub fn from_wire_parser_result(result: &ffi::WireParserResult) -> Self {
        CompactArenaBuilder::new().build(result)
    }

    pub fn from_wire_parser_result_owned(result: ffi::WireParserResult) -> Self {
        CompactArenaBuilder::new().build_owned(result)
    }

    pub fn intern_text(&mut self, bytes: &[u8]) -> CompactTextRef {
        if bytes.is_empty() {
            return CompactTextRef::empty();
        }
        let start = self.text.len() as u32;
        self.text.extend_from_slice(bytes);
        CompactTextRef {
            start,
            len: bytes.len() as u32,
        }
    }

    pub fn push_children(
        &mut self,
        children: impl IntoIterator<Item = CompactNodeRef>,
    ) -> CompactChildRange {
        let start = self.children.len() as u32;
        self.children.extend(children);
        CompactChildRange {
            start,
            len: self.children.len() as u32 - start,
        }
    }

    pub fn push_refs(&mut self, refs: impl IntoIterator<Item = CompactNodeRef>) -> CompactRefRange {
        let start = self.ref_items.len() as u32;
        self.ref_items.extend(refs);
        CompactRefRange {
            start,
            len: self.ref_items.len() as u32 - start,
        }
    }

    pub fn push_name_locations(
        &mut self,
        names: &[ffi::WireString],
        locations: &[ffi::WireSourceLocation],
    ) -> CompactRefRange {
        let start = self.name_locations.len() as u32;
        for (name, location) in names.iter().zip(locations) {
            let text = self.intern_text(&name.bytes);
            self.name_locations.push(CompactNameLocation {
                text,
                span: CompactSpan::from(location),
            });
        }
        CompactRefRange {
            start,
            len: self.name_locations.len() as u32 - start,
        }
    }

    pub fn push_names_without_locations(&mut self, names: &[ffi::WireString]) -> CompactRefRange {
        let start = self.name_locations.len() as u32;
        for name in names {
            let text = self.intern_text(&name.bytes);
            self.name_locations.push(CompactNameLocation {
                text,
                span: CompactSpan::default(),
            });
        }
        CompactRefRange {
            start,
            len: self.name_locations.len() as u32 - start,
        }
    }

    pub fn push_token_literals(
        &mut self,
        tokens: &[u32],
        literals: &[ffi::WireString],
    ) -> CompactRefRange {
        let start = self.token_literals.len() as u32;
        for (token, literal) in tokens.iter().zip(literals) {
            let literal = self.intern_text(&literal.bytes);
            self.token_literals.push(CompactTokenLiteral {
                token: *token,
                literal,
            });
        }
        CompactRefRange {
            start,
            len: self.token_literals.len() as u32 - start,
        }
    }

    pub fn push_import_aliases(
        &mut self,
        aliases: impl IntoIterator<Item = CompactImportAlias>,
    ) -> CompactRefRange {
        let start = self.import_aliases.len() as u32;
        self.import_aliases.extend(aliases);
        CompactRefRange {
            start,
            len: self.import_aliases.len() as u32 - start,
        }
    }

    pub fn push_using_operators(
        &mut self,
        operators: impl IntoIterator<Item = CompactUsingOperator>,
    ) -> CompactRefRange {
        let start = self.using_operators.len() as u32;
        self.using_operators.extend(operators);
        CompactRefRange {
            start,
            len: self.using_operators.len() as u32 - start,
        }
    }

    pub fn push_node(&mut self, node: CompactNode) -> CompactNodeRef {
        let index = self.nodes.len() as u32;
        self.max_id = self.max_id.max(node.id);
        self.nodes.push(node);
        CompactNodeRef(index)
    }

    pub fn text(&self, text: CompactTextRef) -> &[u8] {
        let start = text.start as usize;
        let end = start.saturating_add(text.len as usize);
        self.text.get(start..end).unwrap_or_default()
    }

    pub fn node(&self, node: CompactNodeRef) -> Option<&CompactNode> {
        self.nodes.get(node.0 as usize)
    }

    pub fn children(&self, range: CompactChildRange) -> Option<&[CompactNodeRef]> {
        let start = range.start as usize;
        let end = start.checked_add(range.len as usize)?;
        self.children.get(start..end)
    }

    pub fn into_wire(self) -> ffi::WireCompactParseOutput {
        let error_message = String::from_utf8_lossy(self.text(self.error_message)).into_owned();
        ffi::WireCompactParseOutput {
            ok: self.ok,
            error_code: self.error_code,
            error_message,
            has_root: self.root.is_some(),
            root: wire_compact_node_ref(self.root.unwrap_or_default()),
            nodes: self.nodes.into_iter().map(wire_compact_node).collect(),
            children: self
                .children
                .into_iter()
                .map(wire_compact_node_ref)
                .collect(),
            text: self.text,
            secondary_locations: self
                .secondary_locations
                .into_iter()
                .map(wire_compact_secondary_location)
                .collect(),
            ref_items: self
                .ref_items
                .into_iter()
                .map(wire_compact_node_ref)
                .collect(),
            name_locations: self
                .name_locations
                .into_iter()
                .map(wire_compact_name_location)
                .collect(),
            token_literals: self
                .token_literals
                .into_iter()
                .map(wire_compact_token_literal)
                .collect(),
            import_aliases: self
                .import_aliases
                .into_iter()
                .map(wire_compact_import_alias)
                .collect(),
            using_operators: self
                .using_operators
                .into_iter()
                .map(wire_compact_using_operator)
                .collect(),
            pragma_directive_details: self
                .pragma_directive_details
                .into_iter()
                .map(wire_compact_pragma_directive_detail)
                .collect(),
            import_directive_details: self
                .import_directive_details
                .into_iter()
                .map(wire_compact_import_directive_detail)
                .collect(),
            contract_definition_details: self
                .contract_definition_details
                .into_iter()
                .map(wire_compact_contract_definition_detail)
                .collect(),
            inheritance_specifier_details: self
                .inheritance_specifier_details
                .into_iter()
                .map(wire_compact_inheritance_specifier_detail)
                .collect(),
            using_directive_details: self
                .using_directive_details
                .into_iter()
                .map(wire_compact_using_directive_detail)
                .collect(),
            identifier_path_details: self
                .identifier_path_details
                .into_iter()
                .map(wire_compact_identifier_path_detail)
                .collect(),
            modifier_invocation_details: self
                .modifier_invocation_details
                .into_iter()
                .map(wire_compact_modifier_invocation_detail)
                .collect(),
            function_definition_details: self
                .function_definition_details
                .into_iter()
                .map(wire_compact_function_definition_detail)
                .collect(),
            modifier_definition_details: self
                .modifier_definition_details
                .into_iter()
                .map(wire_compact_modifier_definition_detail)
                .collect(),
            enum_value_details: self
                .enum_value_details
                .into_iter()
                .map(wire_compact_enum_value_detail)
                .collect(),
            enum_definition_details: self
                .enum_definition_details
                .into_iter()
                .map(wire_compact_enum_definition_detail)
                .collect(),
            struct_definition_details: self
                .struct_definition_details
                .into_iter()
                .map(wire_compact_struct_definition_detail)
                .collect(),
            event_definition_details: self
                .event_definition_details
                .into_iter()
                .map(wire_compact_event_definition_detail)
                .collect(),
            error_definition_details: self
                .error_definition_details
                .into_iter()
                .map(wire_compact_error_definition_detail)
                .collect(),
            user_defined_value_type_definition_details: self
                .user_defined_value_type_definition_details
                .into_iter()
                .map(wire_compact_user_defined_value_type_definition_detail)
                .collect(),
            for_all_quantifier_details: self
                .for_all_quantifier_details
                .into_iter()
                .map(wire_compact_for_all_quantifier_detail)
                .collect(),
            type_definition_details: self
                .type_definition_details
                .into_iter()
                .map(wire_compact_type_definition_detail)
                .collect(),
            type_class_name_details: self
                .type_class_name_details
                .into_iter()
                .map(wire_compact_type_class_name_detail)
                .collect(),
            type_class_definition_details: self
                .type_class_definition_details
                .into_iter()
                .map(wire_compact_type_class_definition_detail)
                .collect(),
            type_class_instantiation_details: self
                .type_class_instantiation_details
                .into_iter()
                .map(wire_compact_type_class_instantiation_detail)
                .collect(),
            mapping_type_names: self
                .mapping_type_names
                .into_iter()
                .map(wire_compact_mapping_type_name)
                .collect(),
            type_name_details: self
                .type_name_details
                .into_iter()
                .map(wire_compact_type_name_detail)
                .collect(),
            variable_declaration_details: self
                .variable_declaration_details
                .into_iter()
                .map(wire_compact_variable_declaration_detail)
                .collect(),
            expression_details: self
                .expression_details
                .into_iter()
                .map(wire_compact_expression_detail)
                .collect(),
            statement_details: self
                .statement_details
                .into_iter()
                .map(wire_compact_statement_detail)
                .collect(),
            try_catch_clause_details: self
                .try_catch_clause_details
                .into_iter()
                .map(wire_compact_try_catch_clause_detail)
                .collect(),
            diagnostics: self
                .diagnostics
                .into_iter()
                .map(wire_compact_diagnostic)
                .collect(),
            warnings: self
                .warnings
                .into_iter()
                .map(wire_compact_diagnostic)
                .collect(),
            has_license: self.has_license,
            max_id: self.max_id,
            experimental_solidity: self.experimental_solidity,
            license: wire_compact_text_ref(self.license),
        }
    }
}

fn wire_compact_node_ref(node: CompactNodeRef) -> ffi::WireCompactNodeRef {
    ffi::WireCompactNodeRef { index: node.0 }
}

fn wire_optional_compact_node_ref(node: Option<CompactNodeRef>) -> ffi::WireCompactNodeRef {
    ffi::WireCompactNodeRef {
        index: node.map_or(INVALID_COMPACT_NODE_REF, |node| node.0),
    }
}

fn wire_compact_child_range(range: CompactChildRange) -> ffi::WireCompactChildRange {
    ffi::WireCompactChildRange {
        start: range.start,
        len: range.len,
    }
}

fn wire_compact_ref_range(range: CompactRefRange) -> ffi::WireCompactRefRange {
    ffi::WireCompactRefRange {
        start: range.start,
        len: range.len,
    }
}

fn empty_wire_compact_child_range() -> ffi::WireCompactChildRange {
    wire_compact_child_range(CompactChildRange::empty())
}

fn wire_compact_text_ref(text: CompactTextRef) -> ffi::WireCompactTextRef {
    ffi::WireCompactTextRef {
        start: text.start,
        len: text.len,
    }
}

fn wire_compact_span(span: CompactSpan) -> ffi::WireCompactSpan {
    ffi::WireCompactSpan {
        start: span.start,
        end: span.end,
        source_id: span.source_id,
    }
}

fn wire_compact_node(node: CompactNode) -> ffi::WireCompactNode {
    let mut payload_kind = WIRE_COMPACT_PAYLOAD_NONE;
    let mut child_range = empty_wire_compact_child_range();
    let mut aux_child_range = empty_wire_compact_child_range();
    let mut first_child = CompactNodeRef::default();
    let mut second_child = CompactNodeRef::default();
    let mut third_child = CompactNodeRef::default();
    let mut has_first_child = false;
    let mut has_second_child = false;
    let mut has_third_child = false;
    let mut prefix = false;
    let mut token = 0;
    let mut first_number = 0;
    let mut second_number = 0;

    match node.payload {
        CompactPayload::None => {}
        CompactPayload::Token {
            token: payload_token,
            first_number: payload_first_number,
            second_number: payload_second_number,
        } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_TOKEN;
            token = payload_token;
            first_number = payload_first_number;
            second_number = payload_second_number;
        }
        CompactPayload::List { children } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_LIST;
            child_range = wire_compact_child_range(children);
        }
        CompactPayload::Unary {
            child,
            prefix: payload_prefix,
        } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_UNARY;
            first_child = child;
            has_first_child = true;
            prefix = payload_prefix;
        }
        CompactPayload::Binary { left, right } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_BINARY;
            first_child = left;
            second_child = right;
            has_first_child = true;
            has_second_child = true;
        }
        CompactPayload::Ternary {
            condition,
            true_expression,
            false_expression,
        } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_TERNARY;
            first_child = condition;
            second_child = true_expression;
            third_child = false_expression;
            has_first_child = true;
            has_second_child = true;
            has_third_child = true;
        }
        CompactPayload::OptionalChild { child } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_OPTIONAL_CHILD;
            if let Some(child) = child {
                first_child = child;
                has_first_child = true;
            }
        }
        CompactPayload::Call {
            callee,
            arguments,
            parameter_names,
        } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_CALL;
            first_child = callee;
            has_first_child = true;
            child_range = wire_compact_child_range(arguments);
            aux_child_range = wire_compact_child_range(parameter_names);
        }
        CompactPayload::Declaration { type_name, value } => {
            payload_kind = WIRE_COMPACT_PAYLOAD_DECLARATION;
            if let Some(type_name) = type_name {
                first_child = type_name;
                has_first_child = true;
            }
            if let Some(value) = value {
                second_child = value;
                has_second_child = true;
            }
        }
    }

    ffi::WireCompactNode {
        id: node.id,
        kind: node.kind,
        span: wire_compact_span(node.span),
        text: wire_compact_text_ref(node.text),
        payload_kind,
        child_range,
        aux_child_range,
        first_child: wire_compact_node_ref(first_child),
        second_child: wire_compact_node_ref(second_child),
        third_child: wire_compact_node_ref(third_child),
        prefix,
        has_first_child,
        has_second_child,
        has_third_child,
        token,
        first_number,
        second_number,
    }
}

fn wire_compact_secondary_location(
    location: CompactSecondaryLocation,
) -> ffi::WireCompactSecondaryLocation {
    ffi::WireCompactSecondaryLocation {
        message: wire_compact_text_ref(location.message),
        span: wire_compact_span(location.span),
    }
}

fn wire_compact_name_location(location: CompactNameLocation) -> ffi::WireCompactNameLocation {
    ffi::WireCompactNameLocation {
        text: wire_compact_text_ref(location.text),
        span: wire_compact_span(location.span),
    }
}

fn wire_compact_token_literal(token_literal: CompactTokenLiteral) -> ffi::WireCompactTokenLiteral {
    ffi::WireCompactTokenLiteral {
        token: token_literal.token,
        literal: wire_compact_text_ref(token_literal.literal),
    }
}

fn wire_compact_import_alias(alias: CompactImportAlias) -> ffi::WireCompactImportAlias {
    ffi::WireCompactImportAlias {
        symbol: wire_optional_compact_node_ref(alias.symbol),
        has_alias: alias.has_alias,
        alias: wire_compact_text_ref(alias.alias),
        span: wire_compact_span(alias.span),
    }
}

fn wire_compact_using_operator(operator: CompactUsingOperator) -> ffi::WireCompactUsingOperator {
    ffi::WireCompactUsingOperator {
        present: operator.present,
        token: operator.token,
    }
}

fn wire_compact_pragma_directive_detail(
    detail: CompactPragmaDirectiveDetail,
) -> ffi::WireCompactPragmaDirectiveDetail {
    ffi::WireCompactPragmaDirectiveDetail {
        pragma_directive: wire_optional_compact_node_ref(detail.pragma_directive),
        token_literals: wire_compact_ref_range(detail.token_literals),
    }
}

fn wire_compact_import_directive_detail(
    detail: CompactImportDirectiveDetail,
) -> ffi::WireCompactImportDirectiveDetail {
    ffi::WireCompactImportDirectiveDetail {
        import_directive: wire_optional_compact_node_ref(detail.import_directive),
        path: wire_compact_text_ref(detail.path),
        unit_alias: wire_compact_text_ref(detail.unit_alias),
        unit_alias_location: wire_compact_span(detail.unit_alias_location),
        symbol_aliases: wire_compact_ref_range(detail.symbol_aliases),
    }
}

fn wire_compact_contract_definition_detail(
    detail: CompactContractDefinitionDetail,
) -> ffi::WireCompactContractDefinitionDetail {
    ffi::WireCompactContractDefinitionDetail {
        contract_definition: wire_optional_compact_node_ref(detail.contract_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        documentation: wire_optional_compact_node_ref(detail.documentation),
        base_contracts: wire_compact_ref_range(detail.base_contracts),
        sub_nodes: wire_compact_ref_range(detail.sub_nodes),
        contract_kind: detail.contract_kind,
        is_abstract: detail.is_abstract,
        storage_layout_specifier: wire_optional_compact_node_ref(detail.storage_layout_specifier),
        storage_layout_base_slot_expression: wire_optional_compact_node_ref(
            detail.storage_layout_base_slot_expression,
        ),
    }
}

fn wire_compact_inheritance_specifier_detail(
    detail: CompactInheritanceSpecifierDetail,
) -> ffi::WireCompactInheritanceSpecifierDetail {
    ffi::WireCompactInheritanceSpecifierDetail {
        inheritance_specifier: wire_optional_compact_node_ref(detail.inheritance_specifier),
        base_name: wire_optional_compact_node_ref(detail.base_name),
        base_name_path: wire_compact_ref_range(detail.base_name_path),
        has_arguments: detail.has_arguments,
        arguments: wire_compact_ref_range(detail.arguments),
    }
}

fn wire_compact_using_directive_detail(
    detail: CompactUsingDirectiveDetail,
) -> ffi::WireCompactUsingDirectiveDetail {
    ffi::WireCompactUsingDirectiveDetail {
        using_directive: wire_optional_compact_node_ref(detail.using_directive),
        functions: wire_compact_ref_range(detail.functions),
        operators: wire_compact_ref_range(detail.operators),
        uses_braces: detail.uses_braces,
        type_name: wire_optional_compact_node_ref(detail.type_name),
        global: detail.global,
    }
}

fn wire_compact_identifier_path_detail(
    detail: CompactIdentifierPathDetail,
) -> ffi::WireCompactIdentifierPathDetail {
    ffi::WireCompactIdentifierPathDetail {
        identifier_path: wire_optional_compact_node_ref(detail.identifier_path),
        path: wire_compact_ref_range(detail.path),
    }
}

fn wire_compact_modifier_invocation_detail(
    detail: CompactModifierInvocationDetail,
) -> ffi::WireCompactModifierInvocationDetail {
    ffi::WireCompactModifierInvocationDetail {
        modifier_invocation: wire_optional_compact_node_ref(detail.modifier_invocation),
        modifier_name: wire_optional_compact_node_ref(detail.modifier_name),
        modifier_name_path: wire_compact_ref_range(detail.modifier_name_path),
        has_arguments: detail.has_arguments,
        arguments: wire_compact_ref_range(detail.arguments),
    }
}

fn wire_compact_function_definition_detail(
    detail: CompactFunctionDefinitionDetail,
) -> ffi::WireCompactFunctionDefinitionDetail {
    ffi::WireCompactFunctionDefinitionDetail {
        function_definition: wire_optional_compact_node_ref(detail.function_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        visibility: detail.visibility,
        state_mutability: detail.state_mutability,
        is_free_function: detail.is_free_function,
        kind: detail.kind,
        is_virtual: detail.is_virtual,
        documentation: wire_optional_compact_node_ref(detail.documentation),
        overrides: wire_optional_compact_node_ref(detail.overrides),
        override_paths: wire_compact_ref_range(detail.override_paths),
        parameters: wire_optional_compact_node_ref(detail.parameters),
        parameter_declarations: wire_compact_ref_range(detail.parameter_declarations),
        modifiers: wire_compact_ref_range(detail.modifiers),
        return_parameters: wire_optional_compact_node_ref(detail.return_parameters),
        return_parameter_declarations: wire_compact_ref_range(detail.return_parameter_declarations),
        block: wire_optional_compact_node_ref(detail.block),
        block_unchecked: detail.block_unchecked,
        block_statements: wire_compact_ref_range(detail.block_statements),
        experimental_return_expression: wire_optional_compact_node_ref(
            detail.experimental_return_expression,
        ),
    }
}

fn wire_compact_modifier_definition_detail(
    detail: CompactModifierDefinitionDetail,
) -> ffi::WireCompactModifierDefinitionDetail {
    ffi::WireCompactModifierDefinitionDetail {
        modifier_definition: wire_optional_compact_node_ref(detail.modifier_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        documentation: wire_optional_compact_node_ref(detail.documentation),
        parameters: wire_optional_compact_node_ref(detail.parameters),
        parameter_declarations: wire_compact_ref_range(detail.parameter_declarations),
        is_virtual: detail.is_virtual,
        overrides: wire_optional_compact_node_ref(detail.overrides),
        override_paths: wire_compact_ref_range(detail.override_paths),
        block: wire_optional_compact_node_ref(detail.block),
        block_unchecked: detail.block_unchecked,
        block_statements: wire_compact_ref_range(detail.block_statements),
    }
}

fn wire_compact_enum_value_detail(
    detail: CompactEnumValueDetail,
) -> ffi::WireCompactEnumValueDetail {
    ffi::WireCompactEnumValueDetail {
        enum_value: wire_optional_compact_node_ref(detail.enum_value),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        documentation: wire_optional_compact_node_ref(detail.documentation),
    }
}

fn wire_compact_enum_definition_detail(
    detail: CompactEnumDefinitionDetail,
) -> ffi::WireCompactEnumDefinitionDetail {
    ffi::WireCompactEnumDefinitionDetail {
        enum_definition: wire_optional_compact_node_ref(detail.enum_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        members: wire_compact_ref_range(detail.members),
        documentation: wire_optional_compact_node_ref(detail.documentation),
    }
}

fn wire_compact_struct_definition_detail(
    detail: CompactStructDefinitionDetail,
) -> ffi::WireCompactStructDefinitionDetail {
    ffi::WireCompactStructDefinitionDetail {
        struct_definition: wire_optional_compact_node_ref(detail.struct_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        members: wire_compact_ref_range(detail.members),
        documentation: wire_optional_compact_node_ref(detail.documentation),
    }
}

fn wire_compact_event_definition_detail(
    detail: CompactEventDefinitionDetail,
) -> ffi::WireCompactEventDefinitionDetail {
    ffi::WireCompactEventDefinitionDetail {
        event_definition: wire_optional_compact_node_ref(detail.event_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        documentation: wire_optional_compact_node_ref(detail.documentation),
        parameters: wire_optional_compact_node_ref(detail.parameters),
        parameter_declarations: wire_compact_ref_range(detail.parameter_declarations),
        anonymous: detail.anonymous,
    }
}

fn wire_compact_error_definition_detail(
    detail: CompactErrorDefinitionDetail,
) -> ffi::WireCompactErrorDefinitionDetail {
    ffi::WireCompactErrorDefinitionDetail {
        error_definition: wire_optional_compact_node_ref(detail.error_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        documentation: wire_optional_compact_node_ref(detail.documentation),
        parameters: wire_optional_compact_node_ref(detail.parameters),
        parameter_declarations: wire_compact_ref_range(detail.parameter_declarations),
    }
}

fn wire_compact_user_defined_value_type_definition_detail(
    detail: CompactUserDefinedValueTypeDefinitionDetail,
) -> ffi::WireCompactUserDefinedValueTypeDefinitionDetail {
    ffi::WireCompactUserDefinedValueTypeDefinitionDetail {
        user_defined_value_type_definition: wire_optional_compact_node_ref(
            detail.user_defined_value_type_definition,
        ),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        type_name: wire_optional_compact_node_ref(detail.type_name),
        type_name_elementary_token: detail.type_name_elementary_token,
        type_name_elementary_first_number: detail.type_name_elementary_first_number,
        type_name_elementary_second_number: detail.type_name_elementary_second_number,
        type_name_has_state_mutability: detail.type_name_has_state_mutability,
        type_name_state_mutability: detail.type_name_state_mutability,
    }
}

fn wire_compact_for_all_quantifier_detail(
    detail: CompactForAllQuantifierDetail,
) -> ffi::WireCompactForAllQuantifierDetail {
    ffi::WireCompactForAllQuantifierDetail {
        for_all_quantifier: wire_optional_compact_node_ref(detail.for_all_quantifier),
        type_variable_declarations: wire_optional_compact_node_ref(
            detail.type_variable_declarations,
        ),
        type_variable_declaration_parameters: wire_compact_ref_range(
            detail.type_variable_declaration_parameters,
        ),
        quantified_function: wire_optional_compact_node_ref(detail.quantified_function),
    }
}

fn wire_compact_type_definition_detail(
    detail: CompactTypeDefinitionDetail,
) -> ffi::WireCompactTypeDefinitionDetail {
    ffi::WireCompactTypeDefinitionDetail {
        type_definition: wire_optional_compact_node_ref(detail.type_definition),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        arguments: wire_optional_compact_node_ref(detail.arguments),
        argument_parameters: wire_compact_ref_range(detail.argument_parameters),
        expression: wire_optional_compact_node_ref(detail.expression),
        has_builtin_name_parameter: detail.has_builtin_name_parameter,
        builtin_name_parameter: wire_compact_text_ref(detail.builtin_name_parameter),
        builtin_name_parameter_location: wire_compact_span(detail.builtin_name_parameter_location),
    }
}

fn wire_compact_type_class_name_detail(
    detail: CompactTypeClassNameDetail,
) -> ffi::WireCompactTypeClassNameDetail {
    ffi::WireCompactTypeClassNameDetail {
        type_class_name: wire_optional_compact_node_ref(detail.type_class_name),
        is_builtin: detail.is_builtin,
        builtin_token: detail.builtin_token,
        identifier_path: wire_optional_compact_node_ref(detail.identifier_path),
    }
}

fn wire_compact_type_class_definition_detail(
    detail: CompactTypeClassDefinitionDetail,
) -> ffi::WireCompactTypeClassDefinitionDetail {
    ffi::WireCompactTypeClassDefinitionDetail {
        type_class_definition: wire_optional_compact_node_ref(detail.type_class_definition),
        type_variable: wire_optional_compact_node_ref(detail.type_variable),
        type_variable_name: wire_compact_text_ref(detail.type_variable_name),
        type_variable_name_location: wire_compact_span(detail.type_variable_name_location),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        documentation: wire_optional_compact_node_ref(detail.documentation),
        sub_nodes: wire_compact_ref_range(detail.sub_nodes),
    }
}

fn wire_compact_type_class_instantiation_detail(
    detail: CompactTypeClassInstantiationDetail,
) -> ffi::WireCompactTypeClassInstantiationDetail {
    ffi::WireCompactTypeClassInstantiationDetail {
        type_class_instantiation: wire_optional_compact_node_ref(detail.type_class_instantiation),
        type_constructor: wire_optional_compact_node_ref(detail.type_constructor),
        argument_sorts: wire_optional_compact_node_ref(detail.argument_sorts),
        argument_sort_parameters: wire_compact_ref_range(detail.argument_sort_parameters),
        type_class_name: wire_optional_compact_node_ref(detail.type_class_name),
        sub_nodes: wire_compact_ref_range(detail.sub_nodes),
    }
}

fn wire_compact_mapping_type_name(
    detail: CompactMappingTypeName,
) -> ffi::WireCompactMappingTypeName {
    ffi::WireCompactMappingTypeName {
        mapping: wire_optional_compact_node_ref(detail.mapping),
        key_type: wire_optional_compact_node_ref(detail.key_type),
        key_elementary_token: detail.key_elementary_token,
        key_elementary_first_number: detail.key_elementary_first_number,
        key_elementary_second_number: detail.key_elementary_second_number,
        key_user_defined_path_node: wire_optional_compact_node_ref(
            detail.key_user_defined_path_node,
        ),
        key_user_defined_path: wire_compact_ref_range(detail.key_user_defined_path),
        key_name: wire_compact_text_ref(detail.key_name),
        key_name_location: wire_compact_span(detail.key_name_location),
        value_type: wire_optional_compact_node_ref(detail.value_type),
        value_elementary_token: detail.value_elementary_token,
        value_elementary_first_number: detail.value_elementary_first_number,
        value_elementary_second_number: detail.value_elementary_second_number,
        value_has_state_mutability: detail.value_has_state_mutability,
        value_state_mutability: detail.value_state_mutability,
        value_user_defined_path_node: wire_optional_compact_node_ref(
            detail.value_user_defined_path_node,
        ),
        value_user_defined_path: wire_compact_ref_range(detail.value_user_defined_path),
        value_array_base_types: wire_compact_ref_range(detail.value_array_base_types),
        value_array_lengths: wire_compact_ref_range(detail.value_array_lengths),
        value_function_parameters: wire_optional_compact_node_ref(detail.value_function_parameters),
        value_function_parameter_declarations: wire_compact_ref_range(
            detail.value_function_parameter_declarations,
        ),
        value_function_return_parameters: wire_optional_compact_node_ref(
            detail.value_function_return_parameters,
        ),
        value_function_return_parameter_declarations: wire_compact_ref_range(
            detail.value_function_return_parameter_declarations,
        ),
        value_function_visibility: detail.value_function_visibility,
        value_function_state_mutability: detail.value_function_state_mutability,
        value_name: wire_compact_text_ref(detail.value_name),
        value_name_location: wire_compact_span(detail.value_name_location),
    }
}

fn wire_compact_type_name_detail(detail: CompactTypeNameDetail) -> ffi::WireCompactTypeNameDetail {
    ffi::WireCompactTypeNameDetail {
        type_name: wire_optional_compact_node_ref(detail.type_name),
        elementary_token: detail.elementary_token,
        elementary_first_number: detail.elementary_first_number,
        elementary_second_number: detail.elementary_second_number,
        has_state_mutability: detail.has_state_mutability,
        state_mutability: detail.state_mutability,
        user_defined_path_node: wire_optional_compact_node_ref(detail.user_defined_path_node),
        user_defined_path: wire_compact_ref_range(detail.user_defined_path),
        array_base_types: wire_compact_ref_range(detail.array_base_types),
        array_lengths: wire_compact_ref_range(detail.array_lengths),
        function_parameters: wire_optional_compact_node_ref(detail.function_parameters),
        function_parameter_declarations: wire_compact_ref_range(
            detail.function_parameter_declarations,
        ),
        function_return_parameters: wire_optional_compact_node_ref(
            detail.function_return_parameters,
        ),
        function_return_parameter_declarations: wire_compact_ref_range(
            detail.function_return_parameter_declarations,
        ),
        function_visibility: detail.function_visibility,
        function_state_mutability: detail.function_state_mutability,
        mapping_key_type: wire_optional_compact_node_ref(detail.mapping_key_type),
        mapping_key_elementary_token: detail.mapping_key_elementary_token,
        mapping_key_elementary_first_number: detail.mapping_key_elementary_first_number,
        mapping_key_elementary_second_number: detail.mapping_key_elementary_second_number,
        mapping_key_user_defined_path_node: wire_optional_compact_node_ref(
            detail.mapping_key_user_defined_path_node,
        ),
        mapping_key_user_defined_path: wire_compact_ref_range(detail.mapping_key_user_defined_path),
        mapping_key_name: wire_compact_text_ref(detail.mapping_key_name),
        mapping_key_name_location: wire_compact_span(detail.mapping_key_name_location),
        mapping_value_type: wire_optional_compact_node_ref(detail.mapping_value_type),
        mapping_value_elementary_token: detail.mapping_value_elementary_token,
        mapping_value_elementary_first_number: detail.mapping_value_elementary_first_number,
        mapping_value_elementary_second_number: detail.mapping_value_elementary_second_number,
        mapping_value_has_state_mutability: detail.mapping_value_has_state_mutability,
        mapping_value_state_mutability: detail.mapping_value_state_mutability,
        mapping_value_user_defined_path_node: wire_optional_compact_node_ref(
            detail.mapping_value_user_defined_path_node,
        ),
        mapping_value_user_defined_path: wire_compact_ref_range(
            detail.mapping_value_user_defined_path,
        ),
        mapping_value_array_base_types: wire_compact_ref_range(
            detail.mapping_value_array_base_types,
        ),
        mapping_value_array_lengths: wire_compact_ref_range(detail.mapping_value_array_lengths),
        mapping_value_function_parameters: wire_optional_compact_node_ref(
            detail.mapping_value_function_parameters,
        ),
        mapping_value_function_parameter_declarations: wire_compact_ref_range(
            detail.mapping_value_function_parameter_declarations,
        ),
        mapping_value_function_return_parameters: wire_optional_compact_node_ref(
            detail.mapping_value_function_return_parameters,
        ),
        mapping_value_function_return_parameter_declarations: wire_compact_ref_range(
            detail.mapping_value_function_return_parameter_declarations,
        ),
        mapping_value_function_visibility: detail.mapping_value_function_visibility,
        mapping_value_function_state_mutability: detail.mapping_value_function_state_mutability,
        mapping_value_name: wire_compact_text_ref(detail.mapping_value_name),
        mapping_value_name_location: wire_compact_span(detail.mapping_value_name_location),
        mapping_details: wire_compact_ref_range(detail.mapping_details),
    }
}

fn wire_compact_variable_declaration_detail(
    detail: CompactVariableDeclarationDetail,
) -> ffi::WireCompactVariableDeclarationDetail {
    ffi::WireCompactVariableDeclarationDetail {
        variable_declaration: wire_optional_compact_node_ref(detail.variable_declaration),
        type_name: wire_optional_compact_node_ref(detail.type_name),
        type_expression: wire_optional_compact_node_ref(detail.type_expression),
        documentation: wire_optional_compact_node_ref(detail.documentation),
        overrides: wire_optional_compact_node_ref(detail.overrides),
        override_paths: wire_compact_ref_range(detail.override_paths),
        value: wire_optional_compact_node_ref(detail.value),
        name: wire_compact_text_ref(detail.name),
        name_location: wire_compact_span(detail.name_location),
        visibility: detail.visibility,
        mutability: detail.mutability,
        variable_location: detail.variable_location,
        indexed: detail.indexed,
    }
}

fn wire_compact_expression_detail(
    detail: CompactExpressionDetail,
) -> ffi::WireCompactExpressionDetail {
    ffi::WireCompactExpressionDetail {
        expression: wire_optional_compact_node_ref(detail.expression),
        left_expression: wire_optional_compact_node_ref(detail.left_expression),
        right_expression: wire_optional_compact_node_ref(detail.right_expression),
        condition_expression: wire_optional_compact_node_ref(detail.condition_expression),
        true_expression: wire_optional_compact_node_ref(detail.true_expression),
        false_expression: wire_optional_compact_node_ref(detail.false_expression),
        sub_expression: wire_optional_compact_node_ref(detail.sub_expression),
        base_expression: wire_optional_compact_node_ref(detail.base_expression),
        base_expression_type: wire_optional_compact_node_ref(detail.base_expression_type),
        index_expression: wire_optional_compact_node_ref(detail.index_expression),
        end_index_expression: wire_optional_compact_node_ref(detail.end_index_expression),
        type_name: wire_optional_compact_node_ref(detail.type_name),
        expression_type: wire_optional_compact_node_ref(detail.expression_type),
        arguments: wire_compact_ref_range(detail.arguments),
        argument_names: wire_compact_ref_range(detail.argument_names),
        components: wire_compact_ref_range(detail.components),
        member_name_location: wire_compact_span(detail.member_name_location),
        is_prefix_operation: detail.is_prefix_operation,
        is_inline_array: detail.is_inline_array,
        literal_token: detail.literal_token,
        literal_subdenomination: detail.literal_subdenomination,
    }
}

fn wire_compact_try_catch_clause_detail(
    detail: CompactTryCatchClauseDetail,
) -> ffi::WireCompactTryCatchClauseDetail {
    ffi::WireCompactTryCatchClauseDetail {
        try_catch_clause: wire_optional_compact_node_ref(detail.try_catch_clause),
        error_name: wire_compact_text_ref(detail.error_name),
        error_parameters: wire_optional_compact_node_ref(detail.error_parameters),
        error_parameter_declarations: wire_compact_ref_range(detail.error_parameter_declarations),
        block: wire_optional_compact_node_ref(detail.block),
        block_unchecked: detail.block_unchecked,
        block_statements: wire_compact_ref_range(detail.block_statements),
    }
}

fn wire_compact_statement_detail(
    detail: CompactStatementDetail,
) -> ffi::WireCompactStatementDetail {
    ffi::WireCompactStatementDetail {
        statement: wire_optional_compact_node_ref(detail.statement),
        block_unchecked: detail.block_unchecked,
        block_statements: wire_compact_ref_range(detail.block_statements),
        inline_assembly_flags: wire_compact_ref_range(detail.inline_assembly_flags),
        inline_assembly_block_location: wire_compact_span(detail.inline_assembly_block_location),
        condition_expression: wire_optional_compact_node_ref(detail.condition_expression),
        true_body: wire_optional_compact_node_ref(detail.true_body),
        false_body: wire_optional_compact_node_ref(detail.false_body),
        body: wire_optional_compact_node_ref(detail.body),
        is_do_while: detail.is_do_while,
        external_call: wire_optional_compact_node_ref(detail.external_call),
        clauses: wire_compact_ref_range(detail.clauses),
        clause_error_parameters: wire_compact_ref_range(detail.clause_error_parameters),
        clause_blocks: wire_compact_ref_range(detail.clause_blocks),
        init_expression: wire_optional_compact_node_ref(detail.init_expression),
        loop_expression: wire_optional_compact_node_ref(detail.loop_expression),
        event_call: wire_optional_compact_node_ref(detail.event_call),
        event_call_callee: wire_optional_compact_node_ref(detail.event_call_callee),
        event_call_arguments: wire_compact_ref_range(detail.event_call_arguments),
        event_call_parameter_names: wire_compact_ref_range(detail.event_call_parameter_names),
        error_call: wire_optional_compact_node_ref(detail.error_call),
        error_call_callee: wire_optional_compact_node_ref(detail.error_call_callee),
        error_call_arguments: wire_compact_ref_range(detail.error_call_arguments),
        error_call_parameter_names: wire_compact_ref_range(detail.error_call_parameter_names),
        expression: wire_optional_compact_node_ref(detail.expression),
        variables: wire_compact_ref_range(detail.variables),
        initial_value: wire_optional_compact_node_ref(detail.initial_value),
    }
}

fn wire_compact_secondary_range(range: CompactSecondaryRange) -> ffi::WireCompactSecondaryRange {
    ffi::WireCompactSecondaryRange {
        start: range.start,
        len: range.len,
    }
}

fn wire_compact_diagnostic(diagnostic: CompactDiagnostic) -> ffi::WireCompactDiagnostic {
    ffi::WireCompactDiagnostic {
        error_id: diagnostic.error_id,
        message: wire_compact_text_ref(diagnostic.message),
        span: wire_compact_span(diagnostic.span),
        secondary_locations: wire_compact_secondary_range(diagnostic.secondary_locations),
        syntax: diagnostic.syntax,
        fatal: diagnostic.fatal,
    }
}

struct CompactArenaBuilder {
    output: CompactParseOutput,
    by_id: HashMap<CompactNodeId, CompactNodeRef>,
}

impl CompactArenaBuilder {
    fn new() -> Self {
        Self {
            output: CompactParseOutput::default(),
            by_id: HashMap::new(),
        }
    }

    fn add_parser_result_header(&mut self, result: &ffi::WireParserResult) {
        self.output.ok = result.ok;
        self.output.error_code = result.error_code;
        self.output.error_message = self.output.intern_text(result.error_message.as_bytes());
        self.output.experimental_solidity = result.experimental_solidity;
        self.output.max_id = result.max_id;
        if result.has_license {
            self.output.has_license = true;
            self.output.license = self.output.intern_text(&result.license.bytes);
        }
    }

    fn build(mut self, result: &ffi::WireParserResult) -> CompactParseOutput {
        self.add_parser_result_header(result);

        self.output.root = self.add_ast_node(&result.source_unit);
        let source_unit_children = self.add_ast_nodes(&result.source_unit_nodes);
        if let Some(root) = self.output.root {
            self.set_children(root, source_unit_children);
        }

        for pragma in &result.source_unit_pragmas {
            self.add_pragma(pragma);
        }
        for import in &result.source_unit_imports {
            self.add_import(import);
        }
        for value_type in &result.source_unit_user_defined_value_types {
            self.add_user_defined_value_type(value_type);
        }
        for enum_definition in &result.source_unit_enums {
            self.add_enum_definition(enum_definition);
        }
        for struct_definition in &result.source_unit_structs {
            self.add_struct_definition(struct_definition);
        }
        for event in &result.source_unit_events {
            self.add_event_definition(event);
        }
        for error in &result.source_unit_errors {
            self.add_error_definition(error);
        }
        for contract in &result.source_unit_contracts {
            self.add_contract(contract);
        }
        for function in &result.source_unit_functions {
            self.add_function(function);
        }
        for quantifier in &result.source_unit_for_all_quantifiers {
            self.add_for_all_quantifier(quantifier);
        }
        for type_definition in &result.source_unit_type_definitions {
            self.add_type_definition(type_definition);
        }
        for type_class in &result.source_unit_type_class_definitions {
            self.add_type_class_definition(type_class);
        }
        for instantiation in &result.source_unit_type_class_instantiations {
            self.add_type_class_instantiation(instantiation);
        }
        for using_directive in &result.source_unit_using_directives {
            self.add_using_directive(using_directive);
        }
        for variable in &result.source_unit_variable_declarations {
            self.add_variable(variable);
        }

        let errors = result
            .errors
            .iter()
            .map(|error| self.add_diagnostic(error))
            .collect();
        self.output.diagnostics = errors;
        let warnings = result
            .warnings
            .iter()
            .map(|warning| self.add_diagnostic(warning))
            .collect();
        self.output.warnings = warnings;

        self.output
    }

    fn build_owned(mut self, mut result: ffi::WireParserResult) -> CompactParseOutput {
        self.add_parser_result_header(&result);

        self.output.root = self.add_ast_node(&result.source_unit);
        let source_unit_nodes = std::mem::take(&mut result.source_unit_nodes);
        let source_unit_children = self.add_ast_nodes(&source_unit_nodes);
        if let Some(root) = self.output.root {
            self.set_children(root, source_unit_children);
        }
        result.error_message.clear();
        result.source_unit.text.bytes.clear();
        result.license.bytes.clear();

        for pragma in std::mem::take(&mut result.source_unit_pragmas) {
            self.add_pragma(&pragma);
        }
        for import in std::mem::take(&mut result.source_unit_imports) {
            self.add_import(&import);
        }
        for value_type in std::mem::take(&mut result.source_unit_user_defined_value_types) {
            self.add_user_defined_value_type(&value_type);
        }
        for enum_definition in std::mem::take(&mut result.source_unit_enums) {
            self.add_enum_definition(&enum_definition);
        }
        for struct_definition in std::mem::take(&mut result.source_unit_structs) {
            self.add_struct_definition(&struct_definition);
        }
        for event in std::mem::take(&mut result.source_unit_events) {
            self.add_event_definition(&event);
        }
        for error in std::mem::take(&mut result.source_unit_errors) {
            self.add_error_definition(&error);
        }
        for contract in std::mem::take(&mut result.source_unit_contracts) {
            self.add_contract(&contract);
        }
        for function in std::mem::take(&mut result.source_unit_functions) {
            self.add_function(&function);
        }
        for quantifier in std::mem::take(&mut result.source_unit_for_all_quantifiers) {
            self.add_for_all_quantifier(&quantifier);
        }
        for type_definition in std::mem::take(&mut result.source_unit_type_definitions) {
            self.add_type_definition(&type_definition);
        }
        for type_class in std::mem::take(&mut result.source_unit_type_class_definitions) {
            self.add_type_class_definition(&type_class);
        }
        for instantiation in std::mem::take(&mut result.source_unit_type_class_instantiations) {
            self.add_type_class_instantiation(&instantiation);
        }
        for using_directive in std::mem::take(&mut result.source_unit_using_directives) {
            self.add_using_directive(&using_directive);
        }
        for variable in std::mem::take(&mut result.source_unit_variable_declarations) {
            self.add_variable(&variable);
        }

        for error in std::mem::take(&mut result.errors) {
            let diagnostic = self.add_diagnostic(&error);
            self.output.diagnostics.push(diagnostic);
        }
        for warning in std::mem::take(&mut result.warnings) {
            let diagnostic = self.add_diagnostic(&warning);
            self.output.warnings.push(diagnostic);
        }

        self.output
    }

    fn add_ast_node(&mut self, node: &ffi::WireAstNode) -> Option<CompactNodeRef> {
        if !node.present {
            return None;
        }
        if let Some(existing) = self.by_id.get(&node.node_id) {
            return Some(*existing);
        }

        let text = self.output.intern_text(&node.text.bytes);
        let node_ref = self.output.push_node(CompactNode {
            id: node.node_id,
            kind: node.kind,
            span: CompactSpan::from(&node.location),
            text,
            payload: CompactPayload::None,
        });
        self.by_id.insert(node.node_id, node_ref);
        Some(node_ref)
    }

    fn add_ast_nodes(&mut self, nodes: &[ffi::WireAstNode]) -> Vec<CompactNodeRef> {
        nodes
            .iter()
            .filter_map(|node| self.add_ast_node(node))
            .collect()
    }

    fn add_optional_ast_node_refs(&mut self, nodes: &[ffi::WireAstNode]) -> CompactRefRange {
        let refs = nodes
            .iter()
            .map(|node| {
                self.add_ast_node(node)
                    .unwrap_or(CompactNodeRef(INVALID_COMPACT_NODE_REF))
            })
            .collect::<Vec<_>>();
        self.output.push_refs(refs)
    }

    fn add_ast_node_refs(&mut self, nodes: &[ffi::WireAstNode]) -> CompactRefRange {
        let refs = self.add_ast_nodes(nodes);
        self.output.push_refs(refs)
    }

    fn add_string_name_locations(
        &mut self,
        names: &[ffi::WireString],
        locations: &[ffi::WireSourceLocation],
    ) -> CompactRefRange {
        self.output.push_name_locations(names, locations)
    }

    fn add_strings_without_locations(&mut self, names: &[ffi::WireString]) -> CompactRefRange {
        self.output.push_names_without_locations(names)
    }

    fn add_optional_child(&mut self, children: &mut Vec<CompactNodeRef>, node: &ffi::WireAstNode) {
        if let Some(node) = self.add_ast_node(node) {
            children.push(node);
        }
    }

    fn extend_children(&mut self, children: &mut Vec<CompactNodeRef>, nodes: &[ffi::WireAstNode]) {
        children.extend(self.add_ast_nodes(nodes));
    }

    fn set_children(&mut self, node: CompactNodeRef, children: Vec<CompactNodeRef>) {
        if children.is_empty() {
            return;
        }
        let range = self.output.push_children(children);
        if let Some(node) = self.output.nodes.get_mut(node.0 as usize) {
            node.payload = CompactPayload::List { children: range };
        }
    }

    fn add_diagnostic(&mut self, error: &ffi::WireParserError) -> CompactDiagnostic {
        let secondary_start = self.output.secondary_locations.len() as u32;
        for secondary in &error.secondary_locations {
            let message = self.output.intern_text(secondary.message.as_bytes());
            self.output
                .secondary_locations
                .push(CompactSecondaryLocation {
                    message,
                    span: CompactSpan::from(&secondary.location),
                });
        }
        CompactDiagnostic {
            error_id: error.error_id,
            message: self.output.intern_text(error.message.as_bytes()),
            span: CompactSpan::from(&error.location),
            secondary_locations: CompactSecondaryRange {
                start: secondary_start,
                len: self.output.secondary_locations.len() as u32 - secondary_start,
            },
            syntax: error.syntax,
            fatal: error.fatal,
        }
    }

    fn add_pragma(&mut self, pragma: &ffi::WirePragmaDirectiveResult) {
        let Some(pragma_directive) = self.add_ast_node(&pragma.pragma_directive) else {
            return;
        };
        let token_literals = self
            .output
            .push_token_literals(&pragma.tokens, &pragma.literals);
        self.output
            .pragma_directive_details
            .push(CompactPragmaDirectiveDetail {
                pragma_directive: Some(pragma_directive),
                token_literals,
            });
    }

    fn add_import(&mut self, import: &ffi::WireImportDirectiveResult) {
        let Some(import_node) = self.add_ast_node(&import.import_directive) else {
            return;
        };
        let mut children = Vec::new();
        let mut aliases = Vec::with_capacity(import.symbol_aliases.len());
        for alias in &import.symbol_aliases {
            let symbol = self.add_ast_node(&alias.symbol);
            if let Some(symbol) = symbol {
                children.push(symbol);
            }
            aliases.push(CompactImportAlias {
                symbol,
                has_alias: alias.has_alias,
                alias: self.output.intern_text(&alias.alias.bytes),
                span: CompactSpan::from(&alias.location),
            });
        }
        let symbol_aliases = self.output.push_import_aliases(aliases);
        let path = self.output.intern_text(&import.path.bytes);
        let unit_alias = self.output.intern_text(&import.unit_alias.bytes);
        self.output
            .import_directive_details
            .push(CompactImportDirectiveDetail {
                import_directive: Some(import_node),
                path,
                unit_alias,
                unit_alias_location: CompactSpan::from(&import.unit_alias_location),
                symbol_aliases,
            });
        self.set_children(import_node, children);
    }

    fn add_contract(&mut self, contract: &ffi::WireContractDefinitionResult) {
        let Some(contract_node) = self.add_ast_node(&contract.contract_definition) else {
            return;
        };
        let documentation = self.add_ast_node(&contract.documentation);
        let base_contracts = self.add_ast_node_refs(&contract.base_contracts);
        let sub_nodes = self.add_ast_node_refs(&contract.sub_nodes);
        let storage_layout_specifier = self.add_ast_node(&contract.storage_layout_specifier);
        let storage_layout_base_slot_expression =
            self.add_ast_node(&contract.storage_layout_base_slot_expression);
        let name = self.output.intern_text(&contract.name.bytes);
        self.output
            .contract_definition_details
            .push(CompactContractDefinitionDetail {
                contract_definition: Some(contract_node),
                name,
                name_location: CompactSpan::from(&contract.name_location),
                documentation,
                base_contracts,
                sub_nodes,
                contract_kind: contract.contract_kind,
                is_abstract: contract.is_abstract,
                storage_layout_specifier,
                storage_layout_base_slot_expression,
            });

        let mut children = Vec::new();
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        if let Some(nodes) = self.output.ref_items.get(
            base_contracts.start as usize
                ..base_contracts.start as usize + base_contracts.len as usize,
        ) {
            children.extend_from_slice(nodes);
        }
        if let Some(nodes) = self
            .output
            .ref_items
            .get(sub_nodes.start as usize..sub_nodes.start as usize + sub_nodes.len as usize)
        {
            children.extend_from_slice(nodes);
        }
        if let Some(storage_layout_specifier) = storage_layout_specifier {
            children.push(storage_layout_specifier);
        }
        if let Some(storage_layout_base_slot_expression) = storage_layout_base_slot_expression {
            children.push(storage_layout_base_slot_expression);
        }

        for base_contract in &contract.base_contract_details {
            self.add_inheritance_specifier(base_contract);
        }
        for struct_definition in &contract.sub_node_structs {
            self.add_struct_definition(struct_definition);
        }
        for enum_definition in &contract.sub_node_enums {
            self.add_enum_definition(enum_definition);
        }
        for value_type in &contract.sub_node_user_defined_value_types {
            self.add_user_defined_value_type(value_type);
        }
        for event in &contract.sub_node_events {
            self.add_event_definition(event);
        }
        for error in &contract.sub_node_errors {
            self.add_error_definition(error);
        }
        for function in &contract.sub_node_functions {
            self.add_function(function);
        }
        for modifier in &contract.sub_node_modifiers {
            self.add_modifier_definition(modifier);
        }
        for using_directive in &contract.sub_node_using_directives {
            self.add_using_directive(using_directive);
        }
        for variable in &contract.sub_node_variable_declarations {
            self.add_variable(variable);
        }
        self.add_expression(&contract.storage_layout_base_slot_expression_detail);

        self.set_children(contract_node, children);
    }

    fn add_function(&mut self, function: &ffi::WireFunctionDefinitionResult) {
        let Some(function_node) = self.add_ast_node(&function.function_definition) else {
            return;
        };
        let documentation = self.add_ast_node(&function.documentation);
        let overrides = self.add_ast_node(&function.overrides);
        let override_paths = self.add_ast_node_refs(&function.override_paths);
        let parameters = self.add_ast_node(&function.parameters);
        let parameter_declarations = self.add_ast_node_refs(&function.parameter_declarations);
        let modifiers = self.add_ast_node_refs(&function.modifiers);
        let return_parameters = self.add_ast_node(&function.return_parameters);
        let return_parameter_declarations =
            self.add_ast_node_refs(&function.return_parameter_declarations);
        let block = self.add_ast_node(&function.block);
        let block_statements = self.add_ast_node_refs(&function.block_statements);
        let experimental_return_expression =
            self.add_ast_node(&function.experimental_return_expression);
        let name = self.output.intern_text(&function.name.bytes);
        self.output
            .function_definition_details
            .push(CompactFunctionDefinitionDetail {
                function_definition: Some(function_node),
                name,
                name_location: CompactSpan::from(&function.name_location),
                visibility: function.visibility,
                state_mutability: function.state_mutability,
                is_free_function: function.is_free_function,
                kind: function.kind,
                is_virtual: function.is_virtual,
                documentation,
                overrides,
                override_paths,
                parameters,
                parameter_declarations,
                modifiers,
                return_parameters,
                return_parameter_declarations,
                block,
                block_unchecked: function.block_unchecked,
                block_statements,
                experimental_return_expression,
            });

        let mut children = Vec::new();
        self.add_optional_child(&mut children, &function.documentation);
        self.add_optional_child(&mut children, &function.overrides);
        self.extend_children(&mut children, &function.override_paths);
        self.add_parameter_list(
            &function.parameters,
            &function.parameter_declarations,
            &function.parameter_details,
        );
        self.add_optional_child(&mut children, &function.parameters);
        self.extend_children(&mut children, &function.parameter_declarations);
        self.extend_children(&mut children, &function.modifiers);
        self.add_parameter_list(
            &function.return_parameters,
            &function.return_parameter_declarations,
            &function.return_parameter_details,
        );
        self.add_optional_child(&mut children, &function.return_parameters);
        self.extend_children(&mut children, &function.return_parameter_declarations);
        self.add_block(
            &function.block,
            function.block_unchecked,
            &function.block_statements,
            &function.block_statement_details,
        );
        self.add_optional_child(&mut children, &function.block);
        self.extend_children(&mut children, &function.block_statements);
        self.add_optional_child(&mut children, &function.experimental_return_expression);

        for override_path in &function.override_path_details {
            self.add_identifier_path(override_path);
        }
        for variable in &function.parameter_details {
            self.add_variable(variable);
        }
        for modifier in &function.modifier_details {
            self.add_modifier_invocation(modifier);
        }
        for variable in &function.return_parameter_details {
            self.add_variable(variable);
        }
        self.add_expression(&function.experimental_return_expression_detail);

        self.set_children(function_node, children);
    }

    fn add_for_all_quantifier(&mut self, quantifier: &ffi::WireForAllQuantifierResult) {
        let Some(node) = self.add_ast_node(&quantifier.for_all_quantifier) else {
            return;
        };
        let type_variable_declarations = self.add_ast_node(&quantifier.type_variable_declarations);
        let type_variable_declaration_parameters =
            self.add_ast_node_refs(&quantifier.type_variable_declaration_parameters);
        let quantified_function = self.add_ast_node(&quantifier.quantified_function);
        self.output
            .for_all_quantifier_details
            .push(CompactForAllQuantifierDetail {
                for_all_quantifier: Some(node),
                type_variable_declarations,
                type_variable_declaration_parameters,
                quantified_function,
            });

        let mut children = Vec::new();
        self.add_parameter_list(
            &quantifier.type_variable_declarations,
            &quantifier.type_variable_declaration_parameters,
            &quantifier.type_variable_declaration_details,
        );
        if let Some(type_variable_declarations) = type_variable_declarations {
            children.push(type_variable_declarations);
        }
        if let Some(nodes) = self.output.ref_items.get(
            type_variable_declaration_parameters.start as usize
                ..type_variable_declaration_parameters.start as usize
                    + type_variable_declaration_parameters.len as usize,
        ) {
            children.extend_from_slice(nodes);
        }
        if let Some(quantified_function) = quantified_function {
            children.push(quantified_function);
        }
        for variable in &quantifier.type_variable_declaration_details {
            self.add_variable(variable);
        }
        self.add_function(&quantifier.quantified_function_detail);
        self.set_children(node, children);
    }

    fn add_parameter_list(
        &mut self,
        parameter_list: &ffi::WireAstNode,
        declarations: &[ffi::WireAstNode],
        details: &[ffi::WireVariableDeclarationResult],
    ) {
        let Some(node) = self.add_ast_node(parameter_list) else {
            return;
        };
        let children = self.add_ast_nodes(declarations);
        for detail in details {
            self.add_variable(detail);
        }
        self.set_children(node, children);
    }

    fn add_parameter_list_parts(
        &mut self,
        parameter_list: &ffi::WireAstNode,
        declarations: &[ffi::WireAstNode],
        details: &[ParsedVariableDeclaration],
    ) {
        let Some(node) = self.add_ast_node(parameter_list) else {
            return;
        };
        let children = self.add_ast_nodes(declarations);
        for detail in details {
            self.add_variable_parts(detail.as_compact_parts());
        }
        self.set_children(node, children);
    }

    fn add_block(
        &mut self,
        block: &ffi::WireAstNode,
        unchecked: bool,
        statements: &[ffi::WireAstNode],
        details: &[ffi::WireStatementResult],
    ) {
        let Some(block_node) = self.add_ast_node(block) else {
            return;
        };
        let block_statements = self.add_ast_node_refs(statements);
        let children_start = block_statements.start as usize;
        let children_end = children_start + block_statements.len as usize;
        let children = self.output.ref_items[children_start..children_end].to_vec();
        self.output.statement_details.push(CompactStatementDetail {
            statement: Some(block_node),
            block_unchecked: unchecked,
            block_statements,
            ..Default::default()
        });
        for statement in details {
            self.add_statement(statement);
        }
        self.set_children(block_node, children);
    }

    fn add_rust_block(
        &mut self,
        block: &ffi::WireAstNode,
        unchecked: bool,
        statements: &[ffi::WireAstNode],
        details: &[RustStatementResult],
    ) {
        let Some(block_node) = self.add_ast_node(block) else {
            return;
        };
        let block_statements = self.add_ast_node_refs(statements);
        let children_start = block_statements.start as usize;
        let children_end = children_start + block_statements.len as usize;
        let children = self.output.ref_items[children_start..children_end].to_vec();
        self.output.statement_details.push(CompactStatementDetail {
            statement: Some(block_node),
            block_unchecked: unchecked,
            block_statements,
            ..Default::default()
        });
        for statement in details {
            self.add_rust_statement(statement);
        }
        self.set_children(block_node, children);
    }

    fn add_statement(&mut self, statement: &ffi::WireStatementResult) {
        let Some(statement_node) = self.add_ast_node(&statement.statement) else {
            return;
        };
        let block_statements = self.add_ast_node_refs(&statement.block_statements);
        let inline_assembly_flags =
            self.add_strings_without_locations(&statement.inline_assembly_flags);
        let condition_expression = self.add_ast_node(&statement.condition_expression);
        let true_body = self.add_ast_node(&statement.true_body);
        let false_body = self.add_ast_node(&statement.false_body);
        let body = self.add_ast_node(&statement.body);
        let external_call = self.add_ast_node(&statement.external_call);
        let clauses = self.add_ast_node_refs(&statement.clauses);
        let clause_error_parameters = self.add_ast_node_refs(&statement.clause_error_parameters);
        let clause_blocks = self.add_ast_node_refs(&statement.clause_blocks);
        let init_expression = self.add_ast_node(&statement.init_expression);
        let loop_expression = self.add_ast_node(&statement.loop_expression);
        let event_call = self.add_ast_node(&statement.event_call);
        let event_call_callee = self.add_ast_node(&statement.event_call_callee);
        let event_call_arguments = self.add_ast_node_refs(&statement.event_call_arguments);
        let event_call_parameter_names = self.add_string_name_locations(
            &statement.event_call_parameter_names,
            &statement.event_call_parameter_name_locations,
        );
        let error_call = self.add_ast_node(&statement.error_call);
        let error_call_callee = self.add_ast_node(&statement.error_call_callee);
        let error_call_arguments = self.add_ast_node_refs(&statement.error_call_arguments);
        let error_call_parameter_names = self.add_string_name_locations(
            &statement.error_call_parameter_names,
            &statement.error_call_parameter_name_locations,
        );
        let expression = self.add_ast_node(&statement.expression);
        let variables = self.add_optional_ast_node_refs(&statement.variables);
        let initial_value = self.add_ast_node(&statement.initial_value);
        self.output.statement_details.push(CompactStatementDetail {
            statement: Some(statement_node),
            block_unchecked: statement.block_unchecked,
            block_statements,
            inline_assembly_flags,
            inline_assembly_block_location: CompactSpan::from(
                &statement.inline_assembly_block_location,
            ),
            condition_expression,
            true_body,
            false_body,
            body,
            is_do_while: statement.is_do_while,
            external_call,
            clauses,
            clause_error_parameters,
            clause_blocks,
            init_expression,
            loop_expression,
            event_call,
            event_call_callee,
            event_call_arguments,
            event_call_parameter_names,
            error_call,
            error_call_callee,
            error_call_arguments,
            error_call_parameter_names,
            expression,
            variables,
            initial_value,
        });
        let mut children = Vec::new();
        self.extend_children(&mut children, &statement.block_statements);
        self.add_optional_child(&mut children, &statement.condition_expression);
        self.add_optional_child(&mut children, &statement.true_body);
        self.add_optional_child(&mut children, &statement.false_body);
        self.add_optional_child(&mut children, &statement.body);
        self.add_optional_child(&mut children, &statement.external_call);
        self.extend_children(&mut children, &statement.clauses);
        self.extend_children(&mut children, &statement.clause_error_parameters);
        self.extend_children(&mut children, &statement.clause_blocks);
        self.add_optional_child(&mut children, &statement.init_expression);
        self.add_optional_child(&mut children, &statement.loop_expression);
        self.add_optional_child(&mut children, &statement.event_call);
        self.add_optional_child(&mut children, &statement.event_call_callee);
        self.extend_children(&mut children, &statement.event_call_arguments);
        self.add_optional_child(&mut children, &statement.error_call);
        self.add_optional_child(&mut children, &statement.error_call_callee);
        self.extend_children(&mut children, &statement.error_call_arguments);
        self.add_optional_child(&mut children, &statement.expression);
        self.extend_children(&mut children, &statement.variables);
        self.add_optional_child(&mut children, &statement.initial_value);

        for detail in &statement.block_statement_details {
            self.add_statement(detail);
        }
        for detail in &statement.condition_expression_detail {
            self.add_expression(detail);
        }
        for detail in &statement.true_body_detail {
            self.add_statement(detail);
        }
        for detail in &statement.false_body_detail {
            self.add_statement(detail);
        }
        for detail in &statement.body_detail {
            self.add_statement(detail);
        }
        for detail in &statement.external_call_detail {
            self.add_expression(detail);
        }
        for clause in &statement.clause_details {
            self.add_try_catch_clause(clause);
        }
        for detail in &statement.clause_block_statement_details {
            self.add_statement(detail);
        }
        for detail in &statement.init_expression_detail {
            self.add_statement(detail);
        }
        for detail in &statement.loop_expression_detail {
            self.add_statement(detail);
        }
        for detail in &statement.event_call_callee_detail {
            self.add_expression(detail);
        }
        for detail in &statement.event_call_argument_details {
            self.add_expression(detail);
        }
        for detail in &statement.error_call_callee_detail {
            self.add_expression(detail);
        }
        for detail in &statement.error_call_argument_details {
            self.add_expression(detail);
        }
        for detail in &statement.expression_detail {
            self.add_expression(detail);
        }
        for variable in &statement.variable_details {
            self.add_variable(variable);
        }
        for detail in &statement.initial_value_detail {
            self.add_expression(detail);
        }

        self.set_children(statement_node, children);
    }

    fn add_rust_statement(&mut self, statement: &RustStatementResult) {
        let Some(statement_node) = self.add_ast_node(&statement.statement) else {
            return;
        };

        let mut children = Vec::new();
        let mut detail = CompactStatementDetail {
            statement: Some(statement_node),
            ..Default::default()
        };

        match &statement.data {
            RustStatementData::Empty => {}
            RustStatementData::Block {
                unchecked,
                statements,
                statement_details,
            } => {
                detail.block_unchecked = *unchecked;
                detail.block_statements = self.add_ast_node_refs(statements);
                self.extend_children(&mut children, statements);
                self.output.statement_details.push(detail);
                for statement in statement_details {
                    self.add_rust_statement(statement);
                }
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::InlineAssembly {
                flags,
                block_location,
            } => {
                detail.inline_assembly_flags = self.add_strings_without_locations(flags);
                detail.inline_assembly_block_location = CompactSpan::from(block_location);
            }
            RustStatementData::If {
                condition_expression,
                condition_expression_detail,
                true_body,
                true_body_detail,
                false_body,
                false_body_detail,
            } => {
                detail.condition_expression = self.add_ast_node(condition_expression);
                detail.true_body = self.add_ast_node(true_body);
                detail.false_body = self.add_ast_node(false_body);
                self.add_optional_child(&mut children, condition_expression);
                self.add_optional_child(&mut children, true_body);
                self.add_optional_child(&mut children, false_body);
                self.output.statement_details.push(detail);
                self.add_rust_expression(condition_expression_detail);
                self.add_rust_statement(true_body_detail);
                if let Some(detail) = false_body_detail.as_deref() {
                    self.add_rust_statement(detail);
                }
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::Loop {
                condition_expression,
                condition_expression_detail,
                body,
                body_detail,
                is_do_while,
            } => {
                detail.condition_expression = self.add_ast_node(condition_expression);
                detail.body = self.add_ast_node(body);
                detail.is_do_while = *is_do_while;
                self.add_optional_child(&mut children, condition_expression);
                self.add_optional_child(&mut children, body);
                self.output.statement_details.push(detail);
                self.add_rust_expression(condition_expression_detail);
                self.add_rust_statement(body_detail);
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::Try {
                external_call,
                external_call_detail,
                clauses,
                clause_details,
                clause_block_statement_details,
                clause_error_names: _,
                clause_error_parameters,
                clause_blocks,
            } => {
                detail.external_call = self.add_ast_node(external_call);
                detail.clauses = self.add_ast_node_refs(clauses);
                detail.clause_error_parameters = self.add_ast_node_refs(clause_error_parameters);
                detail.clause_blocks = self.add_ast_node_refs(clause_blocks);
                self.add_optional_child(&mut children, external_call);
                self.extend_children(&mut children, clauses);
                self.extend_children(&mut children, clause_error_parameters);
                self.extend_children(&mut children, clause_blocks);
                self.output.statement_details.push(detail);
                self.add_rust_expression(external_call_detail);
                for clause in clause_details {
                    self.add_rust_try_catch_clause(clause);
                }
                for detail in clause_block_statement_details {
                    self.add_rust_statement(detail);
                }
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::For {
                init_expression,
                init_expression_detail,
                condition_expression,
                condition_expression_detail,
                loop_expression,
                loop_expression_detail,
                body,
                body_detail,
            } => {
                detail.init_expression = self.add_ast_node(init_expression);
                detail.condition_expression = self.add_ast_node(condition_expression);
                detail.loop_expression = self.add_ast_node(loop_expression);
                detail.body = self.add_ast_node(body);
                self.add_optional_child(&mut children, init_expression);
                self.add_optional_child(&mut children, condition_expression);
                self.add_optional_child(&mut children, loop_expression);
                self.add_optional_child(&mut children, body);
                self.output.statement_details.push(detail);
                if let Some(detail) = init_expression_detail.as_deref() {
                    self.add_rust_statement(detail);
                }
                if let Some(detail) = condition_expression_detail.as_ref() {
                    self.add_rust_expression(detail);
                }
                if let Some(detail) = loop_expression_detail.as_deref() {
                    self.add_rust_statement(detail);
                }
                self.add_rust_statement(body_detail);
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::Emit {
                event_call,
                event_call_callee,
                event_call_callee_detail,
                event_call_arguments,
                event_call_argument_details,
                event_call_parameter_names,
                event_call_parameter_name_locations,
            } => {
                detail.event_call = self.add_ast_node(event_call);
                detail.event_call_callee = self.add_ast_node(event_call_callee);
                detail.event_call_arguments = self.add_ast_node_refs(event_call_arguments);
                detail.event_call_parameter_names = self.add_string_name_locations(
                    event_call_parameter_names,
                    event_call_parameter_name_locations,
                );
                self.add_optional_child(&mut children, event_call);
                self.add_optional_child(&mut children, event_call_callee);
                self.extend_children(&mut children, event_call_arguments);
                self.output.statement_details.push(detail);
                self.add_rust_expression(event_call_callee_detail);
                for detail in event_call_argument_details {
                    self.add_rust_expression(detail);
                }
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::Revert {
                error_call,
                error_call_callee,
                error_call_callee_detail,
                error_call_arguments,
                error_call_argument_details,
                error_call_parameter_names,
                error_call_parameter_name_locations,
            } => {
                detail.error_call = self.add_ast_node(error_call);
                detail.error_call_callee = self.add_ast_node(error_call_callee);
                detail.error_call_arguments = self.add_ast_node_refs(error_call_arguments);
                detail.error_call_parameter_names = self.add_string_name_locations(
                    error_call_parameter_names,
                    error_call_parameter_name_locations,
                );
                self.add_optional_child(&mut children, error_call);
                self.add_optional_child(&mut children, error_call_callee);
                self.extend_children(&mut children, error_call_arguments);
                self.output.statement_details.push(detail);
                self.add_rust_expression(error_call_callee_detail);
                for detail in error_call_argument_details {
                    self.add_rust_expression(detail);
                }
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::Expression {
                expression,
                expression_detail,
            } => {
                detail.expression = self.add_ast_node(expression);
                self.add_optional_child(&mut children, expression);
                self.output.statement_details.push(detail);
                if let Some(detail) = expression_detail.as_ref() {
                    self.add_rust_expression(detail);
                }
                self.set_children(statement_node, children);
                return;
            }
            RustStatementData::VariableDeclaration {
                variables,
                variable_details,
                initial_value,
                initial_value_detail,
            } => {
                detail.variables = self.add_optional_ast_node_refs(variables);
                detail.initial_value = self.add_ast_node(initial_value);
                self.extend_children(&mut children, variables);
                self.add_optional_child(&mut children, initial_value);
                self.output.statement_details.push(detail);
                for variable in variable_details {
                    self.add_variable_parts(variable.as_compact_parts());
                }
                if let Some(detail) = initial_value_detail.as_ref() {
                    self.add_rust_expression(detail);
                }
                self.set_children(statement_node, children);
                return;
            }
        }

        self.output.statement_details.push(detail);
        self.set_children(statement_node, children);
    }

    fn add_try_catch_clause(&mut self, clause: &ffi::WireTryCatchClauseResult) {
        let Some(node) = self.add_ast_node(&clause.try_catch_clause) else {
            return;
        };
        let error_parameters = self.add_ast_node(&clause.error_parameters);
        let error_parameter_declarations =
            self.add_ast_node_refs(&clause.error_parameter_declarations);
        let block = self.add_ast_node(&clause.block);
        let block_statements = self.add_ast_node_refs(&clause.block_statements);
        let error_name = self.output.intern_text(&clause.error_name.bytes);
        self.output
            .try_catch_clause_details
            .push(CompactTryCatchClauseDetail {
                try_catch_clause: Some(node),
                error_name,
                error_parameters,
                error_parameter_declarations,
                block,
                block_unchecked: clause.block_unchecked,
                block_statements,
            });
        let mut children = Vec::new();
        self.add_parameter_list(
            &clause.error_parameters,
            &clause.error_parameter_declarations,
            &clause.error_parameter_details,
        );
        self.add_optional_child(&mut children, &clause.error_parameters);
        self.extend_children(&mut children, &clause.error_parameter_declarations);
        self.add_block(
            &clause.block,
            clause.block_unchecked,
            &clause.block_statements,
            &clause.block_statement_details,
        );
        self.add_optional_child(&mut children, &clause.block);
        self.extend_children(&mut children, &clause.block_statements);
        for variable in &clause.error_parameter_details {
            self.add_variable(variable);
        }
        self.set_children(node, children);
    }

    fn add_rust_try_catch_clause(&mut self, clause: &RustTryCatchClauseResult) {
        let Some(node) = self.add_ast_node(&clause.try_catch_clause) else {
            return;
        };
        let error_parameters = self.add_ast_node(&clause.error_parameters);
        let error_parameter_declarations =
            self.add_ast_node_refs(&clause.error_parameter_declarations);
        let block = self.add_ast_node(&clause.block);
        let block_statements = self.add_ast_node_refs(&clause.block_statements);
        let error_name = self.output.intern_text(&clause.error_name.bytes);
        self.output
            .try_catch_clause_details
            .push(CompactTryCatchClauseDetail {
                try_catch_clause: Some(node),
                error_name,
                error_parameters,
                error_parameter_declarations,
                block,
                block_unchecked: clause.block_unchecked,
                block_statements,
            });
        let mut children = Vec::new();
        self.add_parameter_list_parts(
            &clause.error_parameters,
            &clause.error_parameter_declarations,
            &clause.error_parameter_details,
        );
        self.add_optional_child(&mut children, &clause.error_parameters);
        self.extend_children(&mut children, &clause.error_parameter_declarations);
        self.add_rust_block(
            &clause.block,
            clause.block_unchecked,
            &clause.block_statements,
            &clause.block_statement_details,
        );
        self.add_optional_child(&mut children, &clause.block);
        self.extend_children(&mut children, &clause.block_statements);
        self.set_children(node, children);
    }

    fn add_expression(&mut self, expression: &ffi::WireExpressionResult) {
        let Some(expression_node) = self.add_ast_node(&expression.expression) else {
            return;
        };
        let left_expression = self.add_ast_node(&expression.left_expression);
        let right_expression = self.add_ast_node(&expression.right_expression);
        let condition_expression = self.add_ast_node(&expression.condition_expression);
        let true_expression = self.add_ast_node(&expression.true_expression);
        let false_expression = self.add_ast_node(&expression.false_expression);
        let sub_expression = self.add_ast_node(&expression.sub_expression);
        let base_expression = self.add_ast_node(&expression.base_expression);
        let base_expression_type = self.add_ast_node(&expression.base_expression_type);
        let index_expression = self.add_ast_node(&expression.index_expression);
        let end_index_expression = self.add_ast_node(&expression.end_index_expression);
        let type_name = self.add_ast_node(&expression.type_name);
        let expression_type = self.add_ast_node(&expression.expression_type);
        let arguments = self.add_ast_node_refs(&expression.arguments);
        let argument_names = self.add_string_name_locations(
            &expression.parameter_names,
            &expression.parameter_name_locations,
        );
        let components = self.add_optional_ast_node_refs(&expression.components);
        self.output
            .expression_details
            .push(CompactExpressionDetail {
                expression: Some(expression_node),
                left_expression,
                right_expression,
                condition_expression,
                true_expression,
                false_expression,
                sub_expression,
                base_expression,
                base_expression_type,
                index_expression,
                end_index_expression,
                type_name,
                expression_type,
                arguments,
                argument_names,
                components,
                member_name_location: CompactSpan::from(&expression.member_name_location),
                is_prefix_operation: expression.is_prefix_operation,
                is_inline_array: expression.is_inline_array,
                literal_token: expression.literal_token,
                literal_subdenomination: expression.literal_subdenomination,
            });
        let mut children = Vec::new();
        self.add_optional_child(&mut children, &expression.left_expression);
        self.add_optional_child(&mut children, &expression.right_expression);
        self.add_optional_child(&mut children, &expression.condition_expression);
        self.add_optional_child(&mut children, &expression.true_expression);
        self.add_optional_child(&mut children, &expression.false_expression);
        self.add_optional_child(&mut children, &expression.sub_expression);
        self.add_optional_child(&mut children, &expression.base_expression);
        self.add_optional_child(&mut children, &expression.base_expression_type);
        self.add_optional_child(&mut children, &expression.index_expression);
        self.add_optional_child(&mut children, &expression.end_index_expression);
        self.add_optional_child(&mut children, &expression.type_name);
        self.add_optional_child(&mut children, &expression.expression_type);
        self.extend_children(&mut children, &expression.arguments);
        self.extend_children(&mut children, &expression.components);

        for detail in &expression.left_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.right_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.condition_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.true_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.false_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.sub_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.base_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.index_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.end_index_expression_detail {
            self.add_expression(detail);
        }
        for detail in &expression.type_name_details {
            self.add_type_name(detail);
        }
        for detail in &expression.argument_details {
            self.add_expression(detail);
        }
        for detail in &expression.component_details {
            self.add_expression(detail);
        }

        self.set_children(expression_node, children);
    }

    fn add_rust_expression(&mut self, expression: &RustExpressionResult) {
        if let RustExpressionData::RawWire(wire) = &expression.data {
            self.add_expression(wire);
            return;
        }

        let Some(expression_node) = self.add_ast_node(&expression.expression) else {
            return;
        };

        match &expression.data {
            RustExpressionData::Empty | RustExpressionData::RawWire(_) => {
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        ..Default::default()
                    });
                self.set_children(expression_node, Vec::new());
            }
            RustExpressionData::Binary {
                left_expression,
                right_expression,
                expression_details,
            } => {
                let left_expression_ref = self.add_ast_node(left_expression);
                let right_expression_ref = self.add_ast_node(right_expression);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, left_expression);
                self.add_optional_child(&mut children, right_expression);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        left_expression: left_expression_ref,
                        right_expression: right_expression_ref,
                        ..Default::default()
                    });
                self.add_rust_expression(&expression_details[0]);
                self.add_rust_expression(&expression_details[1]);
                self.set_children(expression_node, children);
            }
            RustExpressionData::Conditional {
                condition_expression,
                true_expression,
                false_expression,
                expression_details,
            } => {
                let condition_expression_ref = self.add_ast_node(condition_expression);
                let true_expression_ref = self.add_ast_node(true_expression);
                let false_expression_ref = self.add_ast_node(false_expression);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, condition_expression);
                self.add_optional_child(&mut children, true_expression);
                self.add_optional_child(&mut children, false_expression);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        condition_expression: condition_expression_ref,
                        true_expression: true_expression_ref,
                        false_expression: false_expression_ref,
                        ..Default::default()
                    });
                self.add_rust_expression(&expression_details[0]);
                self.add_rust_expression(&expression_details[1]);
                self.add_rust_expression(&expression_details[2]);
                self.set_children(expression_node, children);
            }
            RustExpressionData::Unary {
                sub_expression,
                sub_expression_detail,
                is_prefix_operation,
            } => {
                let sub_expression_ref = self.add_ast_node(sub_expression);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, sub_expression);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        sub_expression: sub_expression_ref,
                        is_prefix_operation: *is_prefix_operation,
                        ..Default::default()
                    });
                self.add_rust_expression(sub_expression_detail);
                self.set_children(expression_node, children);
            }
            RustExpressionData::New {
                type_name,
                type_name_detail,
            } => {
                let type_name_ref = self.add_ast_node(type_name);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, type_name);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        type_name: type_name_ref,
                        ..Default::default()
                    });
                self.add_type_name_parts(type_name_detail.as_compact_parts());
                self.set_children(expression_node, children);
            }
            RustExpressionData::ElementaryTypeNameExpression { expression_type } => {
                let expression_type_ref = self.add_ast_node(expression_type);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, expression_type);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        expression_type: expression_type_ref,
                        ..Default::default()
                    });
                self.set_children(expression_node, children);
            }
            RustExpressionData::IndexAccess {
                base_expression,
                base_expression_detail,
                base_expression_type,
                index_expression,
                index_expression_detail,
            } => {
                let base_expression_ref = self.add_ast_node(base_expression);
                let base_expression_type_ref = self.add_ast_node(base_expression_type);
                let index_expression_ref = self.add_ast_node(index_expression);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, base_expression);
                self.add_optional_child(&mut children, base_expression_type);
                self.add_optional_child(&mut children, index_expression);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        base_expression: base_expression_ref,
                        base_expression_type: base_expression_type_ref,
                        index_expression: index_expression_ref,
                        ..Default::default()
                    });
                self.add_rust_expression(base_expression_detail);
                if let Some(detail) = index_expression_detail.as_deref() {
                    self.add_rust_expression(detail);
                }
                self.set_children(expression_node, children);
            }
            RustExpressionData::IndexRangeAccess {
                base_expression,
                base_expression_detail,
                base_expression_type,
                index_expression,
                index_expression_detail,
                end_index_expression,
                end_index_expression_detail,
            } => {
                let base_expression_ref = self.add_ast_node(base_expression);
                let base_expression_type_ref = self.add_ast_node(base_expression_type);
                let index_expression_ref = self.add_ast_node(index_expression);
                let end_index_expression_ref = self.add_ast_node(end_index_expression);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, base_expression);
                self.add_optional_child(&mut children, base_expression_type);
                self.add_optional_child(&mut children, index_expression);
                self.add_optional_child(&mut children, end_index_expression);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        base_expression: base_expression_ref,
                        base_expression_type: base_expression_type_ref,
                        index_expression: index_expression_ref,
                        end_index_expression: end_index_expression_ref,
                        ..Default::default()
                    });
                self.add_rust_expression(base_expression_detail);
                if let Some(detail) = index_expression_detail.as_deref() {
                    self.add_rust_expression(detail);
                }
                if let Some(detail) = end_index_expression_detail.as_deref() {
                    self.add_rust_expression(detail);
                }
                self.set_children(expression_node, children);
            }
            RustExpressionData::MemberAccess {
                base_expression,
                base_expression_detail,
                base_expression_type,
                member_name_location,
            } => {
                let base_expression_ref = self.add_ast_node(base_expression);
                let base_expression_type_ref = self.add_ast_node(base_expression_type);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, base_expression);
                self.add_optional_child(&mut children, base_expression_type);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        base_expression: base_expression_ref,
                        base_expression_type: base_expression_type_ref,
                        member_name_location: CompactSpan::from(member_name_location),
                        ..Default::default()
                    });
                self.add_rust_expression(base_expression_detail);
                self.set_children(expression_node, children);
            }
            RustExpressionData::FunctionCall {
                base_expression,
                base_expression_detail,
                base_expression_type,
                arguments,
                argument_details,
                parameter_names,
                parameter_name_locations,
            } => {
                let base_expression_ref = self.add_ast_node(base_expression);
                let base_expression_type_ref = self.add_ast_node(base_expression_type);
                let arguments_ref = self.add_ast_node_refs(arguments);
                let argument_names =
                    self.add_string_name_locations(parameter_names, parameter_name_locations);
                let mut children = Vec::new();
                self.add_optional_child(&mut children, base_expression);
                self.add_optional_child(&mut children, base_expression_type);
                self.extend_children(&mut children, arguments);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        base_expression: base_expression_ref,
                        base_expression_type: base_expression_type_ref,
                        arguments: arguments_ref,
                        argument_names,
                        ..Default::default()
                    });
                self.add_rust_expression(base_expression_detail);
                for detail in argument_details {
                    self.add_rust_expression(detail);
                }
                self.set_children(expression_node, children);
            }
            RustExpressionData::Tuple {
                components,
                component_details,
                is_inline_array,
            } => {
                let components_ref = self.add_optional_ast_node_refs(components);
                let mut children = Vec::new();
                self.extend_children(&mut children, components);
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        components: components_ref,
                        is_inline_array: *is_inline_array,
                        ..Default::default()
                    });
                for detail in component_details {
                    self.add_rust_expression(detail);
                }
                self.set_children(expression_node, children);
            }
            RustExpressionData::Literal {
                literal_token,
                literal_subdenomination,
            } => {
                self.output
                    .expression_details
                    .push(CompactExpressionDetail {
                        expression: Some(expression_node),
                        literal_token: *literal_token,
                        literal_subdenomination: *literal_subdenomination,
                        ..Default::default()
                    });
                self.set_children(expression_node, Vec::new());
            }
        }
    }

    fn add_variable(&mut self, variable: &ffi::WireVariableDeclarationResult) {
        self.add_variable_parts(CompactVariableDeclarationParts::from(variable));
    }

    fn add_expression_detail_ref(&mut self, expression: CompactExpressionDetailRef<'_>) {
        match expression {
            CompactExpressionDetailRef::Wire(expression) => self.add_expression(expression),
            CompactExpressionDetailRef::Rust(expression) => self.add_rust_expression(expression),
        }
    }

    fn add_expression_details_ref(&mut self, expressions: CompactExpressionDetailsRef<'_>) {
        match expressions {
            CompactExpressionDetailsRef::Wire(expressions) => {
                for expression in expressions {
                    self.add_expression(expression);
                }
            }
            CompactExpressionDetailsRef::Rust(expressions) => {
                for expression in expressions {
                    self.add_rust_expression(expression);
                }
            }
        }
    }

    fn add_variable_details_ref(&mut self, variables: CompactVariableDetailsRef<'_>) {
        match variables {
            CompactVariableDetailsRef::Wire(variables) => {
                for variable in variables {
                    self.add_variable(variable);
                }
            }
            CompactVariableDetailsRef::Parsed(variables) => {
                for variable in variables {
                    self.add_variable_parts(variable.as_compact_parts());
                }
            }
        }
    }

    fn add_parameter_list_ref(
        &mut self,
        parameter_list: &ffi::WireAstNode,
        declarations: &[ffi::WireAstNode],
        details: CompactVariableDetailsRef<'_>,
    ) {
        match details {
            CompactVariableDetailsRef::Wire(details) => {
                self.add_parameter_list(parameter_list, declarations, details);
            }
            CompactVariableDetailsRef::Parsed(details) => {
                self.add_parameter_list_parts(parameter_list, declarations, details);
            }
        }
    }

    fn add_variable_parts(&mut self, variable: CompactVariableDeclarationParts<'_>) {
        let Some(variable_node) = self.add_ast_node(variable.variable_declaration) else {
            return;
        };
        let mut children = Vec::new();
        self.add_optional_child(&mut children, variable.type_name);
        self.add_optional_child(&mut children, variable.type_expression);
        self.add_optional_child(&mut children, variable.documentation);
        self.add_optional_child(&mut children, variable.overrides);
        self.extend_children(&mut children, variable.override_paths);
        self.add_optional_child(&mut children, variable.value);
        self.add_optional_child(&mut children, variable.type_name_user_defined_path_node);
        self.extend_children(&mut children, variable.type_name_array_base_types);
        self.extend_children(&mut children, variable.type_name_array_lengths);
        self.add_parameter_list_ref(
            variable.type_name_function_parameters,
            variable.type_name_function_parameter_declarations,
            variable.type_name_function_parameter_details,
        );
        self.add_optional_child(&mut children, variable.type_name_function_parameters);
        self.extend_children(
            &mut children,
            variable.type_name_function_parameter_declarations,
        );
        self.add_parameter_list_ref(
            variable.type_name_function_return_parameters,
            variable.type_name_function_return_parameter_declarations,
            variable.type_name_function_return_parameter_details,
        );
        self.add_optional_child(&mut children, variable.type_name_function_return_parameters);
        self.extend_children(
            &mut children,
            variable.type_name_function_return_parameter_declarations,
        );
        self.add_optional_child(&mut children, variable.type_name_mapping_key_type);
        self.add_optional_child(
            &mut children,
            variable.type_name_mapping_key_user_defined_path_node,
        );
        self.add_optional_child(&mut children, variable.type_name_mapping_value_type);
        self.add_optional_child(
            &mut children,
            variable.type_name_mapping_value_user_defined_path_node,
        );
        self.extend_children(
            &mut children,
            variable.type_name_mapping_value_array_base_types,
        );
        self.extend_children(
            &mut children,
            variable.type_name_mapping_value_array_lengths,
        );

        self.add_variable_type_name_detail_parts(&variable);
        let type_name = self.add_ast_node(variable.type_name);
        let type_expression = self.add_ast_node(variable.type_expression);
        let documentation = self.add_ast_node(variable.documentation);
        let overrides = self.add_ast_node(variable.overrides);
        let override_paths = self.add_ast_node_refs(variable.override_paths);
        let value = self.add_ast_node(variable.value);
        let name = self.output.intern_text(&variable.name.bytes);
        self.output
            .variable_declaration_details
            .push(CompactVariableDeclarationDetail {
                variable_declaration: Some(variable_node),
                type_name,
                type_expression,
                documentation,
                overrides,
                override_paths,
                value,
                name,
                name_location: CompactSpan::from(variable.name_location),
                visibility: variable.visibility,
                mutability: variable.mutability,
                variable_location: variable.variable_location,
                indexed: variable.indexed,
            });

        self.add_expression_detail_ref(variable.type_expression_detail);
        for detail in variable.override_path_details {
            self.add_identifier_path(detail);
        }
        self.add_expression_detail_ref(variable.value_detail);
        self.add_expression_details_ref(variable.type_name_array_length_details);
        self.add_variable_details_ref(variable.type_name_function_parameter_details);
        self.add_variable_details_ref(variable.type_name_function_return_parameter_details);
        self.add_expression_details_ref(variable.type_name_mapping_value_array_length_details);
        self.add_variable_details_ref(variable.type_name_mapping_value_function_parameter_details);
        self.add_variable_details_ref(
            variable.type_name_mapping_value_function_return_parameter_details,
        );

        self.set_children(variable_node, children);
    }

    fn add_variable_type_name_detail_parts(
        &mut self,
        variable: &CompactVariableDeclarationParts<'_>,
    ) {
        let Some(type_name) = self.add_ast_node(variable.type_name) else {
            return;
        };
        let user_defined_path = self.add_string_name_locations(
            variable.type_name_user_defined_path,
            variable.type_name_user_defined_path_locations,
        );
        let array_base_types = self.add_ast_node_refs(variable.type_name_array_base_types);
        let array_lengths = self.add_optional_ast_node_refs(variable.type_name_array_lengths);
        let function_parameters = self.add_ast_node(variable.type_name_function_parameters);
        let function_parameter_declarations =
            self.add_ast_node_refs(variable.type_name_function_parameter_declarations);
        let function_return_parameters =
            self.add_ast_node(variable.type_name_function_return_parameters);
        let function_return_parameter_declarations =
            self.add_ast_node_refs(variable.type_name_function_return_parameter_declarations);
        let mapping_key_type = self.add_ast_node(variable.type_name_mapping_key_type);
        let mapping_key_user_defined_path_node =
            self.add_ast_node(variable.type_name_mapping_key_user_defined_path_node);
        let mapping_key_user_defined_path = self.add_string_name_locations(
            variable.type_name_mapping_key_user_defined_path,
            variable.type_name_mapping_key_user_defined_path_locations,
        );
        let mapping_value_type = self.add_ast_node(variable.type_name_mapping_value_type);
        let mapping_value_user_defined_path_node =
            self.add_ast_node(variable.type_name_mapping_value_user_defined_path_node);
        let mapping_value_user_defined_path = self.add_string_name_locations(
            variable.type_name_mapping_value_user_defined_path,
            variable.type_name_mapping_value_user_defined_path_locations,
        );
        let mapping_value_array_base_types =
            self.add_ast_node_refs(variable.type_name_mapping_value_array_base_types);
        let mapping_value_array_lengths =
            self.add_optional_ast_node_refs(variable.type_name_mapping_value_array_lengths);
        let mapping_value_function_parameters =
            self.add_ast_node(variable.type_name_mapping_value_function_parameters);
        let mapping_value_function_parameter_declarations = self
            .add_ast_node_refs(variable.type_name_mapping_value_function_parameter_declarations);
        let mapping_value_function_return_parameters =
            self.add_ast_node(variable.type_name_mapping_value_function_return_parameters);
        let mapping_value_function_return_parameter_declarations = self.add_ast_node_refs(
            variable.type_name_mapping_value_function_return_parameter_declarations,
        );
        let mapping_refs = variable
            .type_name_mapping_details
            .iter()
            .filter_map(|mapping| self.add_mapping_type_name(mapping))
            .collect::<Vec<_>>();
        let mapping_details = self.output.push_refs(mapping_refs);
        let user_defined_path_node = self.add_ast_node(variable.type_name_user_defined_path_node);
        let mapping_key_name = self
            .output
            .intern_text(&variable.type_name_mapping_key_name.bytes);
        let mapping_value_name = self
            .output
            .intern_text(&variable.type_name_mapping_value_name.bytes);

        self.output.type_name_details.push(CompactTypeNameDetail {
            type_name: Some(type_name),
            elementary_token: variable.type_name_elementary_token,
            elementary_first_number: variable.type_name_elementary_first_number,
            elementary_second_number: variable.type_name_elementary_second_number,
            has_state_mutability: variable.type_name_has_state_mutability,
            state_mutability: variable.type_name_state_mutability,
            user_defined_path_node,
            user_defined_path,
            array_base_types,
            array_lengths,
            function_parameters,
            function_parameter_declarations,
            function_return_parameters,
            function_return_parameter_declarations,
            function_visibility: variable.type_name_function_visibility,
            function_state_mutability: variable.type_name_function_state_mutability,
            mapping_key_type,
            mapping_key_elementary_token: variable.type_name_mapping_key_elementary_token,
            mapping_key_elementary_first_number: variable
                .type_name_mapping_key_elementary_first_number,
            mapping_key_elementary_second_number: variable
                .type_name_mapping_key_elementary_second_number,
            mapping_key_user_defined_path_node,
            mapping_key_user_defined_path,
            mapping_key_name,
            mapping_key_name_location: CompactSpan::from(
                variable.type_name_mapping_key_name_location,
            ),
            mapping_value_type,
            mapping_value_elementary_token: variable.type_name_mapping_value_elementary_token,
            mapping_value_elementary_first_number: variable
                .type_name_mapping_value_elementary_first_number,
            mapping_value_elementary_second_number: variable
                .type_name_mapping_value_elementary_second_number,
            mapping_value_has_state_mutability: variable
                .type_name_mapping_value_has_state_mutability,
            mapping_value_state_mutability: variable.type_name_mapping_value_state_mutability,
            mapping_value_user_defined_path_node,
            mapping_value_user_defined_path,
            mapping_value_array_base_types,
            mapping_value_array_lengths,
            mapping_value_function_parameters,
            mapping_value_function_parameter_declarations,
            mapping_value_function_return_parameters,
            mapping_value_function_return_parameter_declarations,
            mapping_value_function_visibility: variable.type_name_mapping_value_function_visibility,
            mapping_value_function_state_mutability: variable
                .type_name_mapping_value_function_state_mutability,
            mapping_value_name,
            mapping_value_name_location: CompactSpan::from(
                variable.type_name_mapping_value_name_location,
            ),
            mapping_details,
        });
    }

    fn add_type_name(&mut self, type_name: &ffi::WireTypeNameResult) {
        self.add_type_name_parts(CompactTypeNameParts::from(type_name));
    }

    fn add_type_name_parts(&mut self, type_name: CompactTypeNameParts<'_>) {
        let Some(node) = self.add_ast_node(type_name.type_name) else {
            return;
        };
        let mut children = Vec::new();
        self.add_optional_child(&mut children, type_name.array_base_type);
        self.add_optional_child(&mut children, type_name.array_length);
        self.extend_children(&mut children, type_name.array_base_types);
        self.extend_children(&mut children, type_name.array_lengths);
        self.add_optional_child(&mut children, type_name.user_defined_path_node);
        self.add_parameter_list_ref(
            type_name.function_parameters,
            type_name.function_parameter_declarations,
            type_name.function_parameter_details,
        );
        self.add_optional_child(&mut children, type_name.function_parameters);
        self.extend_children(&mut children, type_name.function_parameter_declarations);
        self.add_parameter_list_ref(
            type_name.function_return_parameters,
            type_name.function_return_parameter_declarations,
            type_name.function_return_parameter_details,
        );
        self.add_optional_child(&mut children, type_name.function_return_parameters);
        self.extend_children(
            &mut children,
            type_name.function_return_parameter_declarations,
        );
        self.add_optional_child(&mut children, type_name.mapping_key_type);
        self.add_optional_child(&mut children, type_name.mapping_key_user_defined_path_node);
        self.add_optional_child(&mut children, type_name.mapping_value_type);
        self.add_optional_child(
            &mut children,
            type_name.mapping_value_user_defined_path_node,
        );
        self.extend_children(&mut children, type_name.mapping_value_array_base_types);
        self.extend_children(&mut children, type_name.mapping_value_array_lengths);

        let user_defined_path = self.add_string_name_locations(
            type_name.user_defined_path,
            type_name.user_defined_path_locations,
        );
        let array_base_types = self.add_ast_node_refs(type_name.array_base_types);
        let array_lengths = self.add_optional_ast_node_refs(type_name.array_lengths);
        let function_parameter_declarations =
            self.add_ast_node_refs(type_name.function_parameter_declarations);
        let function_return_parameter_declarations =
            self.add_ast_node_refs(type_name.function_return_parameter_declarations);
        let mapping_key_user_defined_path = self.add_string_name_locations(
            type_name.mapping_key_user_defined_path,
            type_name.mapping_key_user_defined_path_locations,
        );
        let mapping_value_user_defined_path = self.add_string_name_locations(
            type_name.mapping_value_user_defined_path,
            type_name.mapping_value_user_defined_path_locations,
        );
        let mapping_value_array_base_types =
            self.add_ast_node_refs(type_name.mapping_value_array_base_types);
        let mapping_value_array_lengths =
            self.add_optional_ast_node_refs(type_name.mapping_value_array_lengths);
        let mapping_value_function_parameter_declarations =
            self.add_ast_node_refs(type_name.mapping_value_function_parameter_declarations);
        let mapping_value_function_return_parameter_declarations =
            self.add_ast_node_refs(type_name.mapping_value_function_return_parameter_declarations);
        let mapping_refs = type_name
            .mapping_details
            .iter()
            .filter_map(|mapping| self.add_mapping_type_name(mapping))
            .collect::<Vec<_>>();
        let mapping_details = self.output.push_refs(mapping_refs);
        let user_defined_path_node = self.add_ast_node(type_name.user_defined_path_node);
        let function_parameters = self.add_ast_node(type_name.function_parameters);
        let function_return_parameters = self.add_ast_node(type_name.function_return_parameters);
        let mapping_key_type = self.add_ast_node(type_name.mapping_key_type);
        let mapping_key_user_defined_path_node =
            self.add_ast_node(type_name.mapping_key_user_defined_path_node);
        let mapping_value_type = self.add_ast_node(type_name.mapping_value_type);
        let mapping_value_user_defined_path_node =
            self.add_ast_node(type_name.mapping_value_user_defined_path_node);
        let mapping_value_function_parameters =
            self.add_ast_node(type_name.mapping_value_function_parameters);
        let mapping_value_function_return_parameters =
            self.add_ast_node(type_name.mapping_value_function_return_parameters);
        let mapping_key_name = self.output.intern_text(&type_name.mapping_key_name.bytes);
        let mapping_value_name = self.output.intern_text(&type_name.mapping_value_name.bytes);
        self.output.type_name_details.push(CompactTypeNameDetail {
            type_name: Some(node),
            elementary_token: type_name.elementary_type_token,
            elementary_first_number: type_name.elementary_type_first_number,
            elementary_second_number: type_name.elementary_type_second_number,
            has_state_mutability: type_name.has_state_mutability,
            state_mutability: type_name.state_mutability,
            user_defined_path_node,
            user_defined_path,
            array_base_types,
            array_lengths,
            function_parameters,
            function_parameter_declarations,
            function_return_parameters,
            function_return_parameter_declarations,
            function_visibility: type_name.function_visibility,
            function_state_mutability: type_name.function_state_mutability,
            mapping_key_type,
            mapping_key_elementary_token: type_name.mapping_key_elementary_token,
            mapping_key_elementary_first_number: type_name.mapping_key_elementary_first_number,
            mapping_key_elementary_second_number: type_name.mapping_key_elementary_second_number,
            mapping_key_user_defined_path_node,
            mapping_key_user_defined_path,
            mapping_key_name,
            mapping_key_name_location: CompactSpan::from(type_name.mapping_key_name_location),
            mapping_value_type,
            mapping_value_elementary_token: type_name.mapping_value_elementary_token,
            mapping_value_elementary_first_number: type_name.mapping_value_elementary_first_number,
            mapping_value_elementary_second_number: type_name
                .mapping_value_elementary_second_number,
            mapping_value_has_state_mutability: type_name.mapping_value_has_state_mutability,
            mapping_value_state_mutability: type_name.mapping_value_state_mutability,
            mapping_value_user_defined_path_node,
            mapping_value_user_defined_path,
            mapping_value_array_base_types,
            mapping_value_array_lengths,
            mapping_value_function_parameters,
            mapping_value_function_parameter_declarations,
            mapping_value_function_return_parameters,
            mapping_value_function_return_parameter_declarations,
            mapping_value_function_visibility: type_name.mapping_value_function_visibility,
            mapping_value_function_state_mutability: type_name
                .mapping_value_function_state_mutability,
            mapping_value_name,
            mapping_value_name_location: CompactSpan::from(type_name.mapping_value_name_location),
            mapping_details,
        });

        self.add_expression_details_ref(type_name.array_length_details);
        self.add_variable_details_ref(type_name.function_parameter_details);
        self.add_variable_details_ref(type_name.function_return_parameter_details);
        self.add_expression_details_ref(type_name.mapping_value_array_length_details);
        self.add_variable_details_ref(type_name.mapping_value_function_parameter_details);
        self.add_variable_details_ref(type_name.mapping_value_function_return_parameter_details);

        self.set_children(node, children);
    }

    fn add_mapping_type_name(
        &mut self,
        mapping: &ffi::WireMappingTypeName,
    ) -> Option<CompactNodeRef> {
        let mapping_node = self.add_ast_node(&mapping.mapping)?;
        let key_type = self.add_ast_node(&mapping.key_type);
        let key_user_defined_path_node =
            self.add_ast_node(&mapping.key_type_user_defined_path_node);
        let key_user_defined_path = self.add_string_name_locations(
            &mapping.key_type_user_defined_path,
            &mapping.key_type_user_defined_path_locations,
        );
        let value_type = self.add_ast_node(&mapping.value_type);
        let value_user_defined_path_node =
            self.add_ast_node(&mapping.value_type_user_defined_path_node);
        let value_user_defined_path = self.add_string_name_locations(
            &mapping.value_type_user_defined_path,
            &mapping.value_type_user_defined_path_locations,
        );
        let value_array_base_types = self.add_ast_node_refs(&mapping.value_type_array_base_types);
        let value_array_lengths =
            self.add_optional_ast_node_refs(&mapping.value_type_array_lengths);
        let value_function_parameters = self.add_ast_node(&mapping.value_type_function_parameters);
        let value_function_parameter_declarations =
            self.add_ast_node_refs(&mapping.value_type_function_parameter_declarations);
        let value_function_return_parameters =
            self.add_ast_node(&mapping.value_type_function_return_parameters);
        let value_function_return_parameter_declarations =
            self.add_ast_node_refs(&mapping.value_type_function_return_parameter_declarations);
        let key_name = self.output.intern_text(&mapping.key_name.bytes);
        let value_name = self.output.intern_text(&mapping.value_name.bytes);
        self.output.mapping_type_names.push(CompactMappingTypeName {
            mapping: Some(mapping_node),
            key_type,
            key_elementary_token: mapping.key_type_elementary_token,
            key_elementary_first_number: mapping.key_type_elementary_first_number,
            key_elementary_second_number: mapping.key_type_elementary_second_number,
            key_user_defined_path_node,
            key_user_defined_path,
            key_name,
            key_name_location: CompactSpan::from(&mapping.key_name_location),
            value_type,
            value_elementary_token: mapping.value_type_elementary_token,
            value_elementary_first_number: mapping.value_type_elementary_first_number,
            value_elementary_second_number: mapping.value_type_elementary_second_number,
            value_has_state_mutability: mapping.value_type_has_state_mutability,
            value_state_mutability: mapping.value_type_state_mutability,
            value_user_defined_path_node,
            value_user_defined_path,
            value_array_base_types,
            value_array_lengths,
            value_function_parameters,
            value_function_parameter_declarations,
            value_function_return_parameters,
            value_function_return_parameter_declarations,
            value_function_visibility: mapping.value_type_function_visibility,
            value_function_state_mutability: mapping.value_type_function_state_mutability,
            value_name,
            value_name_location: CompactSpan::from(&mapping.value_name_location),
        });
        for detail in &mapping.value_type_array_length_details {
            self.add_expression(detail);
        }
        for detail in &mapping.value_type_function_parameter_details {
            self.add_variable(detail);
        }
        for detail in &mapping.value_type_function_return_parameter_details {
            self.add_variable(detail);
        }
        Some(mapping_node)
    }

    fn add_identifier_path(&mut self, path: &ffi::WireIdentifierPathResult) {
        let Some(node) = self.add_ast_node(&path.identifier_path) else {
            return;
        };
        let path = self.add_string_name_locations(&path.path, &path.path_locations);
        self.output
            .identifier_path_details
            .push(CompactIdentifierPathDetail {
                identifier_path: Some(node),
                path,
            });
    }

    fn add_enum_definition(&mut self, enum_definition: &ffi::WireEnumDefinitionResult) {
        let Some(node) = self.add_ast_node(&enum_definition.enum_definition) else {
            return;
        };
        let members = self.add_ast_node_refs(&enum_definition.members);
        let name = self.output.intern_text(&enum_definition.name.bytes);
        let documentation = self.add_ast_node(&enum_definition.documentation);
        self.output
            .enum_definition_details
            .push(CompactEnumDefinitionDetail {
                enum_definition: Some(node),
                name,
                name_location: CompactSpan::from(&enum_definition.name_location),
                members,
                documentation,
            });
        let mut children = Vec::new();
        self.extend_children(&mut children, &enum_definition.members);
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        for member in &enum_definition.member_details {
            self.add_enum_value(member);
        }
        self.set_children(node, children);
    }

    fn add_enum_value(&mut self, member: &ffi::WireEnumValueResult) {
        let Some(node) = self.add_ast_node(&member.enum_value) else {
            return;
        };
        let name = self.output.intern_text(&member.name.bytes);
        let documentation = self.add_ast_node(&member.documentation);
        self.output.enum_value_details.push(CompactEnumValueDetail {
            enum_value: Some(node),
            name,
            name_location: CompactSpan::from(&member.name_location),
            documentation,
        });
        let mut children = Vec::new();
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        self.set_children(node, children);
    }

    fn add_struct_definition(&mut self, struct_definition: &ffi::WireStructDefinitionResult) {
        let Some(node) = self.add_ast_node(&struct_definition.struct_definition) else {
            return;
        };
        let members = self.add_ast_node_refs(&struct_definition.members);
        let name = self.output.intern_text(&struct_definition.name.bytes);
        let documentation = self.add_ast_node(&struct_definition.documentation);
        self.output
            .struct_definition_details
            .push(CompactStructDefinitionDetail {
                struct_definition: Some(node),
                name,
                name_location: CompactSpan::from(&struct_definition.name_location),
                members,
                documentation,
            });
        let mut children = Vec::new();
        self.extend_children(&mut children, &struct_definition.members);
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        for member in &struct_definition.member_details {
            self.add_variable(member);
        }
        self.set_children(node, children);
    }

    fn add_event_definition(&mut self, event: &ffi::WireEventDefinitionResult) {
        let Some(node) = self.add_ast_node(&event.event_definition) else {
            return;
        };
        let documentation = self.add_ast_node(&event.documentation);
        let parameters = self.add_ast_node(&event.parameters);
        let parameter_declarations = self.add_ast_node_refs(&event.parameter_declarations);
        let name = self.output.intern_text(&event.name.bytes);
        self.output
            .event_definition_details
            .push(CompactEventDefinitionDetail {
                event_definition: Some(node),
                name,
                name_location: CompactSpan::from(&event.name_location),
                documentation,
                parameters,
                parameter_declarations,
                anonymous: event.anonymous,
            });
        let mut children = Vec::new();
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        self.add_parameter_list(
            &event.parameters,
            &event.parameter_declarations,
            &event.parameter_details,
        );
        if let Some(parameters) = parameters {
            children.push(parameters);
        }
        self.extend_children(&mut children, &event.parameter_declarations);
        for parameter in &event.parameter_details {
            self.add_variable(parameter);
        }
        self.set_children(node, children);
    }

    fn add_error_definition(&mut self, error: &ffi::WireErrorDefinitionResult) {
        let Some(node) = self.add_ast_node(&error.error_definition) else {
            return;
        };
        let documentation = self.add_ast_node(&error.documentation);
        let parameters = self.add_ast_node(&error.parameters);
        let parameter_declarations = self.add_ast_node_refs(&error.parameter_declarations);
        let name = self.output.intern_text(&error.name.bytes);
        self.output
            .error_definition_details
            .push(CompactErrorDefinitionDetail {
                error_definition: Some(node),
                name,
                name_location: CompactSpan::from(&error.name_location),
                documentation,
                parameters,
                parameter_declarations,
            });
        let mut children = Vec::new();
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        self.add_parameter_list(
            &error.parameters,
            &error.parameter_declarations,
            &error.parameter_details,
        );
        if let Some(parameters) = parameters {
            children.push(parameters);
        }
        self.extend_children(&mut children, &error.parameter_declarations);
        for parameter in &error.parameter_details {
            self.add_variable(parameter);
        }
        self.set_children(node, children);
    }

    fn add_user_defined_value_type(
        &mut self,
        value_type: &ffi::WireUserDefinedValueTypeDefinitionResult,
    ) {
        let Some(node) = self.add_ast_node(&value_type.user_defined_value_type_definition) else {
            return;
        };
        let type_name = self.add_ast_node(&value_type.type_name);
        let name = self.output.intern_text(&value_type.name.bytes);
        self.output.user_defined_value_type_definition_details.push(
            CompactUserDefinedValueTypeDefinitionDetail {
                user_defined_value_type_definition: Some(node),
                name,
                name_location: CompactSpan::from(&value_type.name_location),
                type_name,
                type_name_elementary_token: value_type.type_name_elementary_token,
                type_name_elementary_first_number: value_type.type_name_elementary_first_number,
                type_name_elementary_second_number: value_type.type_name_elementary_second_number,
                type_name_has_state_mutability: value_type.type_name_has_state_mutability,
                type_name_state_mutability: value_type.type_name_state_mutability,
            },
        );

        let mut children = Vec::new();
        if let Some(type_name) = type_name {
            children.push(type_name);
        }
        self.add_type_name(&value_type.type_name_detail);
        self.set_children(node, children);
    }

    fn add_type_class_definition(&mut self, type_class: &ffi::WireTypeClassDefinitionResult) {
        let Some(node) = self.add_ast_node(&type_class.type_class_definition) else {
            return;
        };
        let type_variable = self.add_ast_node(&type_class.type_variable);
        let documentation = self.add_ast_node(&type_class.documentation);
        let sub_nodes = self.add_ast_node_refs(&type_class.sub_nodes);
        let type_variable_name = self
            .output
            .intern_text(&type_class.type_variable_name.bytes);
        let name = self.output.intern_text(&type_class.name.bytes);
        self.output
            .type_class_definition_details
            .push(CompactTypeClassDefinitionDetail {
                type_class_definition: Some(node),
                type_variable,
                type_variable_name,
                type_variable_name_location: CompactSpan::from(
                    &type_class.type_variable_name_location,
                ),
                name,
                name_location: CompactSpan::from(&type_class.name_location),
                documentation,
                sub_nodes,
            });

        let mut children = Vec::new();
        if let Some(type_variable) = type_variable {
            children.push(type_variable);
        }
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        if let Some(nodes) = self
            .output
            .ref_items
            .get(sub_nodes.start as usize..sub_nodes.start as usize + sub_nodes.len as usize)
        {
            children.extend_from_slice(nodes);
        }
        for function in &type_class.sub_node_function_details {
            self.add_function(function);
        }
        self.set_children(node, children);
    }

    fn add_type_class_instantiation(
        &mut self,
        instantiation: &ffi::WireTypeClassInstantiationResult,
    ) {
        let Some(node) = self.add_ast_node(&instantiation.type_class_instantiation) else {
            return;
        };
        let type_constructor = self.add_ast_node(&instantiation.type_constructor);
        let argument_sorts = self.add_ast_node(&instantiation.argument_sorts);
        let argument_sort_parameters =
            self.add_ast_node_refs(&instantiation.argument_sort_parameters);
        let type_class_name = self.add_ast_node(&instantiation.type_class_name);
        let sub_nodes = self.add_ast_node_refs(&instantiation.sub_nodes);
        self.output
            .type_class_instantiation_details
            .push(CompactTypeClassInstantiationDetail {
                type_class_instantiation: Some(node),
                type_constructor,
                argument_sorts,
                argument_sort_parameters,
                type_class_name,
                sub_nodes,
            });

        let mut children = Vec::new();
        if let Some(type_constructor) = type_constructor {
            children.push(type_constructor);
        }
        if let Some(argument_sorts) = argument_sorts {
            children.push(argument_sorts);
        }
        if let Some(nodes) = self.output.ref_items.get(
            argument_sort_parameters.start as usize
                ..argument_sort_parameters.start as usize + argument_sort_parameters.len as usize,
        ) {
            children.extend_from_slice(nodes);
        }
        if let Some(type_class_name) = type_class_name {
            children.push(type_class_name);
        }
        if let Some(nodes) = self
            .output
            .ref_items
            .get(sub_nodes.start as usize..sub_nodes.start as usize + sub_nodes.len as usize)
        {
            children.extend_from_slice(nodes);
        }
        self.add_type_name(&instantiation.type_constructor_detail);
        for argument in &instantiation.argument_sort_details {
            self.add_variable(argument);
        }
        self.add_type_class_name(&instantiation.type_class_name_detail);
        for function in &instantiation.sub_node_function_details {
            self.add_function(function);
        }
        self.set_children(node, children);
    }

    fn add_type_definition(&mut self, type_definition: &ffi::WireTypeDefinitionResult) {
        let Some(node) = self.add_ast_node(&type_definition.type_definition) else {
            return;
        };
        let arguments = self.add_ast_node(&type_definition.arguments);
        let argument_parameters = self.add_ast_node_refs(&type_definition.argument_parameters);
        let expression = self.add_ast_node(&type_definition.expression);
        let name = self.output.intern_text(&type_definition.name.bytes);
        let builtin_name_parameter = self
            .output
            .intern_text(&type_definition.builtin_name_parameter.bytes);
        self.output
            .type_definition_details
            .push(CompactTypeDefinitionDetail {
                type_definition: Some(node),
                name,
                name_location: CompactSpan::from(&type_definition.name_location),
                arguments,
                argument_parameters,
                expression,
                has_builtin_name_parameter: type_definition.has_builtin_name_parameter,
                builtin_name_parameter,
                builtin_name_parameter_location: CompactSpan::from(
                    &type_definition.builtin_name_parameter_location,
                ),
            });

        let mut children = Vec::new();
        self.add_parameter_list(
            &type_definition.arguments,
            &type_definition.argument_parameters,
            &type_definition.argument_details,
        );
        if let Some(arguments) = arguments {
            children.push(arguments);
        }
        if let Some(nodes) = self.output.ref_items.get(
            argument_parameters.start as usize
                ..argument_parameters.start as usize + argument_parameters.len as usize,
        ) {
            children.extend_from_slice(nodes);
        }
        if let Some(expression) = expression {
            children.push(expression);
        }
        for argument in &type_definition.argument_details {
            self.add_variable(argument);
        }
        self.add_expression(&type_definition.expression_detail);
        self.set_children(node, children);
    }

    fn add_parsed_type_definition(&mut self, type_definition: &ParsedTypeDefinitionResult) {
        let Some(node) = self.add_ast_node(&type_definition.type_definition) else {
            return;
        };
        let arguments = self.add_ast_node(&type_definition.arguments);
        let argument_parameters = self.add_ast_node_refs(&type_definition.argument_parameters);
        let expression = self.add_ast_node(&type_definition.expression);
        let name = self.output.intern_text(&type_definition.name.bytes);
        let builtin_name_parameter = self
            .output
            .intern_text(&type_definition.builtin_name_parameter.bytes);
        self.output
            .type_definition_details
            .push(CompactTypeDefinitionDetail {
                type_definition: Some(node),
                name,
                name_location: CompactSpan::from(&type_definition.name_location),
                arguments,
                argument_parameters,
                expression,
                has_builtin_name_parameter: type_definition.has_builtin_name_parameter,
                builtin_name_parameter,
                builtin_name_parameter_location: CompactSpan::from(
                    &type_definition.builtin_name_parameter_location,
                ),
            });

        let mut children = Vec::new();
        self.add_parameter_list_parts(
            &type_definition.arguments,
            &type_definition.argument_parameters,
            &type_definition.argument_details,
        );
        if let Some(arguments) = arguments {
            children.push(arguments);
        }
        if let Some(nodes) = self.output.ref_items.get(
            argument_parameters.start as usize
                ..argument_parameters.start as usize + argument_parameters.len as usize,
        ) {
            children.extend_from_slice(nodes);
        }
        if let Some(expression) = expression {
            children.push(expression);
        }
        self.add_rust_expression(&type_definition.expression_detail);
        self.set_children(node, children);
    }

    fn add_using_directive(&mut self, using_directive: &ffi::WireUsingDirectiveResult) {
        let Some(node) = self.add_ast_node(&using_directive.using_directive) else {
            return;
        };
        let functions = self.add_ast_node_refs(&using_directive.functions);
        let operators = self
            .output
            .push_using_operators(using_directive.operators.iter().map(|operator| {
                CompactUsingOperator {
                    present: operator.present,
                    token: operator.token,
                }
            }));
        let type_name = self.add_ast_node(&using_directive.type_name);
        self.output
            .using_directive_details
            .push(CompactUsingDirectiveDetail {
                using_directive: Some(node),
                functions,
                operators,
                uses_braces: using_directive.uses_braces,
                type_name,
                global: using_directive.global,
            });

        let mut children = Vec::new();
        if let Some(nodes) = self
            .output
            .ref_items
            .get(functions.start as usize..functions.start as usize + functions.len as usize)
        {
            children.extend_from_slice(nodes);
        }
        if let Some(type_name) = type_name {
            children.push(type_name);
        }
        for function in &using_directive.function_details {
            self.add_identifier_path(function);
        }
        self.add_type_name(&using_directive.type_name_detail);
        self.set_children(node, children);
    }

    fn add_modifier_definition(&mut self, modifier: &ffi::WireModifierDefinitionResult) {
        let Some(node) = self.add_ast_node(&modifier.modifier_definition) else {
            return;
        };
        let documentation = self.add_ast_node(&modifier.documentation);
        let parameters = self.add_ast_node(&modifier.parameters);
        let parameter_declarations = self.add_ast_node_refs(&modifier.parameter_declarations);
        let overrides = self.add_ast_node(&modifier.overrides);
        let override_paths = self.add_ast_node_refs(&modifier.override_paths);
        let block = self.add_ast_node(&modifier.block);
        let block_statements = self.add_ast_node_refs(&modifier.block_statements);
        let name = self.output.intern_text(&modifier.name.bytes);
        self.output
            .modifier_definition_details
            .push(CompactModifierDefinitionDetail {
                modifier_definition: Some(node),
                name,
                name_location: CompactSpan::from(&modifier.name_location),
                documentation,
                parameters,
                parameter_declarations,
                is_virtual: modifier.is_virtual,
                overrides,
                override_paths,
                block,
                block_unchecked: modifier.block_unchecked,
                block_statements,
            });

        let mut children = Vec::new();
        self.add_optional_child(&mut children, &modifier.documentation);
        self.add_parameter_list(
            &modifier.parameters,
            &modifier.parameter_declarations,
            &modifier.parameter_details,
        );
        self.add_optional_child(&mut children, &modifier.parameters);
        self.extend_children(&mut children, &modifier.parameter_declarations);
        self.add_optional_child(&mut children, &modifier.overrides);
        self.extend_children(&mut children, &modifier.override_paths);
        self.add_block(
            &modifier.block,
            modifier.block_unchecked,
            &modifier.block_statements,
            &modifier.block_statement_details,
        );
        self.add_optional_child(&mut children, &modifier.block);
        self.extend_children(&mut children, &modifier.block_statements);
        for parameter in &modifier.parameter_details {
            self.add_variable(parameter);
        }
        for override_path in &modifier.override_path_details {
            self.add_identifier_path(override_path);
        }
        self.set_children(node, children);
    }

    fn add_modifier_invocation(&mut self, modifier: &ffi::WireModifierInvocationResult) {
        let Some(node) = self.add_ast_node(&modifier.modifier_invocation) else {
            return;
        };
        let modifier_name = self.add_ast_node(&modifier.modifier_name);
        let modifier_name_path = self.add_string_name_locations(
            &modifier.modifier_name_detail.path,
            &modifier.modifier_name_detail.path_locations,
        );
        let arguments = self.add_ast_node_refs(&modifier.arguments);
        self.output
            .modifier_invocation_details
            .push(CompactModifierInvocationDetail {
                modifier_invocation: Some(node),
                modifier_name,
                modifier_name_path,
                has_arguments: modifier.has_arguments,
                arguments,
            });

        let mut children = Vec::new();
        self.add_optional_child(&mut children, &modifier.modifier_name);
        self.extend_children(&mut children, &modifier.arguments);
        self.add_identifier_path(&modifier.modifier_name_detail);
        for argument in &modifier.argument_details {
            self.add_expression(argument);
        }
        self.set_children(node, children);
    }

    fn add_parsed_modifier_invocation(&mut self, modifier: &ParsedModifierInvocationResult) {
        let Some(node) = self.add_ast_node(&modifier.modifier_invocation) else {
            return;
        };
        let modifier_name = self.add_ast_node(&modifier.modifier_name);
        let modifier_name_path = self.add_string_name_locations(
            &modifier.modifier_name_detail.path,
            &modifier.modifier_name_detail.path_locations,
        );
        let arguments = self.add_ast_node_refs(&modifier.arguments);
        self.output
            .modifier_invocation_details
            .push(CompactModifierInvocationDetail {
                modifier_invocation: Some(node),
                modifier_name,
                modifier_name_path,
                has_arguments: modifier.has_arguments,
                arguments,
            });

        let mut children = Vec::new();
        self.add_optional_child(&mut children, &modifier.modifier_name);
        self.extend_children(&mut children, &modifier.arguments);
        self.add_identifier_path(&modifier.modifier_name_detail);
        for argument in &modifier.argument_details {
            self.add_rust_expression(argument);
        }
        self.set_children(node, children);
    }

    fn add_inheritance_specifier(&mut self, inheritance: &ffi::WireInheritanceSpecifierResult) {
        let Some(node) = self.add_ast_node(&inheritance.inheritance_specifier) else {
            return;
        };
        let base_name = self.add_ast_node(&inheritance.base_name);
        let base_name_path = self.add_string_name_locations(
            &inheritance.base_name_path,
            &inheritance.base_name_path_locations,
        );
        let arguments = self.add_ast_node_refs(&inheritance.arguments);
        self.output
            .inheritance_specifier_details
            .push(CompactInheritanceSpecifierDetail {
                inheritance_specifier: Some(node),
                base_name,
                base_name_path,
                has_arguments: inheritance.has_arguments,
                arguments,
            });

        let mut children = Vec::new();
        if let Some(base_name) = base_name {
            children.push(base_name);
        }
        if let Some(nodes) = self
            .output
            .ref_items
            .get(arguments.start as usize..arguments.start as usize + arguments.len as usize)
        {
            children.extend_from_slice(nodes);
        }
        for argument in &inheritance.argument_details {
            self.add_expression(argument);
        }
        self.set_children(node, children);
    }

    fn add_type_class_name(&mut self, type_class_name: &ffi::WireTypeClassNameResult) {
        let Some(node) = self.add_ast_node(&type_class_name.type_class_name) else {
            return;
        };
        let identifier_path = self.add_ast_node(&type_class_name.identifier_path);
        self.output
            .type_class_name_details
            .push(CompactTypeClassNameDetail {
                type_class_name: Some(node),
                is_builtin: type_class_name.is_builtin,
                builtin_token: type_class_name.builtin_token,
                identifier_path,
            });

        let mut children = Vec::new();
        if let Some(identifier_path) = identifier_path {
            children.push(identifier_path);
        }
        self.add_identifier_path(&type_class_name.identifier_path_detail);
        self.set_children(node, children);
    }
}

pub(crate) struct CompactTypeClassDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    sub_nodes: Vec<CompactNodeRef>,
}

impl CompactTypeClassDefinitionBuilder<'_> {
    pub(crate) fn start_function(&mut self) -> CompactFunctionBuilder<'_> {
        CompactFunctionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        type_class_definition: &ffi::WireAstNode,
        type_variable: &ffi::WireAstNode,
        type_variable_name: &ffi::WireString,
        type_variable_name_location: &ffi::WireSourceLocation,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
    ) {
        let Some(node) = self.arena.add_ast_node(type_class_definition) else {
            return;
        };
        self.parent_children.push(node);

        let type_variable_ref = self.arena.add_ast_node(type_variable);
        let documentation_ref = self.arena.add_ast_node(documentation);
        let sub_nodes = self.arena.output.push_refs(self.sub_nodes.clone());
        let type_variable_name = self.arena.output.intern_text(&type_variable_name.bytes);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .type_class_definition_details
            .push(CompactTypeClassDefinitionDetail {
                type_class_definition: Some(node),
                type_variable: type_variable_ref,
                type_variable_name,
                type_variable_name_location: CompactSpan::from(type_variable_name_location),
                name,
                name_location: CompactSpan::from(name_location),
                documentation: documentation_ref,
                sub_nodes,
            });

        let mut children = Vec::new();
        if let Some(type_variable) = type_variable_ref {
            children.push(type_variable);
        }
        if let Some(documentation) = documentation_ref {
            children.push(documentation);
        }
        children.extend_from_slice(&self.sub_nodes);
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactTypeClassInstantiationBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    sub_nodes: Vec<CompactNodeRef>,
}

impl CompactTypeClassInstantiationBuilder<'_> {
    pub(crate) fn start_function(&mut self) -> CompactFunctionBuilder<'_> {
        CompactFunctionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        type_class_instantiation: &ffi::WireAstNode,
        type_constructor: &ffi::WireAstNode,
        type_constructor_detail: &ParsedTypeNameResult,
        argument_sorts: &ffi::WireAstNode,
        argument_sort_parameters: &[ffi::WireAstNode],
        argument_sort_details: &[ParsedVariableDeclaration],
        type_class_name: &ffi::WireAstNode,
        type_class_name_detail: &ffi::WireTypeClassNameResult,
    ) {
        let Some(node) = self.arena.add_ast_node(type_class_instantiation) else {
            return;
        };
        self.parent_children.push(node);

        let type_constructor_ref = self.arena.add_ast_node(type_constructor);
        let argument_sorts_ref = self.arena.add_ast_node(argument_sorts);
        let argument_sort_parameter_refs = self.arena.add_ast_node_refs(argument_sort_parameters);
        let type_class_name_ref = self.arena.add_ast_node(type_class_name);
        let sub_nodes = self.arena.output.push_refs(self.sub_nodes.clone());
        self.arena.output.type_class_instantiation_details.push(
            CompactTypeClassInstantiationDetail {
                type_class_instantiation: Some(node),
                type_constructor: type_constructor_ref,
                argument_sorts: argument_sorts_ref,
                argument_sort_parameters: argument_sort_parameter_refs,
                type_class_name: type_class_name_ref,
                sub_nodes,
            },
        );

        let mut children = Vec::new();
        self.arena
            .add_type_name_parts(type_constructor_detail.as_compact_parts());
        if let Some(type_constructor) = type_constructor_ref {
            children.push(type_constructor);
        }
        self.arena.add_parameter_list_parts(
            argument_sorts,
            argument_sort_parameters,
            argument_sort_details,
        );
        if let Some(argument_sorts) = argument_sorts_ref {
            children.push(argument_sorts);
        }
        self.arena
            .extend_children(&mut children, argument_sort_parameters);
        self.arena.add_type_class_name(type_class_name_detail);
        if let Some(type_class_name) = type_class_name_ref {
            children.push(type_class_name);
        }
        children.extend_from_slice(&self.sub_nodes);
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactSourceUnitBuilder {
    arena: CompactArenaBuilder,
    source_unit_children: Vec<CompactNodeRef>,
}

impl CompactSourceUnitBuilder {
    pub(crate) fn new() -> Self {
        Self {
            arena: CompactArenaBuilder::new(),
            source_unit_children: Vec::new(),
        }
    }

    fn push_source_unit_child(&mut self, node: &ffi::WireAstNode) {
        if let Some(node_ref) = self.arena.add_ast_node(node) {
            self.source_unit_children.push(node_ref);
        }
    }

    pub(crate) fn add_pragma(&mut self, pragma: &ffi::WirePragmaDirectiveResult) {
        self.push_source_unit_child(&pragma.pragma_directive);
        self.arena.add_pragma(pragma);
    }

    pub(crate) fn add_import(&mut self, import: &ffi::WireImportDirectiveResult) {
        self.push_source_unit_child(&import.import_directive);
        self.arena.add_import(import);
    }

    pub(crate) fn add_user_defined_value_type(
        &mut self,
        value_type: &ffi::WireUserDefinedValueTypeDefinitionResult,
    ) {
        self.push_source_unit_child(&value_type.user_defined_value_type_definition);
        self.arena.add_user_defined_value_type(value_type);
    }

    pub(crate) fn add_enum_definition(&mut self, enum_definition: &ffi::WireEnumDefinitionResult) {
        self.push_source_unit_child(&enum_definition.enum_definition);
        self.arena.add_enum_definition(enum_definition);
    }

    pub(crate) fn add_struct_definition(
        &mut self,
        struct_definition: &ffi::WireStructDefinitionResult,
    ) {
        self.push_source_unit_child(&struct_definition.struct_definition);
        self.arena.add_struct_definition(struct_definition);
    }

    pub(crate) fn add_event_definition(&mut self, event: &ffi::WireEventDefinitionResult) {
        self.push_source_unit_child(&event.event_definition);
        self.arena.add_event_definition(event);
    }

    pub(crate) fn add_error_definition(&mut self, error: &ffi::WireErrorDefinitionResult) {
        self.push_source_unit_child(&error.error_definition);
        self.arena.add_error_definition(error);
    }

    pub(crate) fn add_contract(&mut self, contract: &ffi::WireContractDefinitionResult) {
        self.push_source_unit_child(&contract.contract_definition);
        self.arena.add_contract(contract);
    }

    pub(crate) fn add_function(&mut self, function: &ffi::WireFunctionDefinitionResult) {
        self.push_source_unit_child(&function.function_definition);
        self.arena.add_function(function);
    }

    pub(crate) fn add_for_all_quantifier(&mut self, quantifier: &ffi::WireForAllQuantifierResult) {
        self.push_source_unit_child(&quantifier.for_all_quantifier);
        self.arena.add_for_all_quantifier(quantifier);
    }

    pub(crate) fn start_for_all_quantifier(&mut self) -> CompactForAllQuantifierBuilder<'_> {
        CompactForAllQuantifierBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            quantified_function: Vec::new(),
        }
    }

    pub(crate) fn add_type_definition(&mut self, type_definition: &ffi::WireTypeDefinitionResult) {
        self.push_source_unit_child(&type_definition.type_definition);
        self.arena.add_type_definition(type_definition);
    }

    pub(crate) fn add_parsed_type_definition(
        &mut self,
        type_definition: &ParsedTypeDefinitionResult,
    ) {
        self.push_source_unit_child(&type_definition.type_definition);
        self.arena.add_parsed_type_definition(type_definition);
    }

    pub(crate) fn add_type_class_definition(
        &mut self,
        type_class: &ffi::WireTypeClassDefinitionResult,
    ) {
        self.push_source_unit_child(&type_class.type_class_definition);
        self.arena.add_type_class_definition(type_class);
    }

    pub(crate) fn start_type_class_definition(&mut self) -> CompactTypeClassDefinitionBuilder<'_> {
        CompactTypeClassDefinitionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            sub_nodes: Vec::new(),
        }
    }

    pub(crate) fn add_type_class_instantiation(
        &mut self,
        instantiation: &ffi::WireTypeClassInstantiationResult,
    ) {
        self.push_source_unit_child(&instantiation.type_class_instantiation);
        self.arena.add_type_class_instantiation(instantiation);
    }

    pub(crate) fn start_type_class_instantiation(
        &mut self,
    ) -> CompactTypeClassInstantiationBuilder<'_> {
        CompactTypeClassInstantiationBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            sub_nodes: Vec::new(),
        }
    }

    pub(crate) fn add_using_directive(&mut self, using_directive: &ffi::WireUsingDirectiveResult) {
        self.push_source_unit_child(&using_directive.using_directive);
        self.arena.add_using_directive(using_directive);
    }

    pub(crate) fn add_variable(&mut self, variable: &ffi::WireVariableDeclarationResult) {
        self.push_source_unit_child(&variable.variable_declaration);
        self.arena.add_variable(variable);
    }

    pub(crate) fn start_variable(&mut self) -> CompactVariableDeclarationBuilder<'_> {
        CompactVariableDeclarationBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
        }
    }

    pub(crate) fn finish_success(
        mut self,
        source_unit: &ffi::WireAstNode,
        has_license: bool,
        license: &ffi::WireString,
        experimental_solidity: bool,
        max_id: CompactNodeId,
        errors: Vec<ffi::WireParserError>,
        warnings: Vec<ffi::WireParserError>,
    ) -> CompactParseOutput {
        self.arena.output.ok = true;
        self.arena.output.error_code = 0;
        self.arena.output.experimental_solidity = experimental_solidity;
        self.arena.output.max_id = max_id;
        if has_license {
            self.arena.output.has_license = true;
            self.arena.output.license = self.arena.output.intern_text(&license.bytes);
        }

        self.arena.output.root = self.arena.add_ast_node(source_unit);
        if let Some(root) = self.arena.output.root {
            self.arena
                .set_children(root, std::mem::take(&mut self.source_unit_children));
        }

        self.add_diagnostics(errors, warnings);
        self.arena.output
    }

    pub(crate) fn finish_error(
        mut self,
        message: &str,
        experimental_solidity: bool,
        max_id: CompactNodeId,
        errors: Vec<ffi::WireParserError>,
        warnings: Vec<ffi::WireParserError>,
    ) -> CompactParseOutput {
        self.arena.output.ok = false;
        self.arena.output.error_code = 1;
        self.arena.output.error_message = self.arena.output.intern_text(message.as_bytes());
        self.arena.output.experimental_solidity = experimental_solidity;
        self.arena.output.max_id = max_id;
        self.add_diagnostics(errors, warnings);
        self.arena.output
    }

    fn add_diagnostics(
        &mut self,
        errors: Vec<ffi::WireParserError>,
        warnings: Vec<ffi::WireParserError>,
    ) {
        for error in errors {
            let diagnostic = self.arena.add_diagnostic(&error);
            self.arena.output.diagnostics.push(diagnostic);
        }
        for warning in warnings {
            let diagnostic = self.arena.add_diagnostic(&warning);
            self.arena.output.warnings.push(diagnostic);
        }
    }

    pub(crate) fn start_contract(&mut self) -> CompactContractBuilder<'_> {
        CompactContractBuilder {
            arena: &mut self.arena,
            source_unit_children: &mut self.source_unit_children,
            base_contracts: Vec::new(),
            sub_nodes: Vec::new(),
        }
    }

    pub(crate) fn start_function(&mut self) -> CompactFunctionBuilder<'_> {
        CompactFunctionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
        }
    }

    pub(crate) fn start_struct_definition(&mut self) -> CompactStructDefinitionBuilder<'_> {
        CompactStructDefinitionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            members: Vec::new(),
        }
    }

    pub(crate) fn start_enum_definition(&mut self) -> CompactEnumDefinitionBuilder<'_> {
        CompactEnumDefinitionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            members: Vec::new(),
        }
    }

    pub(crate) fn start_event_definition(&mut self) -> CompactEventDefinitionBuilder<'_> {
        CompactEventDefinitionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
        }
    }

    pub(crate) fn start_error_definition(&mut self) -> CompactErrorDefinitionBuilder<'_> {
        CompactErrorDefinitionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
        }
    }

    pub(crate) fn start_user_defined_value_type_definition(
        &mut self,
    ) -> CompactUserDefinedValueTypeDefinitionBuilder<'_> {
        CompactUserDefinedValueTypeDefinitionBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
        }
    }

    pub(crate) fn start_using_directive(&mut self) -> CompactUsingDirectiveBuilder<'_> {
        CompactUsingDirectiveBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            functions: Vec::new(),
            operators: Vec::new(),
        }
    }

    pub(crate) fn start_pragma_directive(&mut self) -> CompactPragmaDirectiveBuilder<'_> {
        CompactPragmaDirectiveBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
        }
    }

    pub(crate) fn start_import_directive(&mut self) -> CompactImportDirectiveBuilder<'_> {
        CompactImportDirectiveBuilder {
            arena: &mut self.arena,
            parent_children: &mut self.source_unit_children,
            aliases: Vec::new(),
            children: Vec::new(),
        }
    }
}

pub(crate) struct CompactFunctionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactFunctionBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        function_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        is_free_function: bool,
        kind: u32,
        documentation: &ffi::WireAstNode,
        header: &ParsedFunctionHeaderParserResult,
        block: &ffi::WireAstNode,
        block_unchecked: bool,
        block_statements: &[ffi::WireAstNode],
        block_statement_details: &[RustStatementResult],
    ) {
        let Some(function_node) = self.arena.add_ast_node(function_definition) else {
            return;
        };
        self.parent_children.push(function_node);

        let documentation_ref = self.arena.add_ast_node(documentation);
        let overrides_ref = self.arena.add_ast_node(&header.overrides);
        let override_paths = self.arena.add_ast_node_refs(&header.override_paths);
        let parameters = self.arena.add_ast_node(&header.parameters);
        let parameter_declarations = self.arena.add_ast_node_refs(&header.parameter_declarations);
        let modifiers = self.arena.add_ast_node_refs(&header.modifiers);
        let return_parameters = self.arena.add_ast_node(&header.return_parameters);
        let return_parameter_declarations = self
            .arena
            .add_ast_node_refs(&header.return_parameter_declarations);
        let block_ref = self.arena.add_ast_node(block);
        let block_statement_refs = self.arena.add_ast_node_refs(block_statements);
        let experimental_return_expression = self
            .arena
            .add_ast_node(&header.experimental_return_expression);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .function_definition_details
            .push(CompactFunctionDefinitionDetail {
                function_definition: Some(function_node),
                name,
                name_location: CompactSpan::from(name_location),
                visibility: header.visibility,
                state_mutability: header.state_mutability,
                is_free_function,
                kind,
                is_virtual: header.is_virtual,
                documentation: documentation_ref,
                overrides: overrides_ref,
                override_paths,
                parameters,
                parameter_declarations,
                modifiers,
                return_parameters,
                return_parameter_declarations,
                block: block_ref,
                block_unchecked,
                block_statements: block_statement_refs,
                experimental_return_expression,
            });

        let mut children = Vec::new();
        self.arena.add_optional_child(&mut children, documentation);
        self.arena
            .add_optional_child(&mut children, &header.overrides);
        self.arena
            .extend_children(&mut children, &header.override_paths);
        self.arena.add_parameter_list_parts(
            &header.parameters,
            &header.parameter_declarations,
            &header.parameter_details,
        );
        self.arena
            .add_optional_child(&mut children, &header.parameters);
        self.arena
            .extend_children(&mut children, &header.parameter_declarations);
        self.arena.extend_children(&mut children, &header.modifiers);
        self.arena.add_parameter_list_parts(
            &header.return_parameters,
            &header.return_parameter_declarations,
            &header.return_parameter_details,
        );
        self.arena
            .add_optional_child(&mut children, &header.return_parameters);
        self.arena
            .extend_children(&mut children, &header.return_parameter_declarations);
        self.arena.add_rust_block(
            block,
            block_unchecked,
            block_statements,
            block_statement_details,
        );
        self.arena.add_optional_child(&mut children, block);
        self.arena.extend_children(&mut children, block_statements);
        self.arena
            .add_optional_child(&mut children, &header.experimental_return_expression);

        for override_path in &header.override_path_details {
            self.arena.add_identifier_path(override_path);
        }
        for modifier in &header.modifier_details {
            self.arena.add_parsed_modifier_invocation(modifier);
        }
        self.arena
            .add_rust_expression(&header.experimental_return_expression_detail);

        self.arena.set_children(function_node, children);
    }
}

pub(crate) struct CompactForAllQuantifierBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    quantified_function: Vec<CompactNodeRef>,
}

impl CompactForAllQuantifierBuilder<'_> {
    pub(crate) fn start_function(&mut self) -> CompactFunctionBuilder<'_> {
        CompactFunctionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.quantified_function,
        }
    }

    pub(crate) fn finish(
        self,
        for_all_quantifier: &ffi::WireAstNode,
        type_variable_declarations: &ffi::WireAstNode,
        type_variable_declaration_parameters: &[ffi::WireAstNode],
        type_variable_declaration_details: &[ParsedVariableDeclaration],
        quantified_function: &ffi::WireAstNode,
    ) {
        let Some(node) = self.arena.add_ast_node(for_all_quantifier) else {
            return;
        };
        self.parent_children.push(node);

        let type_variable_declarations_ref = self.arena.add_ast_node(type_variable_declarations);
        let type_variable_declaration_parameter_refs = self
            .arena
            .add_ast_node_refs(type_variable_declaration_parameters);
        let quantified_function_ref = self.arena.add_ast_node(quantified_function);
        self.arena
            .output
            .for_all_quantifier_details
            .push(CompactForAllQuantifierDetail {
                for_all_quantifier: Some(node),
                type_variable_declarations: type_variable_declarations_ref,
                type_variable_declaration_parameters: type_variable_declaration_parameter_refs,
                quantified_function: quantified_function_ref,
            });

        let mut children = Vec::new();
        self.arena.add_parameter_list_parts(
            type_variable_declarations,
            type_variable_declaration_parameters,
            type_variable_declaration_details,
        );
        if let Some(type_variable_declarations) = type_variable_declarations_ref {
            children.push(type_variable_declarations);
        }
        self.arena
            .extend_children(&mut children, type_variable_declaration_parameters);
        children.extend_from_slice(&self.quantified_function);
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactModifierDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactModifierDefinitionBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        modifier_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
        parameters: &ffi::WireAstNode,
        parameter_declarations: &[ffi::WireAstNode],
        parameter_details: &[ParsedVariableDeclaration],
        is_virtual: bool,
        overrides: &ffi::WireAstNode,
        override_paths: &[ffi::WireAstNode],
        override_path_details: &[ffi::WireIdentifierPathResult],
        block: &ffi::WireAstNode,
        block_unchecked: bool,
        block_statements: &[ffi::WireAstNode],
        block_statement_details: &[RustStatementResult],
    ) {
        let Some(node) = self.arena.add_ast_node(modifier_definition) else {
            return;
        };
        self.parent_children.push(node);

        let documentation_ref = self.arena.add_ast_node(documentation);
        let parameters_ref = self.arena.add_ast_node(parameters);
        let parameter_declaration_refs = self.arena.add_ast_node_refs(parameter_declarations);
        let overrides_ref = self.arena.add_ast_node(overrides);
        let override_path_refs = self.arena.add_ast_node_refs(override_paths);
        let block_ref = self.arena.add_ast_node(block);
        let block_statement_refs = self.arena.add_ast_node_refs(block_statements);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .modifier_definition_details
            .push(CompactModifierDefinitionDetail {
                modifier_definition: Some(node),
                name,
                name_location: CompactSpan::from(name_location),
                documentation: documentation_ref,
                parameters: parameters_ref,
                parameter_declarations: parameter_declaration_refs,
                is_virtual,
                overrides: overrides_ref,
                override_paths: override_path_refs,
                block: block_ref,
                block_unchecked,
                block_statements: block_statement_refs,
            });

        let mut children = Vec::new();
        self.arena.add_optional_child(&mut children, documentation);
        self.arena
            .add_parameter_list_parts(parameters, parameter_declarations, parameter_details);
        self.arena.add_optional_child(&mut children, parameters);
        self.arena
            .extend_children(&mut children, parameter_declarations);
        self.arena.add_optional_child(&mut children, overrides);
        self.arena.extend_children(&mut children, override_paths);
        self.arena.add_rust_block(
            block,
            block_unchecked,
            block_statements,
            block_statement_details,
        );
        self.arena.add_optional_child(&mut children, block);
        self.arena.extend_children(&mut children, block_statements);
        for override_path in override_path_details {
            self.arena.add_identifier_path(override_path);
        }
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactStructDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    members: Vec<CompactNodeRef>,
}

pub(crate) struct CompactVariableDeclarationBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactVariableDeclarationBuilder<'_> {
    pub(crate) fn finish(self, variable: CompactVariableDeclarationParts<'_>) {
        if let Some(node) = self.arena.add_ast_node(variable.variable_declaration) {
            self.parent_children.push(node);
        }
        self.arena.add_variable_parts(variable);
    }
}

impl CompactStructDefinitionBuilder<'_> {
    pub(crate) fn add_member(&mut self, member: &ffi::WireVariableDeclarationResult) {
        if let Some(node) = self.arena.add_ast_node(&member.variable_declaration) {
            self.members.push(node);
        }
        self.arena.add_variable(member);
    }

    pub(crate) fn start_member(&mut self) -> CompactVariableDeclarationBuilder<'_> {
        CompactVariableDeclarationBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.members,
        }
    }

    pub(crate) fn finish(
        self,
        struct_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
    ) {
        let Some(node) = self.arena.add_ast_node(struct_definition) else {
            return;
        };
        self.parent_children.push(node);

        let members = self.arena.output.push_refs(self.members.clone());
        let name = self.arena.output.intern_text(&name.bytes);
        let documentation_ref = self.arena.add_ast_node(documentation);
        self.arena
            .output
            .struct_definition_details
            .push(CompactStructDefinitionDetail {
                struct_definition: Some(node),
                name,
                name_location: CompactSpan::from(name_location),
                members,
                documentation: documentation_ref,
            });

        let mut children = self.members;
        if let Some(documentation) = documentation_ref {
            children.push(documentation);
        }
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactEnumDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    members: Vec<CompactNodeRef>,
}

impl CompactEnumDefinitionBuilder<'_> {
    pub(crate) fn add_member(&mut self, member: &ffi::WireEnumValueResult) {
        if let Some(node) = self.arena.add_ast_node(&member.enum_value) {
            self.members.push(node);
        }
        self.arena.add_enum_value(member);
    }

    pub(crate) fn finish(
        self,
        enum_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
    ) {
        let Some(node) = self.arena.add_ast_node(enum_definition) else {
            return;
        };
        self.parent_children.push(node);

        let members = self.arena.output.push_refs(self.members.clone());
        let name = self.arena.output.intern_text(&name.bytes);
        let documentation_ref = self.arena.add_ast_node(documentation);
        self.arena
            .output
            .enum_definition_details
            .push(CompactEnumDefinitionDetail {
                enum_definition: Some(node),
                name,
                name_location: CompactSpan::from(name_location),
                members,
                documentation: documentation_ref,
            });

        let mut children = self.members;
        if let Some(documentation) = documentation_ref {
            children.push(documentation);
        }
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactEventDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactEventDefinitionBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        event_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
        parameters: &ffi::WireAstNode,
        parameter_declarations: &[ffi::WireAstNode],
        parameter_details: &[ParsedVariableDeclaration],
        anonymous: bool,
    ) {
        let Some(node) = self.arena.add_ast_node(event_definition) else {
            return;
        };
        self.parent_children.push(node);

        let documentation_ref = self.arena.add_ast_node(documentation);
        let parameters_ref = self.arena.add_ast_node(parameters);
        let parameter_declaration_refs = self.arena.add_ast_node_refs(parameter_declarations);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .event_definition_details
            .push(CompactEventDefinitionDetail {
                event_definition: Some(node),
                name,
                name_location: CompactSpan::from(name_location),
                documentation: documentation_ref,
                parameters: parameters_ref,
                parameter_declarations: parameter_declaration_refs,
                anonymous,
            });

        let mut children = Vec::new();
        if let Some(documentation) = documentation_ref {
            children.push(documentation);
        }
        self.arena
            .add_parameter_list_parts(parameters, parameter_declarations, parameter_details);
        if let Some(parameters) = parameters_ref {
            children.push(parameters);
        }
        self.arena
            .extend_children(&mut children, parameter_declarations);
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactErrorDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactErrorDefinitionBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        error_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
        parameters: &ffi::WireAstNode,
        parameter_declarations: &[ffi::WireAstNode],
        parameter_details: &[ParsedVariableDeclaration],
    ) {
        let Some(node) = self.arena.add_ast_node(error_definition) else {
            return;
        };
        self.parent_children.push(node);

        let documentation_ref = self.arena.add_ast_node(documentation);
        let parameters_ref = self.arena.add_ast_node(parameters);
        let parameter_declaration_refs = self.arena.add_ast_node_refs(parameter_declarations);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .error_definition_details
            .push(CompactErrorDefinitionDetail {
                error_definition: Some(node),
                name,
                name_location: CompactSpan::from(name_location),
                documentation: documentation_ref,
                parameters: parameters_ref,
                parameter_declarations: parameter_declaration_refs,
            });

        let mut children = Vec::new();
        if let Some(documentation) = documentation_ref {
            children.push(documentation);
        }
        self.arena
            .add_parameter_list_parts(parameters, parameter_declarations, parameter_details);
        if let Some(parameters) = parameters_ref {
            children.push(parameters);
        }
        self.arena
            .extend_children(&mut children, parameter_declarations);
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactUserDefinedValueTypeDefinitionBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactUserDefinedValueTypeDefinitionBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        value_type_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        type_name: &ffi::WireAstNode,
        type_name_elementary_token: u32,
        type_name_elementary_first_number: u32,
        type_name_elementary_second_number: u32,
        type_name_has_state_mutability: bool,
        type_name_state_mutability: u8,
        type_name_detail: &ParsedTypeNameResult,
    ) {
        let Some(node) = self.arena.add_ast_node(value_type_definition) else {
            return;
        };
        self.parent_children.push(node);

        let type_name_ref = self.arena.add_ast_node(type_name);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .user_defined_value_type_definition_details
            .push(CompactUserDefinedValueTypeDefinitionDetail {
                user_defined_value_type_definition: Some(node),
                name,
                name_location: CompactSpan::from(name_location),
                type_name: type_name_ref,
                type_name_elementary_token,
                type_name_elementary_first_number,
                type_name_elementary_second_number,
                type_name_has_state_mutability,
                type_name_state_mutability,
            });

        let mut children = Vec::new();
        if let Some(type_name) = type_name_ref {
            children.push(type_name);
        }
        self.arena
            .add_type_name_parts(type_name_detail.as_compact_parts());
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactUsingDirectiveBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    functions: Vec<CompactNodeRef>,
    operators: Vec<CompactUsingOperator>,
}

impl CompactUsingDirectiveBuilder<'_> {
    pub(crate) fn add_function(
        &mut self,
        function: &ffi::WireIdentifierPathResult,
        operator: Option<u32>,
    ) {
        if let Some(node) = self.arena.add_ast_node(&function.identifier_path) {
            self.functions.push(node);
        }
        self.operators.push(CompactUsingOperator {
            present: operator.is_some(),
            token: operator.unwrap_or(token::TOKEN_EOS),
        });
        self.arena.add_identifier_path(function);
    }

    pub(crate) fn finish(
        self,
        using_directive: &ffi::WireAstNode,
        uses_braces: bool,
        type_name: &ffi::WireAstNode,
        type_name_detail: &ParsedTypeNameResult,
        global: bool,
    ) {
        let Some(node) = self.arena.add_ast_node(using_directive) else {
            return;
        };
        self.parent_children.push(node);

        let functions = self.arena.output.push_refs(self.functions.clone());
        let operators = self.arena.output.push_using_operators(self.operators);
        let type_name_ref = self.arena.add_ast_node(type_name);
        self.arena
            .output
            .using_directive_details
            .push(CompactUsingDirectiveDetail {
                using_directive: Some(node),
                functions,
                operators,
                uses_braces,
                type_name: type_name_ref,
                global,
            });

        let mut children = self.functions;
        if let Some(type_name) = type_name_ref {
            children.push(type_name);
        }
        self.arena
            .add_type_name_parts(type_name_detail.as_compact_parts());
        self.arena.set_children(node, children);
    }
}

pub(crate) struct CompactPragmaDirectiveBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactPragmaDirectiveBuilder<'_> {
    pub(crate) fn finish(
        self,
        pragma_directive: &ffi::WireAstNode,
        tokens: &[u32],
        literals: &[ffi::WireString],
    ) {
        let Some(node) = self.arena.add_ast_node(pragma_directive) else {
            return;
        };
        self.parent_children.push(node);

        let token_literals = self.arena.output.push_token_literals(tokens, literals);
        self.arena
            .output
            .pragma_directive_details
            .push(CompactPragmaDirectiveDetail {
                pragma_directive: Some(node),
                token_literals,
            });
    }
}

pub(crate) struct CompactImportDirectiveBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
    aliases: Vec<CompactImportAlias>,
    children: Vec<CompactNodeRef>,
}

impl CompactImportDirectiveBuilder<'_> {
    pub(crate) fn add_symbol_alias(
        &mut self,
        symbol: &ffi::WireAstNode,
        has_alias: bool,
        alias: &ffi::WireString,
        location: &ffi::WireSourceLocation,
    ) {
        let symbol_ref = self.arena.add_ast_node(symbol);
        if let Some(symbol) = symbol_ref {
            self.children.push(symbol);
        }
        self.aliases.push(CompactImportAlias {
            symbol: symbol_ref,
            has_alias,
            alias: self.arena.output.intern_text(&alias.bytes),
            span: CompactSpan::from(location),
        });
    }

    pub(crate) fn finish(
        self,
        import_directive: &ffi::WireAstNode,
        path: &ffi::WireString,
        unit_alias: &ffi::WireString,
        unit_alias_location: &ffi::WireSourceLocation,
    ) {
        let Some(node) = self.arena.add_ast_node(import_directive) else {
            return;
        };
        self.parent_children.push(node);

        let symbol_aliases = self.arena.output.push_import_aliases(self.aliases);
        let path = self.arena.output.intern_text(&path.bytes);
        let unit_alias = self.arena.output.intern_text(&unit_alias.bytes);
        self.arena
            .output
            .import_directive_details
            .push(CompactImportDirectiveDetail {
                import_directive: Some(node),
                path,
                unit_alias,
                unit_alias_location: CompactSpan::from(unit_alias_location),
                symbol_aliases,
            });
        self.arena.set_children(node, self.children);
    }
}

pub(crate) struct CompactContractBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    source_unit_children: &'a mut Vec<CompactNodeRef>,
    base_contracts: Vec<CompactNodeRef>,
    sub_nodes: Vec<CompactNodeRef>,
}

impl CompactContractBuilder<'_> {
    pub(crate) fn add_base_contract(
        &mut self,
        base_contract: &ffi::WireInheritanceSpecifierResult,
    ) {
        if let Some(node) = self
            .arena
            .add_ast_node(&base_contract.inheritance_specifier)
        {
            self.base_contracts.push(node);
        }
        self.arena.add_inheritance_specifier(base_contract);
    }

    pub(crate) fn start_base_contract(&mut self) -> CompactInheritanceSpecifierBuilder<'_> {
        CompactInheritanceSpecifierBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.base_contracts,
        }
    }

    pub(crate) fn add_struct_definition(
        &mut self,
        struct_definition: &ffi::WireStructDefinitionResult,
    ) {
        if let Some(node) = self
            .arena
            .add_ast_node(&struct_definition.struct_definition)
        {
            self.sub_nodes.push(node);
        }
        self.arena.add_struct_definition(struct_definition);
    }

    pub(crate) fn add_enum_definition(&mut self, enum_definition: &ffi::WireEnumDefinitionResult) {
        if let Some(node) = self.arena.add_ast_node(&enum_definition.enum_definition) {
            self.sub_nodes.push(node);
        }
        self.arena.add_enum_definition(enum_definition);
    }

    pub(crate) fn add_user_defined_value_type(
        &mut self,
        value_type: &ffi::WireUserDefinedValueTypeDefinitionResult,
    ) {
        if let Some(node) = self
            .arena
            .add_ast_node(&value_type.user_defined_value_type_definition)
        {
            self.sub_nodes.push(node);
        }
        self.arena.add_user_defined_value_type(value_type);
    }

    pub(crate) fn add_event_definition(&mut self, event: &ffi::WireEventDefinitionResult) {
        if let Some(node) = self.arena.add_ast_node(&event.event_definition) {
            self.sub_nodes.push(node);
        }
        self.arena.add_event_definition(event);
    }

    pub(crate) fn add_error_definition(&mut self, error: &ffi::WireErrorDefinitionResult) {
        if let Some(node) = self.arena.add_ast_node(&error.error_definition) {
            self.sub_nodes.push(node);
        }
        self.arena.add_error_definition(error);
    }

    pub(crate) fn add_function(&mut self, function: &ffi::WireFunctionDefinitionResult) {
        if let Some(node) = self.arena.add_ast_node(&function.function_definition) {
            self.sub_nodes.push(node);
        }
        self.arena.add_function(function);
    }

    pub(crate) fn start_function(&mut self) -> CompactFunctionBuilder<'_> {
        CompactFunctionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    pub(crate) fn add_modifier_definition(&mut self, modifier: &ffi::WireModifierDefinitionResult) {
        if let Some(node) = self.arena.add_ast_node(&modifier.modifier_definition) {
            self.sub_nodes.push(node);
        }
        self.arena.add_modifier_definition(modifier);
    }

    pub(crate) fn start_modifier_definition(&mut self) -> CompactModifierDefinitionBuilder<'_> {
        CompactModifierDefinitionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    pub(crate) fn start_struct_definition(&mut self) -> CompactStructDefinitionBuilder<'_> {
        CompactStructDefinitionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
            members: Vec::new(),
        }
    }

    pub(crate) fn start_enum_definition(&mut self) -> CompactEnumDefinitionBuilder<'_> {
        CompactEnumDefinitionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
            members: Vec::new(),
        }
    }

    pub(crate) fn start_event_definition(&mut self) -> CompactEventDefinitionBuilder<'_> {
        CompactEventDefinitionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    pub(crate) fn start_error_definition(&mut self) -> CompactErrorDefinitionBuilder<'_> {
        CompactErrorDefinitionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    pub(crate) fn start_user_defined_value_type_definition(
        &mut self,
    ) -> CompactUserDefinedValueTypeDefinitionBuilder<'_> {
        CompactUserDefinedValueTypeDefinitionBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    pub(crate) fn start_using_directive(&mut self) -> CompactUsingDirectiveBuilder<'_> {
        CompactUsingDirectiveBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
            functions: Vec::new(),
            operators: Vec::new(),
        }
    }

    pub(crate) fn add_using_directive(&mut self, using_directive: &ffi::WireUsingDirectiveResult) {
        if let Some(node) = self.arena.add_ast_node(&using_directive.using_directive) {
            self.sub_nodes.push(node);
        }
        self.arena.add_using_directive(using_directive);
    }

    pub(crate) fn add_variable(&mut self, variable: &ffi::WireVariableDeclarationResult) {
        if let Some(node) = self.arena.add_ast_node(&variable.variable_declaration) {
            self.sub_nodes.push(node);
        }
        self.arena.add_variable(variable);
    }

    pub(crate) fn start_variable(&mut self) -> CompactVariableDeclarationBuilder<'_> {
        CompactVariableDeclarationBuilder {
            arena: &mut *self.arena,
            parent_children: &mut self.sub_nodes,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish(
        self,
        contract_definition: &ffi::WireAstNode,
        name: &ffi::WireString,
        name_location: &ffi::WireSourceLocation,
        documentation: &ffi::WireAstNode,
        contract_kind: u8,
        is_abstract: bool,
        storage_layout_specifier: &ffi::WireAstNode,
        storage_layout_base_slot_expression: &ffi::WireAstNode,
        storage_layout_base_slot_expression_detail: &RustExpressionResult,
    ) {
        let Some(contract_node) = self.arena.add_ast_node(contract_definition) else {
            return;
        };
        self.source_unit_children.push(contract_node);

        let documentation = self.arena.add_ast_node(documentation);
        let storage_layout_specifier = self.arena.add_ast_node(storage_layout_specifier);
        let storage_layout_base_slot_expression =
            self.arena.add_ast_node(storage_layout_base_slot_expression);

        let mut children = Vec::new();
        if let Some(documentation) = documentation {
            children.push(documentation);
        }
        children.extend_from_slice(&self.base_contracts);
        children.extend_from_slice(&self.sub_nodes);
        if let Some(storage_layout_specifier) = storage_layout_specifier {
            children.push(storage_layout_specifier);
        }
        if let Some(storage_layout_base_slot_expression) = storage_layout_base_slot_expression {
            children.push(storage_layout_base_slot_expression);
        }

        let base_contracts = self.arena.output.push_refs(self.base_contracts);
        let sub_nodes = self.arena.output.push_refs(self.sub_nodes);
        let name = self.arena.output.intern_text(&name.bytes);
        self.arena
            .output
            .contract_definition_details
            .push(CompactContractDefinitionDetail {
                contract_definition: Some(contract_node),
                name,
                name_location: CompactSpan::from(name_location),
                documentation,
                base_contracts,
                sub_nodes,
                contract_kind,
                is_abstract,
                storage_layout_specifier,
                storage_layout_base_slot_expression,
            });
        self.arena
            .add_rust_expression(storage_layout_base_slot_expression_detail);
        self.arena.set_children(contract_node, children);
    }
}

pub(crate) struct CompactInheritanceSpecifierBuilder<'a> {
    arena: &'a mut CompactArenaBuilder,
    parent_children: &'a mut Vec<CompactNodeRef>,
}

impl CompactInheritanceSpecifierBuilder<'_> {
    pub(crate) fn finish(
        self,
        inheritance_specifier: &ffi::WireAstNode,
        base_name: &ffi::WireAstNode,
        base_name_path: &[ffi::WireString],
        base_name_path_locations: &[ffi::WireSourceLocation],
        has_arguments: bool,
        arguments: &[ffi::WireAstNode],
        argument_details: &[RustExpressionResult],
    ) {
        let Some(node) = self.arena.add_ast_node(inheritance_specifier) else {
            return;
        };
        self.parent_children.push(node);

        let base_name_ref = self.arena.add_ast_node(base_name);
        let base_name_path = self
            .arena
            .add_string_name_locations(base_name_path, base_name_path_locations);
        let arguments_ref = self.arena.add_ast_node_refs(arguments);
        self.arena
            .output
            .inheritance_specifier_details
            .push(CompactInheritanceSpecifierDetail {
                inheritance_specifier: Some(node),
                base_name: base_name_ref,
                base_name_path,
                has_arguments,
                arguments: arguments_ref,
            });

        let mut children = Vec::new();
        if let Some(base_name) = base_name_ref {
            children.push(base_name);
        }
        self.arena.extend_children(&mut children, arguments);
        for argument in argument_details {
            self.arena.add_rust_expression(argument);
        }
        self.arena.set_children(node, children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse_source(source: &[u8]) -> ffi::WireParserResult {
        parser::set_parser_source_input_with_evm_version(
            ffi::WireString {
                bytes: source.to_vec(),
            },
            0,
            String::new(),
            "osaka".to_string(),
        );
        parser::parse()
    }

    fn list_children(output: &CompactParseOutput, node: CompactNodeRef) -> &[CompactNodeRef] {
        let node = output.node(node).expect("node exists");
        let CompactPayload::List { children } = node.payload else {
            panic!("expected list payload");
        };
        output.children(children).expect("valid child range")
    }

    #[test]
    fn compact_arena_builds_source_unit_tree_from_wire_result() {
        let result = parse_source(b"contract C { function f(uint x) public { x; } }");
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        let root = output.root.expect("source unit root");
        assert_eq!(
            output.node(root).expect("root node").kind,
            parser::AST_NODE_KIND_SOURCE_UNIT
        );
        assert_eq!(output.max_id, result.max_id);

        let root_children = list_children(&output, root);
        assert!(root_children.iter().any(|child| {
            output.node(*child).expect("child node").kind
                == parser::AST_NODE_KIND_CONTRACT_DEFINITION
        }));
        assert!(output
            .nodes
            .iter()
            .any(|node| node.kind == parser::AST_NODE_KIND_FUNCTION_DEFINITION));
        assert!(output
            .nodes
            .iter()
            .any(|node| node.kind == parser::AST_NODE_KIND_BLOCK));
        assert!(output
            .nodes
            .iter()
            .any(|node| node.kind == parser::AST_NODE_KIND_EXPRESSION_STATEMENT));
        assert!(output.nodes.len() > result.source_unit_nodes.len());
    }

    #[test]
    fn compact_owned_adapter_matches_borrowed_arena_shape() {
        let result =
            parse_source(b"contract C { uint256[2] xs; function f(uint x) public { xs[0] = x; } }");
        assert!(result.ok, "{:?}", result.errors);

        let borrowed = CompactParseOutput::from_wire_parser_result(&result);
        let owned = CompactParseOutput::from_wire_parser_result_owned(result);

        assert_eq!(owned.ok, borrowed.ok);
        assert_eq!(owned.error_code, borrowed.error_code);
        assert_eq!(owned.root, borrowed.root);
        assert_eq!(owned.max_id, borrowed.max_id);
        assert_eq!(owned.experimental_solidity, borrowed.experimental_solidity);
        assert_eq!(owned.nodes, borrowed.nodes);
        assert_eq!(owned.children, borrowed.children);
        assert_eq!(owned.ref_items, borrowed.ref_items);
        assert_eq!(owned.text, borrowed.text);
        assert_eq!(
            owned.function_definition_details.len(),
            borrowed.function_definition_details.len()
        );
        assert_eq!(
            owned.variable_declaration_details.len(),
            borrowed.variable_declaration_details.len()
        );
        assert_eq!(
            owned.expression_details.len(),
            borrowed.expression_details.len()
        );
        assert_eq!(
            owned.statement_details.len(),
            borrowed.statement_details.len()
        );
    }

    #[test]
    fn compact_arena_deduplicates_nodes_by_ast_id() {
        let result = parse_source(b"contract C { function f() public { if (true) { return; } } }");
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        let mut ids = output.nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), output.nodes.len());
    }

    #[test]
    fn compact_arena_interns_node_text() {
        let result = parse_source(b"contract C { function f() public { uint256 localValue; } }");
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        assert!(output
            .nodes
            .iter()
            .any(|node| output.text(node.text) == b"C"));
        assert!(output
            .nodes
            .iter()
            .any(|node| output.text(node.text) == b"localValue"));
    }

    #[test]
    fn compact_parser_handle_exposes_wire_result_and_arena() {
        parser::set_parser_source_input_with_evm_version(
            ffi::WireString {
                bytes: b"contract C {}".to_vec(),
            },
            0,
            String::new(),
            "osaka".to_string(),
        );

        let handle = parse_compact_with_legacy();
        let wire = compact_parser_legacy_wire_result(&handle);
        let arena = compact_parser_arena(&handle);

        assert!(wire.ok, "{:?}", wire.errors);
        assert!(arena.ok);
        assert!(!arena.has_license);
        assert!(arena.error_message.is_empty());
        assert!(arena.has_root);
        let root = &arena.nodes[arena.root.index as usize];
        assert_eq!(root.kind, parser::AST_NODE_KIND_SOURCE_UNIT);
        assert_eq!(root.payload_kind, WIRE_COMPACT_PAYLOAD_LIST);
        assert_eq!(arena.max_id, wire.max_id);
    }

    #[test]
    fn compact_arena_populates_expression_and_statement_detail_tables() {
        let result =
            parse_source(b"contract C { function f(uint x) public { if (x > 1) { x; } } }");
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        assert!(output
            .statement_details
            .iter()
            .any(|detail| output.node(detail.statement.expect("statement")).is_some()));
        assert!(output.expression_details.iter().any(|detail| output
            .node(detail.expression.expect("expression"))
            .is_some()));
        assert!(output
            .expression_details
            .iter()
            .any(|detail| detail.left_expression.is_some() && detail.right_expression.is_some()));
    }

    #[test]
    fn compact_arena_populates_function_and_modifier_detail_tables() {
        let result = parse_source(
            b"contract B { function f() public virtual {} } contract C is B { modifier m(uint x) { _; } function f() public override m(1) {} }",
        );
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        assert_eq!(output.function_definition_details.len(), 2);
        assert_eq!(output.modifier_definition_details.len(), 1);
        assert_eq!(output.modifier_invocation_details.len(), 1);
        assert!(!output.identifier_path_details.is_empty());
        assert!(output.function_definition_details.iter().all(|detail| {
            output
                .node(detail.function_definition.expect("function node"))
                .is_some_and(|node| node.kind == parser::AST_NODE_KIND_FUNCTION_DEFINITION)
        }));
        let modifier = &output.modifier_invocation_details[0];
        assert!(output
            .node(
                modifier
                    .modifier_invocation
                    .expect("modifier invocation node")
            )
            .is_some());
        assert!(modifier.has_arguments);
        assert_eq!(modifier.arguments.len, 1);
        assert!(modifier.modifier_name_path.len > 0);
    }

    #[test]
    fn compact_arena_populates_source_unit_detail_tables() {
        let result = parse_source(
            br#"pragma abicoder v2; import {A as B} from "x.sol"; enum E { One, Two }"#,
        );
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        assert_eq!(output.pragma_directive_details.len(), 1);
        let pragma = &output.pragma_directive_details[0];
        assert!(output
            .node(pragma.pragma_directive.expect("pragma node"))
            .is_some());
        assert!(pragma.token_literals.len > 0);
        assert_eq!(output.import_directive_details.len(), 1);
        let import = &output.import_directive_details[0];
        assert!(output
            .node(import.import_directive.expect("import node"))
            .is_some());
        assert_eq!(import.symbol_aliases.len, 1);
        assert!(!output.text(import.path).is_empty());
        assert_eq!(output.enum_definition_details.len(), 1);
        assert_eq!(output.enum_value_details.len(), 2);
        let enum_definition = &output.enum_definition_details[0];
        assert!(output
            .node(enum_definition.enum_definition.expect("enum node"))
            .is_some());
        assert_eq!(output.text(enum_definition.name), b"E");
        assert_eq!(enum_definition.members.len, 2);
    }

    #[test]
    fn compact_arena_populates_type_name_detail_tables() {
        let result = parse_source(
            b"contract D {} library L {} using L for mapping(address => D); contract C { struct S { D x; } event E(D indexed d); error Err(D d); D member; function f() public { D local; new D(); } }",
        );
        assert!(result.ok, "{:?}", result.errors);

        let output = CompactParseOutput::from_wire_parser_result(&result);
        assert!(!output.type_name_details.is_empty());
        assert!(!output.mapping_type_names.is_empty());
        assert!(!output.variable_declaration_details.is_empty());
        assert!(!output.struct_definition_details.is_empty());
        assert!(!output.event_definition_details.is_empty());
        assert!(!output.error_definition_details.is_empty());
        assert!(output.type_name_details.iter().any(|detail| {
            output
                .node(detail.type_name.expect("type name node"))
                .is_some_and(|node| node.kind == parser::AST_NODE_KIND_USER_DEFINED_TYPE_NAME)
        }));
        assert!(output.mapping_type_names.iter().any(|detail| {
            output
                .node(detail.mapping.expect("mapping node"))
                .is_some_and(|node| node.kind == parser::AST_NODE_KIND_MAPPING)
        }));
    }
}
