#[cxx::bridge(namespace = "solidity::frontend::rust")]
pub mod ffi {
    #[derive(Clone, Debug)]
    struct WireString {
        bytes: Vec<u8>,
    }

    #[derive(Clone, Debug)]
    struct WireToken {
        token: u32,
        literal: WireString,
        token_name: String,
    }

    #[derive(Clone, Debug)]
    struct WireSourceLocation {
        start: i64,
        end: i64,
        source_id: i64,
    }

    #[derive(Clone, Debug)]
    struct WireLocatedToken {
        token: u32,
        literal: WireString,
        token_name: String,
        first_number: u32,
        second_number: u32,
        error: String,
        location: WireSourceLocation,
    }

    #[derive(Clone, Debug)]
    struct WireAstNode {
        present: bool,
        node_id: i64,
        kind: u8,
        location: WireSourceLocation,
        text: WireString,
    }

    #[derive(Clone, Debug)]
    struct WireParserResult {
        ok: bool,
        error_code: u8,
        error_message: String,
        source_unit: WireAstNode,
        source_unit_nodes: Vec<WireAstNode>,
        source_unit_pragmas: Vec<WirePragmaDirectiveResult>,
        source_unit_imports: Vec<WireImportDirectiveResult>,
        source_unit_user_defined_value_types: Vec<WireUserDefinedValueTypeDefinitionResult>,
        source_unit_enums: Vec<WireEnumDefinitionResult>,
        source_unit_structs: Vec<WireStructDefinitionResult>,
        source_unit_events: Vec<WireEventDefinitionResult>,
        source_unit_errors: Vec<WireErrorDefinitionResult>,
        source_unit_contracts: Vec<WireContractDefinitionResult>,
        source_unit_functions: Vec<WireFunctionDefinitionResult>,
        source_unit_for_all_quantifiers: Vec<WireForAllQuantifierResult>,
        source_unit_type_definitions: Vec<WireTypeDefinitionResult>,
        source_unit_type_class_definitions: Vec<WireTypeClassDefinitionResult>,
        source_unit_type_class_instantiations: Vec<WireTypeClassInstantiationResult>,
        source_unit_using_directives: Vec<WireUsingDirectiveResult>,
        source_unit_variable_declarations: Vec<WireVariableDeclarationResult>,
        errors: Vec<WireParserError>,
        warnings: Vec<WireParserError>,
        has_license: bool,
        license: WireString,
        experimental_solidity: bool,
        max_id: i64,
    }

    #[derive(Clone, Debug)]
    struct WireParserError {
        error_id: u32,
        message: String,
        location: WireSourceLocation,
        secondary_locations: Vec<WireSecondarySourceLocation>,
        syntax: bool,
        fatal: bool,
    }

    #[derive(Clone, Debug)]
    struct WireSecondarySourceLocation {
        message: String,
        location: WireSourceLocation,
    }

