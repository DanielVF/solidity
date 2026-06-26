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
        condition_expression_detail: WireExpressionResult,
        true_body: WireAstNode,
        true_body_detail: Vec<WireStatementResult>,
        false_body: WireAstNode,
        false_body_detail: Vec<WireStatementResult>,
        body: WireAstNode,
        body_detail: Vec<WireStatementResult>,
        is_do_while: bool,
        external_call: WireAstNode,
        external_call_detail: WireExpressionResult,
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
        event_call_callee_detail: WireExpressionResult,
        event_call_arguments: Vec<WireAstNode>,
        event_call_argument_details: Vec<WireExpressionResult>,
        event_call_parameter_names: Vec<WireString>,
        event_call_parameter_name_locations: Vec<WireSourceLocation>,
        error_call: WireAstNode,
        error_call_callee: WireAstNode,
        error_call_callee_detail: WireExpressionResult,
        error_call_arguments: Vec<WireAstNode>,
        error_call_argument_details: Vec<WireExpressionResult>,
        error_call_parameter_names: Vec<WireString>,
        error_call_parameter_name_locations: Vec<WireSourceLocation>,
        expression: WireAstNode,
        expression_detail: WireExpressionResult,
        variables: Vec<WireAstNode>,
        variable_details: Vec<WireVariableDeclarationResult>,
        initial_value: WireAstNode,
        initial_value_detail: WireExpressionResult,
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
        type_name_detail: WireTypeNameResult,
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
        fn parse() -> WireParserResult;
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

pub use crate::doc_string_parser::parse_doc_string;
pub use crate::parser::*;