    #[derive(Clone, Copy, Debug)]
    struct WireCompactNodeRef {
        index: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct WireCompactChildRange {
        start: u32,
        len: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct WireCompactTextRef {
        start: u32,
        len: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct WireCompactSpan {
        start: i32,
        end: i32,
        source_id: i32,
    }

    #[derive(Clone, Debug)]
    struct WireCompactNode {
        id: i64,
        kind: u8,
        span: WireCompactSpan,
        text: WireCompactTextRef,
        payload_kind: u8,
        child_range: WireCompactChildRange,
        aux_child_range: WireCompactChildRange,
        first_child: WireCompactNodeRef,
        second_child: WireCompactNodeRef,
        third_child: WireCompactNodeRef,
        prefix: bool,
        has_first_child: bool,
        has_second_child: bool,
        has_third_child: bool,
        token: u32,
        first_number: u32,
        second_number: u32,
    }

    #[derive(Clone, Debug)]
    struct WireCompactSecondaryLocation {
        message: WireCompactTextRef,
        span: WireCompactSpan,
    }

    #[derive(Clone, Copy, Debug)]
    struct WireCompactSecondaryRange {
        start: u32,
        len: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct WireCompactRefRange {
        start: u32,
        len: u32,
    }

    #[derive(Clone, Debug)]
    struct WireCompactNameLocation {
        text: WireCompactTextRef,
        span: WireCompactSpan,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTokenLiteral {
        token: u32,
        literal: WireCompactTextRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactImportAlias {
        symbol: WireCompactNodeRef,
        has_alias: bool,
        alias: WireCompactTextRef,
        span: WireCompactSpan,
    }

    #[derive(Clone, Debug)]
    struct WireCompactPragmaDirectiveDetail {
        pragma_directive: WireCompactNodeRef,
        token_literals: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactImportDirectiveDetail {
        import_directive: WireCompactNodeRef,
        path: WireCompactTextRef,
        unit_alias: WireCompactTextRef,
        unit_alias_location: WireCompactSpan,
        symbol_aliases: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactUsingOperator {
        present: bool,
        token: u32,
    }

    #[derive(Clone, Debug)]
    struct WireCompactContractDefinitionDetail {
        contract_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        documentation: WireCompactNodeRef,
        base_contracts: WireCompactRefRange,
        sub_nodes: WireCompactRefRange,
        contract_kind: u8,
        is_abstract: bool,
        storage_layout_specifier: WireCompactNodeRef,
        storage_layout_base_slot_expression: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactInheritanceSpecifierDetail {
        inheritance_specifier: WireCompactNodeRef,
        base_name: WireCompactNodeRef,
        base_name_path: WireCompactRefRange,
        has_arguments: bool,
        arguments: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactUsingDirectiveDetail {
        using_directive: WireCompactNodeRef,
        functions: WireCompactRefRange,
        operators: WireCompactRefRange,
        uses_braces: bool,
        type_name: WireCompactNodeRef,
        global: bool,
    }

    #[derive(Clone, Debug)]
    struct WireCompactIdentifierPathDetail {
        identifier_path: WireCompactNodeRef,
        path: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactModifierInvocationDetail {
        modifier_invocation: WireCompactNodeRef,
        modifier_name: WireCompactNodeRef,
        modifier_name_path: WireCompactRefRange,
        has_arguments: bool,
        arguments: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactFunctionDefinitionDetail {
        function_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        visibility: u8,
        state_mutability: u8,
        is_free_function: bool,
        kind: u32,
        is_virtual: bool,
        documentation: WireCompactNodeRef,
        overrides: WireCompactNodeRef,
        override_paths: WireCompactRefRange,
        parameters: WireCompactNodeRef,
        parameter_declarations: WireCompactRefRange,
        modifiers: WireCompactRefRange,
        return_parameters: WireCompactNodeRef,
        return_parameter_declarations: WireCompactRefRange,
        block: WireCompactNodeRef,
        block_unchecked: bool,
        block_statements: WireCompactRefRange,
        experimental_return_expression: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactModifierDefinitionDetail {
        modifier_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        documentation: WireCompactNodeRef,
        parameters: WireCompactNodeRef,
        parameter_declarations: WireCompactRefRange,
        is_virtual: bool,
        overrides: WireCompactNodeRef,
        override_paths: WireCompactRefRange,
        block: WireCompactNodeRef,
        block_unchecked: bool,
        block_statements: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactEnumValueDetail {
        enum_value: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        documentation: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactEnumDefinitionDetail {
        enum_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        members: WireCompactRefRange,
        documentation: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactStructDefinitionDetail {
        struct_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        members: WireCompactRefRange,
        documentation: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactEventDefinitionDetail {
        event_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        documentation: WireCompactNodeRef,
        parameters: WireCompactNodeRef,
        parameter_declarations: WireCompactRefRange,
        anonymous: bool,
    }

    #[derive(Clone, Debug)]
    struct WireCompactErrorDefinitionDetail {
        error_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        documentation: WireCompactNodeRef,
        parameters: WireCompactNodeRef,
        parameter_declarations: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactUserDefinedValueTypeDefinitionDetail {
        user_defined_value_type_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        type_name: WireCompactNodeRef,
        type_name_elementary_token: u32,
        type_name_elementary_first_number: u32,
        type_name_elementary_second_number: u32,
        type_name_has_state_mutability: bool,
        type_name_state_mutability: u8,
    }

    #[derive(Clone, Debug)]
    struct WireCompactForAllQuantifierDetail {
        for_all_quantifier: WireCompactNodeRef,
        type_variable_declarations: WireCompactNodeRef,
        type_variable_declaration_parameters: WireCompactRefRange,
        quantified_function: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTypeDefinitionDetail {
        type_definition: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        arguments: WireCompactNodeRef,
        argument_parameters: WireCompactRefRange,
        expression: WireCompactNodeRef,
        has_builtin_name_parameter: bool,
        builtin_name_parameter: WireCompactTextRef,
        builtin_name_parameter_location: WireCompactSpan,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTypeClassNameDetail {
        type_class_name: WireCompactNodeRef,
        is_builtin: bool,
        builtin_token: u32,
        identifier_path: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTypeClassDefinitionDetail {
        type_class_definition: WireCompactNodeRef,
        type_variable: WireCompactNodeRef,
        type_variable_name: WireCompactTextRef,
        type_variable_name_location: WireCompactSpan,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        documentation: WireCompactNodeRef,
        sub_nodes: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTypeClassInstantiationDetail {
        type_class_instantiation: WireCompactNodeRef,
        type_constructor: WireCompactNodeRef,
        argument_sorts: WireCompactNodeRef,
        argument_sort_parameters: WireCompactRefRange,
        type_class_name: WireCompactNodeRef,
        sub_nodes: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactMappingTypeName {
        mapping: WireCompactNodeRef,
        key_type: WireCompactNodeRef,
        key_elementary_token: u32,
        key_elementary_first_number: u32,
        key_elementary_second_number: u32,
        key_user_defined_path_node: WireCompactNodeRef,
        key_user_defined_path: WireCompactRefRange,
        key_name: WireCompactTextRef,
        key_name_location: WireCompactSpan,
        value_type: WireCompactNodeRef,
        value_elementary_token: u32,
        value_elementary_first_number: u32,
        value_elementary_second_number: u32,
        value_has_state_mutability: bool,
        value_state_mutability: u8,
        value_user_defined_path_node: WireCompactNodeRef,
        value_user_defined_path: WireCompactRefRange,
        value_array_base_types: WireCompactRefRange,
        value_array_lengths: WireCompactRefRange,
        value_function_parameters: WireCompactNodeRef,
        value_function_parameter_declarations: WireCompactRefRange,
        value_function_return_parameters: WireCompactNodeRef,
        value_function_return_parameter_declarations: WireCompactRefRange,
        value_function_visibility: u8,
        value_function_state_mutability: u8,
        value_name: WireCompactTextRef,
        value_name_location: WireCompactSpan,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTypeNameDetail {
        type_name: WireCompactNodeRef,
        elementary_token: u32,
        elementary_first_number: u32,
        elementary_second_number: u32,
        has_state_mutability: bool,
        state_mutability: u8,
        user_defined_path_node: WireCompactNodeRef,
        user_defined_path: WireCompactRefRange,
        array_base_types: WireCompactRefRange,
        array_lengths: WireCompactRefRange,
        function_parameters: WireCompactNodeRef,
        function_parameter_declarations: WireCompactRefRange,
        function_return_parameters: WireCompactNodeRef,
        function_return_parameter_declarations: WireCompactRefRange,
        function_visibility: u8,
        function_state_mutability: u8,
        mapping_key_type: WireCompactNodeRef,
        mapping_key_elementary_token: u32,
        mapping_key_elementary_first_number: u32,
        mapping_key_elementary_second_number: u32,
        mapping_key_user_defined_path_node: WireCompactNodeRef,
        mapping_key_user_defined_path: WireCompactRefRange,
        mapping_key_name: WireCompactTextRef,
        mapping_key_name_location: WireCompactSpan,
        mapping_value_type: WireCompactNodeRef,
        mapping_value_elementary_token: u32,
        mapping_value_elementary_first_number: u32,
        mapping_value_elementary_second_number: u32,
        mapping_value_has_state_mutability: bool,
        mapping_value_state_mutability: u8,
        mapping_value_user_defined_path_node: WireCompactNodeRef,
        mapping_value_user_defined_path: WireCompactRefRange,
        mapping_value_array_base_types: WireCompactRefRange,
        mapping_value_array_lengths: WireCompactRefRange,
        mapping_value_function_parameters: WireCompactNodeRef,
        mapping_value_function_parameter_declarations: WireCompactRefRange,
        mapping_value_function_return_parameters: WireCompactNodeRef,
        mapping_value_function_return_parameter_declarations: WireCompactRefRange,
        mapping_value_function_visibility: u8,
        mapping_value_function_state_mutability: u8,
        mapping_value_name: WireCompactTextRef,
        mapping_value_name_location: WireCompactSpan,
        mapping_details: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactVariableDeclarationDetail {
        variable_declaration: WireCompactNodeRef,
        type_name: WireCompactNodeRef,
        type_expression: WireCompactNodeRef,
        documentation: WireCompactNodeRef,
        overrides: WireCompactNodeRef,
        override_paths: WireCompactRefRange,
        value: WireCompactNodeRef,
        name: WireCompactTextRef,
        name_location: WireCompactSpan,
        visibility: u8,
        mutability: u8,
        variable_location: u8,
        indexed: bool,
    }

    #[derive(Clone, Debug)]
    struct WireCompactExpressionDetail {
        expression: WireCompactNodeRef,
        left_expression: WireCompactNodeRef,
        right_expression: WireCompactNodeRef,
        condition_expression: WireCompactNodeRef,
        true_expression: WireCompactNodeRef,
        false_expression: WireCompactNodeRef,
        sub_expression: WireCompactNodeRef,
        base_expression: WireCompactNodeRef,
        base_expression_type: WireCompactNodeRef,
        index_expression: WireCompactNodeRef,
        end_index_expression: WireCompactNodeRef,
        type_name: WireCompactNodeRef,
        expression_type: WireCompactNodeRef,
        arguments: WireCompactRefRange,
        argument_names: WireCompactRefRange,
        components: WireCompactRefRange,
        member_name_location: WireCompactSpan,
        is_prefix_operation: bool,
        is_inline_array: bool,
        literal_token: u32,
        literal_subdenomination: u32,
    }

    #[derive(Clone, Debug)]
    struct WireCompactTryCatchClauseDetail {
        try_catch_clause: WireCompactNodeRef,
        error_name: WireCompactTextRef,
        error_parameters: WireCompactNodeRef,
        error_parameter_declarations: WireCompactRefRange,
        block: WireCompactNodeRef,
        block_unchecked: bool,
        block_statements: WireCompactRefRange,
    }

    #[derive(Clone, Debug)]
    struct WireCompactStatementDetail {
        statement: WireCompactNodeRef,
        block_unchecked: bool,
        block_statements: WireCompactRefRange,
        inline_assembly_flags: WireCompactRefRange,
        inline_assembly_block_location: WireCompactSpan,
        condition_expression: WireCompactNodeRef,
        true_body: WireCompactNodeRef,
        false_body: WireCompactNodeRef,
        body: WireCompactNodeRef,
        is_do_while: bool,
        external_call: WireCompactNodeRef,
        clauses: WireCompactRefRange,
        clause_error_parameters: WireCompactRefRange,
        clause_blocks: WireCompactRefRange,
        init_expression: WireCompactNodeRef,
        loop_expression: WireCompactNodeRef,
        event_call: WireCompactNodeRef,
        event_call_callee: WireCompactNodeRef,
        event_call_arguments: WireCompactRefRange,
        event_call_parameter_names: WireCompactRefRange,
        error_call: WireCompactNodeRef,
        error_call_callee: WireCompactNodeRef,
        error_call_arguments: WireCompactRefRange,
        error_call_parameter_names: WireCompactRefRange,
        expression: WireCompactNodeRef,
        variables: WireCompactRefRange,
        initial_value: WireCompactNodeRef,
    }

    #[derive(Clone, Debug)]
    struct WireCompactDiagnostic {
        error_id: u32,
        message: WireCompactTextRef,
        span: WireCompactSpan,
        secondary_locations: WireCompactSecondaryRange,
        syntax: bool,
        fatal: bool,
    }

    #[derive(Clone, Debug)]
    struct WireCompactParseOutput {
        ok: bool,
        error_code: u8,
        error_message: String,
        has_root: bool,
        root: WireCompactNodeRef,
        nodes: Vec<WireCompactNode>,
        children: Vec<WireCompactNodeRef>,
        text: Vec<u8>,
        secondary_locations: Vec<WireCompactSecondaryLocation>,
        ref_items: Vec<WireCompactNodeRef>,
        name_locations: Vec<WireCompactNameLocation>,
        token_literals: Vec<WireCompactTokenLiteral>,
        import_aliases: Vec<WireCompactImportAlias>,
        using_operators: Vec<WireCompactUsingOperator>,
        pragma_directive_details: Vec<WireCompactPragmaDirectiveDetail>,
        import_directive_details: Vec<WireCompactImportDirectiveDetail>,
        contract_definition_details: Vec<WireCompactContractDefinitionDetail>,
        inheritance_specifier_details: Vec<WireCompactInheritanceSpecifierDetail>,
        using_directive_details: Vec<WireCompactUsingDirectiveDetail>,
        identifier_path_details: Vec<WireCompactIdentifierPathDetail>,
        modifier_invocation_details: Vec<WireCompactModifierInvocationDetail>,
        function_definition_details: Vec<WireCompactFunctionDefinitionDetail>,
        modifier_definition_details: Vec<WireCompactModifierDefinitionDetail>,
        enum_value_details: Vec<WireCompactEnumValueDetail>,
        enum_definition_details: Vec<WireCompactEnumDefinitionDetail>,
        struct_definition_details: Vec<WireCompactStructDefinitionDetail>,
        event_definition_details: Vec<WireCompactEventDefinitionDetail>,
        error_definition_details: Vec<WireCompactErrorDefinitionDetail>,
        user_defined_value_type_definition_details:
            Vec<WireCompactUserDefinedValueTypeDefinitionDetail>,
        for_all_quantifier_details: Vec<WireCompactForAllQuantifierDetail>,
        type_definition_details: Vec<WireCompactTypeDefinitionDetail>,
        type_class_name_details: Vec<WireCompactTypeClassNameDetail>,
        type_class_definition_details: Vec<WireCompactTypeClassDefinitionDetail>,
        type_class_instantiation_details: Vec<WireCompactTypeClassInstantiationDetail>,
        mapping_type_names: Vec<WireCompactMappingTypeName>,
        type_name_details: Vec<WireCompactTypeNameDetail>,
        variable_declaration_details: Vec<WireCompactVariableDeclarationDetail>,
        expression_details: Vec<WireCompactExpressionDetail>,
        statement_details: Vec<WireCompactStatementDetail>,
        try_catch_clause_details: Vec<WireCompactTryCatchClauseDetail>,
        diagnostics: Vec<WireCompactDiagnostic>,
        warnings: Vec<WireCompactDiagnostic>,
        has_license: bool,
        max_id: i64,
        experimental_solidity: bool,
        license: WireCompactTextRef,
    }

    #[derive(Clone, Debug)]
    struct WireStructuredDocumentationResult {
        documentation: WireAstNode,
        current_node_id: i64,
    }

    #[derive(Clone, Debug)]
    struct WirePragmaVersionResult {
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WirePragmaDirectiveResult {
        pragma_directive: WireAstNode,
        tokens: Vec<u32>,
        literals: Vec<WireString>,
        experimental_solidity_enabled: bool,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireImportSymbolAlias {
        symbol: WireAstNode,
        has_alias: bool,
        alias: WireString,
        location: WireSourceLocation,
    }

    #[derive(Clone, Debug)]
    struct WireImportDirectiveResult {
        import_directive: WireAstNode,
        path: WireString,
        unit_alias: WireString,
        unit_alias_location: WireSourceLocation,
        symbol_aliases: Vec<WireImportSymbolAlias>,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireContractKindResult {
        contract_kind: u8,
        is_abstract: bool,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireContractDefinitionResult {
        contract_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        base_contracts: Vec<WireAstNode>,
        base_contract_details: Vec<WireInheritanceSpecifierResult>,
        sub_nodes: Vec<WireAstNode>,
        sub_node_structs: Vec<WireStructDefinitionResult>,
        sub_node_enums: Vec<WireEnumDefinitionResult>,
        sub_node_user_defined_value_types: Vec<WireUserDefinedValueTypeDefinitionResult>,
        sub_node_events: Vec<WireEventDefinitionResult>,
        sub_node_errors: Vec<WireErrorDefinitionResult>,
        sub_node_functions: Vec<WireFunctionDefinitionResult>,
        sub_node_modifiers: Vec<WireModifierDefinitionResult>,
        sub_node_using_directives: Vec<WireUsingDirectiveResult>,
        sub_node_variable_declarations: Vec<WireVariableDeclarationResult>,
        contract_kind: u8,
        is_abstract: bool,
        storage_layout_specifier: WireAstNode,
        storage_layout_base_slot_expression: WireAstNode,
        storage_layout_base_slot_expression_detail: WireExpressionResult,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireStorageLayoutSpecifierResult {
        storage_layout_specifier: WireAstNode,
        base_slot_expression: WireAstNode,
        base_slot_expression_detail: WireExpressionResult,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireVisibilitySpecifierResult {
        visibility: u8,
        tokens_consumed: u64,
    }

    #[derive(Clone, Debug)]
    struct WireStateMutabilitySpecifierResult {
        state_mutability: u8,
        tokens_consumed: u64,
    }

    #[derive(Clone, Debug)]
    struct WireFunctionHeaderParserResult {
        is_virtual: bool,
        overrides: WireAstNode,
        override_paths: Vec<WireAstNode>,
        override_path_details: Vec<WireIdentifierPathResult>,
        parameters: WireAstNode,
        parameter_declarations: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        return_parameters: WireAstNode,
        return_parameter_declarations: Vec<WireAstNode>,
        return_parameter_details: Vec<WireVariableDeclarationResult>,
        visibility: u8,
        state_mutability: u8,
        modifiers: Vec<WireAstNode>,
        modifier_details: Vec<WireModifierInvocationResult>,
        experimental_return_expression: WireAstNode,
        experimental_return_expression_detail: WireExpressionResult,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireFunctionDefinitionResult {
        function_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        visibility: u8,
        state_mutability: u8,
        is_free_function: bool,
        kind: u32,
        is_virtual: bool,
        overrides: WireAstNode,
        override_paths: Vec<WireAstNode>,
        override_path_details: Vec<WireIdentifierPathResult>,
        documentation: WireAstNode,
        parameters: WireAstNode,
        parameter_declarations: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        modifiers: Vec<WireAstNode>,
        modifier_details: Vec<WireModifierInvocationResult>,
        return_parameters: WireAstNode,
        return_parameter_declarations: Vec<WireAstNode>,
        return_parameter_details: Vec<WireVariableDeclarationResult>,
        block: WireAstNode,
        block_unchecked: bool,
        block_statements: Vec<WireAstNode>,
        block_statement_details: Vec<WireStatementResult>,
        experimental_return_expression: WireAstNode,
        experimental_return_expression_detail: WireExpressionResult,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
        warnings: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireForAllQuantifierResult {
        for_all_quantifier: WireAstNode,
        type_variable_declarations: WireAstNode,
        type_variable_declaration_parameters: Vec<WireAstNode>,
        type_variable_declaration_details: Vec<WireVariableDeclarationResult>,
        quantified_function: WireAstNode,
        quantified_function_detail: WireFunctionDefinitionResult,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireFunctionCallArguments {
        arguments: Vec<WireAstNode>,
        argument_details: Vec<WireExpressionResult>,
        parameter_names: Vec<WireString>,
        parameter_name_locations: Vec<WireSourceLocation>,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireIdentifierWithLocation {
        identifier: WireString,
        location: WireSourceLocation,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireOptionalString {
        has_value: bool,
        value: WireString,
    }

    #[derive(Clone, Debug)]
    struct WireParserDiagnostic {
        error_id: u32,
        message: String,
        location: WireSourceLocation,
        fatal: bool,
        warning: bool,
    }

    #[derive(Clone, Debug)]
    struct WireLicenseStringResult {
        license: WireOptionalString,
        diagnostics: Vec<WireParserDiagnostic>,
    }

    #[derive(Clone, Debug)]
    struct WireStringAndAdvanceResult {
        value: WireString,
        tokens_consumed: u64,
    }

    #[derive(Clone, Debug)]
    struct WireIdentifierResult {
        value: WireString,
        tokens_consumed: u64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireIdentifierNodeResult {
        identifier: WireAstNode,
        name: WireString,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireIdentifierPathResult {
        identifier_path: WireAstNode,
        path: Vec<WireString>,
        path_locations: Vec<WireSourceLocation>,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireUserDefinedTypeNameResult {
        type_name: WireAstNode,
        path_node: WireAstNode,
        path: Vec<WireString>,
        path_locations: Vec<WireSourceLocation>,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireEnumValueResult {
        enum_value: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireEnumDefinitionResult {
        enum_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        members: Vec<WireAstNode>,
        member_details: Vec<WireEnumValueResult>,
        documentation: WireAstNode,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireStructDefinitionResult {
        struct_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        members: Vec<WireAstNode>,
        member_details: Vec<WireVariableDeclarationResult>,
        documentation: WireAstNode,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireEventDefinitionResult {
        event_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        parameters: WireAstNode,
        parameter_declarations: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        anonymous: bool,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireErrorDefinitionResult {
        error_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        parameters: WireAstNode,
        parameter_declarations: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireUserDefinedValueTypeDefinitionResult {
        user_defined_value_type_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        type_name: WireAstNode,
        type_name_elementary_token: u32,
        type_name_elementary_first_number: u32,
        type_name_elementary_second_number: u32,
        type_name_has_state_mutability: bool,
        type_name_state_mutability: u8,
        type_name_detail: WireTypeNameResult,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireTypeClassDefinitionResult {
        type_class_definition: WireAstNode,
        type_variable: WireAstNode,
        type_variable_name: WireString,
        type_variable_name_location: WireSourceLocation,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        sub_nodes: Vec<WireAstNode>,
        sub_node_function_details: Vec<WireFunctionDefinitionResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireTypeClassInstantiationResult {
        type_class_instantiation: WireAstNode,
        type_constructor: WireAstNode,
        type_constructor_detail: WireTypeNameResult,
        argument_sorts: WireAstNode,
        argument_sort_parameters: Vec<WireAstNode>,
        argument_sort_details: Vec<WireVariableDeclarationResult>,
        type_class_name: WireAstNode,
        type_class_name_detail: WireTypeClassNameResult,
        sub_nodes: Vec<WireAstNode>,
        sub_node_function_details: Vec<WireFunctionDefinitionResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireTypeDefinitionResult {
        type_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        arguments: WireAstNode,
        argument_parameters: Vec<WireAstNode>,
        argument_details: Vec<WireVariableDeclarationResult>,
        expression: WireAstNode,
        expression_detail: WireExpressionResult,
        has_builtin_name_parameter: bool,
        builtin_name_parameter: WireString,
        builtin_name_parameter_location: WireSourceLocation,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireUsingOperator {
        present: bool,
        token: u32,
    }

    #[derive(Clone, Debug)]
    struct WireUsingDirectiveResult {
        using_directive: WireAstNode,
        functions: Vec<WireAstNode>,
        function_details: Vec<WireIdentifierPathResult>,
        operators: Vec<WireUsingOperator>,
        uses_braces: bool,
        type_name: WireAstNode,
        type_name_detail: WireTypeNameResult,
        global: bool,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireModifierDefinitionResult {
        modifier_definition: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        parameters: WireAstNode,
        parameter_declarations: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        is_virtual: bool,
        overrides: WireAstNode,
        override_paths: Vec<WireAstNode>,
        override_path_details: Vec<WireIdentifierPathResult>,
        block: WireAstNode,
        block_unchecked: bool,
        block_statements: Vec<WireAstNode>,
        block_statement_details: Vec<WireStatementResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireVariableDeclarationResult {
        variable_declaration: WireAstNode,
        type_name: WireAstNode,
        type_expression: WireAstNode,
        type_expression_detail: WireExpressionResult,
        documentation: WireAstNode,
        overrides: WireAstNode,
        override_paths: Vec<WireAstNode>,
        override_path_details: Vec<WireIdentifierPathResult>,
        value: WireAstNode,
        value_detail: WireExpressionResult,
        type_name_elementary_token: u32,
        type_name_elementary_first_number: u32,
        type_name_elementary_second_number: u32,
        type_name_has_state_mutability: bool,
        type_name_state_mutability: u8,
        type_name_user_defined_path_node: WireAstNode,
        type_name_user_defined_path: Vec<WireString>,
        type_name_user_defined_path_locations: Vec<WireSourceLocation>,
        type_name_array_base_types: Vec<WireAstNode>,
        type_name_array_lengths: Vec<WireAstNode>,
        type_name_array_length_details: Vec<WireExpressionResult>,
        type_name_function_parameters: WireAstNode,
        type_name_function_parameter_declarations: Vec<WireAstNode>,
        type_name_function_parameter_details: Vec<WireVariableDeclarationResult>,
        type_name_function_return_parameters: WireAstNode,
        type_name_function_return_parameter_declarations: Vec<WireAstNode>,
        type_name_function_return_parameter_details: Vec<WireVariableDeclarationResult>,
        type_name_function_visibility: u8,
        type_name_function_state_mutability: u8,
        type_name_mapping_key_type: WireAstNode,
        type_name_mapping_key_elementary_token: u32,
        type_name_mapping_key_elementary_first_number: u32,
        type_name_mapping_key_elementary_second_number: u32,
        type_name_mapping_key_user_defined_path_node: WireAstNode,
        type_name_mapping_key_user_defined_path: Vec<WireString>,
        type_name_mapping_key_user_defined_path_locations: Vec<WireSourceLocation>,
        type_name_mapping_key_name: WireString,
        type_name_mapping_key_name_location: WireSourceLocation,
        type_name_mapping_value_type: WireAstNode,
        type_name_mapping_value_elementary_token: u32,
        type_name_mapping_value_elementary_first_number: u32,
        type_name_mapping_value_elementary_second_number: u32,
        type_name_mapping_value_has_state_mutability: bool,
        type_name_mapping_value_state_mutability: u8,
        type_name_mapping_value_user_defined_path_node: WireAstNode,
        type_name_mapping_value_user_defined_path: Vec<WireString>,
        type_name_mapping_value_user_defined_path_locations: Vec<WireSourceLocation>,
        type_name_mapping_value_array_base_types: Vec<WireAstNode>,
        type_name_mapping_value_array_lengths: Vec<WireAstNode>,
        type_name_mapping_value_array_length_details: Vec<WireExpressionResult>,
        type_name_mapping_value_function_parameters: WireAstNode,
        type_name_mapping_value_function_parameter_declarations: Vec<WireAstNode>,
        type_name_mapping_value_function_parameter_details: Vec<WireVariableDeclarationResult>,
        type_name_mapping_value_function_return_parameters: WireAstNode,
        type_name_mapping_value_function_return_parameter_declarations: Vec<WireAstNode>,
        type_name_mapping_value_function_return_parameter_details:
            Vec<WireVariableDeclarationResult>,
        type_name_mapping_value_function_visibility: u8,
        type_name_mapping_value_function_state_mutability: u8,
        type_name_mapping_value_name: WireString,
        type_name_mapping_value_name_location: WireSourceLocation,
        type_name_mapping_details: Vec<WireMappingTypeName>,
        name: WireString,
        name_location: WireSourceLocation,
        visibility: u8,
        mutability: u8,
        variable_location: u8,
        indexed: bool,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WirePostfixVariableDeclarationResult {
        variable_declaration: WireAstNode,
        name: WireString,
        name_location: WireSourceLocation,
        documentation: WireAstNode,
        type_expression: WireAstNode,
        type_expression_detail: WireExpressionResult,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireMappingTypeName {
        mapping: WireAstNode,
        key_type: WireAstNode,
        key_type_elementary_token: u32,
        key_type_elementary_first_number: u32,
        key_type_elementary_second_number: u32,
        key_type_user_defined_path_node: WireAstNode,
        key_type_user_defined_path: Vec<WireString>,
        key_type_user_defined_path_locations: Vec<WireSourceLocation>,
        key_name: WireString,
        key_name_location: WireSourceLocation,
        value_type: WireAstNode,
        value_type_elementary_token: u32,
        value_type_elementary_first_number: u32,
        value_type_elementary_second_number: u32,
        value_type_has_state_mutability: bool,
        value_type_state_mutability: u8,
        value_type_user_defined_path_node: WireAstNode,
        value_type_user_defined_path: Vec<WireString>,
        value_type_user_defined_path_locations: Vec<WireSourceLocation>,
        value_type_array_base_types: Vec<WireAstNode>,
        value_type_array_lengths: Vec<WireAstNode>,
        value_type_array_length_details: Vec<WireExpressionResult>,
        value_type_function_parameters: WireAstNode,
        value_type_function_parameter_declarations: Vec<WireAstNode>,
        value_type_function_parameter_details: Vec<WireVariableDeclarationResult>,
        value_type_function_return_parameters: WireAstNode,
        value_type_function_return_parameter_declarations: Vec<WireAstNode>,
        value_type_function_return_parameter_details: Vec<WireVariableDeclarationResult>,
        value_type_function_visibility: u8,
        value_type_function_state_mutability: u8,
        value_name: WireString,
        value_name_location: WireSourceLocation,
    }

    #[derive(Clone, Debug)]
    struct WireTypeNameResult {
        type_name: WireAstNode,
        array_base_type: WireAstNode,
        array_length: WireAstNode,
        array_base_types: Vec<WireAstNode>,
        array_lengths: Vec<WireAstNode>,
        array_length_details: Vec<WireExpressionResult>,
        elementary_type_token: u32,
        elementary_type_first_number: u32,
        elementary_type_second_number: u32,
        has_state_mutability: bool,
        state_mutability: u8,
        user_defined_path_node: WireAstNode,
        user_defined_path: Vec<WireString>,
        user_defined_path_locations: Vec<WireSourceLocation>,
        function_parameters: WireAstNode,
        function_parameter_declarations: Vec<WireAstNode>,
        function_parameter_details: Vec<WireVariableDeclarationResult>,
        function_return_parameters: WireAstNode,
        function_return_parameter_declarations: Vec<WireAstNode>,
        function_return_parameter_details: Vec<WireVariableDeclarationResult>,
        function_visibility: u8,
        function_state_mutability: u8,
        mapping_key_type: WireAstNode,
        mapping_key_elementary_token: u32,
        mapping_key_elementary_first_number: u32,
        mapping_key_elementary_second_number: u32,
        mapping_key_user_defined_path_node: WireAstNode,
        mapping_key_user_defined_path: Vec<WireString>,
        mapping_key_user_defined_path_locations: Vec<WireSourceLocation>,
        mapping_key_name: WireString,
        mapping_key_name_location: WireSourceLocation,
        mapping_value_type: WireAstNode,
        mapping_value_elementary_token: u32,
        mapping_value_elementary_first_number: u32,
        mapping_value_elementary_second_number: u32,
        mapping_value_has_state_mutability: bool,
        mapping_value_state_mutability: u8,
        mapping_value_user_defined_path_node: WireAstNode,
        mapping_value_user_defined_path: Vec<WireString>,
        mapping_value_user_defined_path_locations: Vec<WireSourceLocation>,
        mapping_value_array_base_types: Vec<WireAstNode>,
        mapping_value_array_lengths: Vec<WireAstNode>,
        mapping_value_array_length_details: Vec<WireExpressionResult>,
        mapping_value_function_parameters: WireAstNode,
        mapping_value_function_parameter_declarations: Vec<WireAstNode>,
        mapping_value_function_parameter_details: Vec<WireVariableDeclarationResult>,
        mapping_value_function_return_parameters: WireAstNode,
        mapping_value_function_return_parameter_declarations: Vec<WireAstNode>,
        mapping_value_function_return_parameter_details: Vec<WireVariableDeclarationResult>,
        mapping_value_function_visibility: u8,
        mapping_value_function_state_mutability: u8,
        mapping_value_name: WireString,
        mapping_value_name_location: WireSourceLocation,
        mapping_details: Vec<WireMappingTypeName>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireMappingResult {
        mapping: WireAstNode,
        key_type: WireAstNode,
        key_type_elementary_token: u32,
        key_type_elementary_first_number: u32,
        key_type_elementary_second_number: u32,
        key_type_user_defined_path_node: WireAstNode,
        key_type_user_defined_path: Vec<WireString>,
        key_type_user_defined_path_locations: Vec<WireSourceLocation>,
        key_name: WireString,
        key_name_location: WireSourceLocation,
        value_type: WireAstNode,
        value_type_elementary_token: u32,
        value_type_elementary_first_number: u32,
        value_type_elementary_second_number: u32,
        value_type_has_state_mutability: bool,
        value_type_state_mutability: u8,
        value_type_user_defined_path_node: WireAstNode,
        value_type_user_defined_path: Vec<WireString>,
        value_type_user_defined_path_locations: Vec<WireSourceLocation>,
        value_type_array_base_types: Vec<WireAstNode>,
        value_type_array_lengths: Vec<WireAstNode>,
        value_type_array_length_details: Vec<WireExpressionResult>,
        value_type_function_parameters: WireAstNode,
        value_type_function_parameter_declarations: Vec<WireAstNode>,
        value_type_function_parameter_details: Vec<WireVariableDeclarationResult>,
        value_type_function_return_parameters: WireAstNode,
        value_type_function_return_parameter_declarations: Vec<WireAstNode>,
        value_type_function_return_parameter_details: Vec<WireVariableDeclarationResult>,
        value_type_function_visibility: u8,
        value_type_function_state_mutability: u8,
        value_name: WireString,
        value_name_location: WireSourceLocation,
        mapping_details: Vec<WireMappingTypeName>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireFunctionTypeResult {
        function_type: WireAstNode,
        parameters: WireAstNode,
        parameter_declarations: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        return_parameters: WireAstNode,
        return_parameter_declarations: Vec<WireAstNode>,
        return_parameter_details: Vec<WireVariableDeclarationResult>,
        visibility: u8,
        state_mutability: u8,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireParameterListParseResult {
        parameter_list: WireAstNode,
        parameters: Vec<WireAstNode>,
        parameter_details: Vec<WireVariableDeclarationResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireBlockResult {
        block: WireAstNode,
        unchecked: bool,
        statements: Vec<WireAstNode>,
        statement_details: Vec<WireStatementResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireInlineAssemblyResult {
        inline_assembly: WireAstNode,
        flags: Vec<WireString>,
        block_location: WireSourceLocation,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireStatementResult {
        statement: WireAstNode,
        block_unchecked: bool,
        block_statements: Vec<WireAstNode>,
        block_statement_details: Vec<WireStatementResult>,
        inline_assembly_flags: Vec<WireString>,
        inline_assembly_block_location: WireSourceLocation,
        condition_expression: WireAstNode,
        condition_expression_detail: Vec<WireExpressionResult>,
        true_body: WireAstNode,
        true_body_detail: Vec<WireStatementResult>,
        false_body: WireAstNode,
        false_body_detail: Vec<WireStatementResult>,
        body: WireAstNode,
        body_detail: Vec<WireStatementResult>,
        is_do_while: bool,
        external_call: WireAstNode,
        external_call_detail: Vec<WireExpressionResult>,
        clauses: Vec<WireAstNode>,
        clause_details: Vec<WireTryCatchClauseResult>,
        clause_block_statement_details: Vec<WireStatementResult>,
        clause_error_names: Vec<WireString>,
        clause_error_parameters: Vec<WireAstNode>,
        clause_blocks: Vec<WireAstNode>,
        init_expression: WireAstNode,
        init_expression_detail: Vec<WireStatementResult>,
        loop_expression: WireAstNode,
        loop_expression_detail: Vec<WireStatementResult>,
        event_call: WireAstNode,
        event_call_callee: WireAstNode,
        event_call_callee_detail: Vec<WireExpressionResult>,
        event_call_arguments: Vec<WireAstNode>,
        event_call_argument_details: Vec<WireExpressionResult>,
        event_call_parameter_names: Vec<WireString>,
        event_call_parameter_name_locations: Vec<WireSourceLocation>,
        error_call: WireAstNode,
        error_call_callee: WireAstNode,
        error_call_callee_detail: Vec<WireExpressionResult>,
        error_call_arguments: Vec<WireAstNode>,
        error_call_argument_details: Vec<WireExpressionResult>,
        error_call_parameter_names: Vec<WireString>,
        error_call_parameter_name_locations: Vec<WireSourceLocation>,
        expression: WireAstNode,
        expression_detail: Vec<WireExpressionResult>,
        variables: Vec<WireAstNode>,
        variable_details: Vec<WireVariableDeclarationResult>,
        initial_value: WireAstNode,
        initial_value_detail: Vec<WireExpressionResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireTryCatchClauseResult {
        try_catch_clause: WireAstNode,
        error_name: WireString,
        error_parameters: WireAstNode,
        error_parameter_declarations: Vec<WireAstNode>,
        error_parameter_details: Vec<WireVariableDeclarationResult>,
        block: WireAstNode,
        block_unchecked: bool,
        block_statements: Vec<WireAstNode>,
        block_statement_details: Vec<WireStatementResult>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WirePathFunctionCallResult {
        function_call: WireAstNode,
        callee: WireAstNode,
        callee_detail: WireExpressionResult,
        arguments: Vec<WireAstNode>,
        argument_details: Vec<WireExpressionResult>,
        parameter_names: Vec<WireString>,
        parameter_name_locations: Vec<WireSourceLocation>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireExpressionResult {
        expression: WireAstNode,
        left_expression: WireAstNode,
        left_expression_detail: Vec<WireExpressionResult>,
        right_expression: WireAstNode,
        right_expression_detail: Vec<WireExpressionResult>,
        condition_expression: WireAstNode,
        condition_expression_detail: Vec<WireExpressionResult>,
        true_expression: WireAstNode,
        true_expression_detail: Vec<WireExpressionResult>,
        false_expression: WireAstNode,
        false_expression_detail: Vec<WireExpressionResult>,
        sub_expression: WireAstNode,
        sub_expression_detail: Vec<WireExpressionResult>,
        is_prefix_operation: bool,
        base_expression: WireAstNode,
        base_expression_detail: Vec<WireExpressionResult>,
        base_expression_type: WireAstNode,
        index_expression: WireAstNode,
        index_expression_detail: Vec<WireExpressionResult>,
        end_index_expression: WireAstNode,
        end_index_expression_detail: Vec<WireExpressionResult>,
        type_name: WireAstNode,
        type_name_details: Vec<WireTypeNameResult>,
        expression_type: WireAstNode,
        member_name_location: WireSourceLocation,
        arguments: Vec<WireAstNode>,
        argument_details: Vec<WireExpressionResult>,
        parameter_names: Vec<WireString>,
        parameter_name_locations: Vec<WireSourceLocation>,
        components: Vec<WireAstNode>,
        component_details: Vec<WireExpressionResult>,
        is_inline_array: bool,
        literal_token: u32,
        literal_subdenomination: u32,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireParameterListResult {
        parameter_list: WireAstNode,
        parameters: Vec<WireAstNode>,
        current_node_id: i64,
    }

    #[derive(Clone, Debug)]
    struct WireOverrideSpecifierResult {
        override_specifier: WireAstNode,
        overrides: Vec<WireAstNode>,
        override_details: Vec<WireIdentifierPathResult>,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireTypeClassNameResult {
        type_class_name: WireAstNode,
        is_builtin: bool,
        builtin_token: u32,
        identifier_path: WireAstNode,
        identifier_path_detail: WireIdentifierPathResult,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireInheritanceSpecifierResult {
        inheritance_specifier: WireAstNode,
        base_name: WireAstNode,
        base_name_path: Vec<WireString>,
        base_name_path_locations: Vec<WireSourceLocation>,
        has_arguments: bool,
        arguments: Vec<WireAstNode>,
        argument_details: Vec<WireExpressionResult>,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireModifierInvocationResult {
        modifier_invocation: WireAstNode,
        modifier_name: WireAstNode,
        modifier_name_detail: WireIdentifierPathResult,
        has_arguments: bool,
        arguments: Vec<WireAstNode>,
        argument_details: Vec<WireExpressionResult>,
        tokens_consumed: u64,
        current_node_id: i64,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireIndexAccess {
        start: WireAstNode,
        start_detail: WireExpressionResult,
        has_end: bool,
        end: WireAstNode,
        end_detail: WireExpressionResult,
        location: WireSourceLocation,
    }

    #[derive(Clone, Debug)]
    struct WireIndexAccessedPath {
        path: Vec<WireAstNode>,
        path_expression_types: Vec<WireAstNode>,
        indices: Vec<WireIndexAccess>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireLookAheadResult {
        kind: u8,
        path: WireIndexAccessedPath,
    }

    #[derive(Clone, Debug)]
    struct WireTypeNameFromIndexAccessStructureResult {
        type_name: WireAstNode,
        array_base_type: WireAstNode,
        array_length: WireAstNode,
        array_base_types: Vec<WireAstNode>,
        array_lengths: Vec<WireAstNode>,
        array_length_details: Vec<WireExpressionResult>,
        elementary_type_token: u32,
        elementary_type_first_number: u32,
        elementary_type_second_number: u32,
        has_state_mutability: bool,
        state_mutability: u8,
        user_defined_path_node: WireAstNode,
        user_defined_path: Vec<WireString>,
        user_defined_path_locations: Vec<WireSourceLocation>,
        errors: Vec<WireParserError>,
    }

    #[derive(Clone, Debug)]
    struct WireDocTag {
        name: WireString,
        content: WireString,
        param_name: WireString,
    }

    #[derive(Clone, Debug)]
    struct WireDocStringError {
        error_id: u32,
        location: WireSourceLocation,
        message: WireString,
    }

    #[derive(Clone, Debug)]
    struct WireDocStringResult {
        tags: Vec<WireDocTag>,
        errors: Vec<WireDocStringError>,
    }

    extern "Rust" {
        type CompactParserHandle;

        fn parse() -> WireParserResult;
        fn parse_compact() -> Box<CompactParserHandle>;
        fn parse_compact_with_legacy() -> Box<CompactParserHandle>;
        fn compact_parser_legacy_wire_result(handle: &CompactParserHandle) -> &WireParserResult;
        fn compact_parser_arena(handle: &CompactParserHandle) -> &WireCompactParseOutput;
        fn max_id(current_node_id: i64) -> i64;
        fn next_id(current_node_id: i64) -> i64;
        fn reset_parser_input();
        fn set_parser_input(
            tokens: Vec<WireLocatedToken>,
            source: WireString,
            comment_literals: Vec<WireString>,
            comment_locations: Vec<WireSourceLocation>,
            current_node_id: i64,
            current_compiler_version: String,
            evm_version_at_least_constantinople: bool,
        );
        fn set_parser_input_with_evm_version(
            tokens: Vec<WireLocatedToken>,
            source: WireString,
            comment_literals: Vec<WireString>,
            comment_locations: Vec<WireSourceLocation>,
            current_node_id: i64,
            current_compiler_version: String,
            evm_version_name: String,
        );
        fn set_parser_source_input_with_evm_version(
            source: WireString,
            current_node_id: i64,
            current_compiler_version: String,
            evm_version_name: String,
        );

        fn parse_pragma_version(
            location: WireSourceLocation,
            tokens: Vec<u32>,
            literals: Vec<WireString>,
            current_version: String,
        ) -> WirePragmaVersionResult;
        fn parse_structured_documentation(
            comment_literal: WireString,
            comment_location: WireSourceLocation,
            current_node_id: i64,
        ) -> WireStructuredDocumentationResult;
        fn parse_pragma_directive(
            finished_parsing_top_level_pragmas: bool,
        ) -> WirePragmaDirectiveResult;
        fn parse_import_directive() -> WireImportDirectiveResult;
        fn parse_contract_kind(current_token: u32, next_token: u32) -> WireContractKindResult;
        fn parse_contract_definition() -> WireContractDefinitionResult;
        fn parse_inheritance_specifier(
            tokens: Vec<WireLocatedToken>,
            current_node_id: i64,
            arguments: Vec<WireAstNode>,
            argument_tokens_consumed: u64,
        ) -> WireInheritanceSpecifierResult;
        fn parse_visibility_specifier(token: u32) -> WireVisibilitySpecifierResult;
        fn parse_override_specifier(
            tokens: Vec<WireLocatedToken>,
            current_node_id: i64,
        ) -> WireOverrideSpecifierResult;
        fn parse_state_mutability(token: u32) -> WireStateMutabilitySpecifierResult;
        fn parse_function_header(is_state_variable: bool) -> WireFunctionHeaderParserResult;
        fn parse_quantified_function_definition() -> WireForAllQuantifierResult;
        fn parse_function_definition(
            free_function: bool,
            allow_body: bool,
        ) -> WireFunctionDefinitionResult;
        fn parse_struct_definition() -> WireStructDefinitionResult;
        fn parse_enum_definition() -> WireEnumDefinitionResult;
        fn parse_user_defined_value_type_definition() -> WireUserDefinedValueTypeDefinitionResult;
        fn parse_enum_value(
            comment_literal: WireString,
            comment_location: WireSourceLocation,
            token: u32,
            literal: WireString,
            token_name: String,
            token_location: WireSourceLocation,
            current_node_id: i64,
        ) -> WireEnumValueResult;
        fn parse_variable_declaration() -> WireVariableDeclarationResult;
        fn parse_modifier_definition() -> WireModifierDefinitionResult;
        fn parse_event_definition() -> WireEventDefinitionResult;
        fn parse_error_definition() -> WireErrorDefinitionResult;
        fn parse_using_directive() -> WireUsingDirectiveResult;
        fn parse_modifier_invocation(
            tokens: Vec<WireLocatedToken>,
            current_node_id: i64,
            arguments: Vec<WireAstNode>,
            argument_tokens_consumed: u64,
        ) -> WireModifierInvocationResult;
        fn parse_identifier(
            token: u32,
            literal: WireString,
            token_name: String,
            location: WireSourceLocation,
            current_node_id: i64,
        ) -> WireIdentifierNodeResult;
        fn parse_identifier_or_address(
            token: u32,
            literal: WireString,
            token_name: String,
            location: WireSourceLocation,
            current_node_id: i64,
        ) -> WireIdentifierNodeResult;
        fn parse_user_defined_type_name(
            tokens: Vec<WireLocatedToken>,
            current_node_id: i64,
        ) -> WireUserDefinedTypeNameResult;
        fn parse_identifier_path(
            tokens: Vec<WireLocatedToken>,
            current_node_id: i64,
        ) -> WireIdentifierPathResult;
        fn parse_type_name_suffix() -> WireTypeNameResult;
        fn parse_type_name() -> WireTypeNameResult;
        fn parse_function_type() -> WireFunctionTypeResult;
        fn parse_mapping() -> WireMappingResult;
        fn parse_parameter_list() -> WireParameterListParseResult;
        fn parse_block() -> WireBlockResult;
        fn parse_statement() -> WireStatementResult;
        fn parse_inline_assembly() -> WireInlineAssemblyResult;
        fn parse_if_statement() -> WireStatementResult;
        fn parse_try_statement() -> WireStatementResult;
        fn parse_catch_clause() -> WireTryCatchClauseResult;
        fn parse_while_statement() -> WireStatementResult;
        fn parse_do_while_statement() -> WireStatementResult;
        fn parse_for_statement() -> WireStatementResult;
        fn parse_emit_statement() -> WireStatementResult;
        fn parse_revert_statement() -> WireStatementResult;
        fn parse_simple_statement() -> WireStatementResult;
        fn parse_variable_declaration_statement() -> WireStatementResult;
        fn parse_expression_statement() -> WireStatementResult;
        fn parse_expression() -> WireExpressionResult;
        fn parse_binary_expression() -> WireExpressionResult;
        fn parse_unary_expression() -> WireExpressionResult;
        fn parse_left_hand_side_expression() -> WireExpressionResult;
        fn parse_literal() -> WireExpressionResult;
        fn parse_primary_expression() -> WireExpressionResult;
        fn parse_function_call_list_arguments() -> WireFunctionCallArguments;
        fn parse_function_call_arguments() -> WireFunctionCallArguments;
        fn parse_named_arguments() -> WireFunctionCallArguments;
        fn expect_identifier_with_location(
            token: u32,
            literal: WireString,
            token_name: String,
            location: WireSourceLocation,
        ) -> WireIdentifierWithLocation;
        fn parse_storage_layout_specifier() -> WireStorageLayoutSpecifierResult;

        fn parse_postfix_variable_declaration_statement() -> WireStatementResult;
        fn parse_postfix_variable_declaration() -> WirePostfixVariableDeclarationResult;
        fn parse_type_class_definition() -> WireTypeClassDefinitionResult;
        fn parse_type_class_instantiation() -> WireTypeClassInstantiationResult;
        fn parse_type_definition() -> WireTypeDefinitionResult;
        fn parse_type_class_name(
            tokens: Vec<WireLocatedToken>,
            current_node_id: i64,
        ) -> WireTypeClassNameResult;

        fn variable_declaration_start(current_token: u32, next_token: u32) -> bool;
        fn find_license_string(
            source: WireString,
            nodes: Vec<WireAstNode>,
            source_id: i64,
        ) -> WireLicenseStringResult;
        fn try_parse_index_accessed_path(
            current_token: u32,
            next_token: u32,
            experimental_solidity_enabled: bool,
            parsed_path: WireIndexAccessedPath,
            token_after_path: u32,
        ) -> WireLookAheadResult;
        fn peek_statement_type(current_token: u32, next_token: u32) -> u8;
        fn parse_index_accessed_path() -> WireIndexAccessedPath;
        fn index_accessed_path_empty(path: WireIndexAccessedPath) -> bool;
        fn type_name_from_index_access_structure(
            path_and_indices: WireIndexAccessedPath,
        ) -> WireTypeNameFromIndexAccessStructureResult;
        fn expression_from_index_access_structure(
            path_and_indices: WireIndexAccessedPath,
        ) -> WireExpressionResult;
        fn expect_identifier_token(
            token: u32,
            literal: WireString,
            token_name: String,
        ) -> WireIdentifierResult;
        fn expect_identifier_token_or_address(
            token: u32,
            literal: WireString,
            token_name: String,
        ) -> WireIdentifierResult;
        fn get_literal_and_advance(literal: WireString) -> WireStringAndAdvanceResult;
        fn is_quoted_path(token: u32) -> bool;
        fn is_stdlib_path(
            token: u32,
            literal: WireString,
            experimental_solidity_enabled: bool,
        ) -> bool;
        fn token_precedence(token: u32, experimental_solidity_enabled: bool) -> i32;
        fn get_stdlib_import_path_and_advance(
            current: WireToken,
            after_current: WireToken,
            after_period: WireToken,
        ) -> WireIdentifierResult;
        fn create_empty_parameter_list(
            current_location: WireSourceLocation,
            current_node_id: i64,
        ) -> WireParameterListResult;

        fn parse_doc_string(text: WireString, location: WireSourceLocation) -> WireDocStringResult;
    }
}

pub use crate::compact::{
    compact_parser_arena, compact_parser_legacy_wire_result, parse_compact,
    parse_compact_with_legacy, CompactParserHandle,
};
pub use crate::doc_string_parser::parse_doc_string;
pub use crate::parser::*;
