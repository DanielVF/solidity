/*
	This file is part of solidity.

	solidity is free software: you can redistribute it and/or modify
	it under the terms of the GNU General Public License as published by
	the Free Software Foundation, either version 3 of the License, or
	(at your option) any later version.

	solidity is distributed in the hope that it will be useful,
	but WITHOUT ANY WARRANTY; without even the implied warranty of
	MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
	GNU General Public License for more details.

	You should have received a copy of the GNU General Public License
	along with solidity.  If not, see <http://www.gnu.org/licenses/>.
*/
// SPDX-License-Identifier: GPL-3.0

#pragma once

#include <libsolidity/ast/ASTForward.h>

#include <liblangutil/EVMVersion.h>
#include <liblangutil/SourceLocation.h>
#include <liblangutil/Token.h>

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace solidity::langutil
{
class CharStream;
class ErrorReporter;
}

namespace solidity::frontend
{

#if defined(SOLIDITY_USE_RUST_SOLIDITY_PARSER)

enum class RustParserErrorCode: std::uint8_t
{
	None = 0,
	Parse = 1,
	Unknown = 255,
};

struct RustParserAstNode
{
	bool present = false;
	std::int64_t nodeID = 0;
	std::uint8_t kind = 0;
	langutil::SourceLocation location;
	std::string text;
};

struct RustParserDiagnostic
{
	std::uint32_t errorID = 0;
	std::string message;
	langutil::SourceLocation location;
	std::vector<std::pair<std::string, langutil::SourceLocation>> secondaryLocations;
	bool syntax = false;
	bool fatal = false;
};

struct RustParserPragmaDirective
{
	RustParserAstNode node;
	std::vector<langutil::Token> tokens;
	std::vector<std::string> literals;
};

struct RustParserImportSymbolAlias
{
	RustParserAstNode symbol;
	bool hasAlias = false;
	std::string alias;
	langutil::SourceLocation location;
};

struct RustParserImportDirective
{
	RustParserAstNode node;
	std::string path;
	std::string unitAlias;
	langutil::SourceLocation unitAliasLocation;
	std::vector<RustParserImportSymbolAlias> symbolAliases;
};

struct RustParserEnumValue
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode documentation;
};

struct RustParserEnumDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	std::vector<RustParserEnumValue> members;
	RustParserAstNode documentation;
};

struct RustParserIdentifierPath
{
	RustParserAstNode node;
	std::vector<std::string> path;
	std::vector<langutil::SourceLocation> pathLocations;
};

struct RustParserTypeName;
struct RustParserExpression;
struct RustParserVariableDeclaration;

struct RustParserMappingTypeName
{
	RustParserAstNode mapping;
	RustParserAstNode keyType;
	std::uint32_t keyElementaryToken = 0;
	std::uint32_t keyElementaryFirstNumber = 0;
	std::uint32_t keyElementarySecondNumber = 0;
	RustParserAstNode keyUserDefinedPathNode;
	std::vector<std::string> keyUserDefinedPath;
	std::vector<langutil::SourceLocation> keyUserDefinedPathLocations;
	std::string keyName;
	langutil::SourceLocation keyNameLocation;
	RustParserAstNode valueType;
	std::uint32_t valueElementaryToken = 0;
	std::uint32_t valueElementaryFirstNumber = 0;
	std::uint32_t valueElementarySecondNumber = 0;
	bool valueHasStateMutability = false;
	std::uint8_t valueStateMutability = 0;
	RustParserAstNode valueUserDefinedPathNode;
	std::vector<std::string> valueUserDefinedPath;
	std::vector<langutil::SourceLocation> valueUserDefinedPathLocations;
	std::vector<RustParserAstNode> valueArrayBaseTypes;
	std::vector<RustParserAstNode> valueArrayLengths;
	std::vector<RustParserExpression> valueArrayLengthDetails;
	RustParserAstNode valueFunctionParameters;
	std::vector<RustParserAstNode> valueFunctionParameterDeclarations;
	std::vector<RustParserVariableDeclaration> valueFunctionParameterDetails;
	RustParserAstNode valueFunctionReturnParameters;
	std::vector<RustParserAstNode> valueFunctionReturnParameterDeclarations;
	std::vector<RustParserVariableDeclaration> valueFunctionReturnParameterDetails;
	std::uint8_t valueFunctionVisibility = 0;
	std::uint8_t valueFunctionStateMutability = 0;
	std::string valueName;
	langutil::SourceLocation valueNameLocation;
};

struct RustParserExpression
{
	RustParserAstNode node;
	RustParserAstNode leftExpression;
	std::vector<RustParserExpression> leftExpressionDetail;
	RustParserAstNode rightExpression;
	std::vector<RustParserExpression> rightExpressionDetail;
	RustParserAstNode conditionExpression;
	std::vector<RustParserExpression> conditionExpressionDetail;
	RustParserAstNode trueExpression;
	std::vector<RustParserExpression> trueExpressionDetail;
	RustParserAstNode falseExpression;
	std::vector<RustParserExpression> falseExpressionDetail;
	RustParserAstNode subExpression;
	std::vector<RustParserExpression> subExpressionDetail;
	bool isPrefixOperation = false;
	RustParserAstNode baseExpression;
	std::vector<RustParserExpression> baseExpressionDetail;
	RustParserAstNode indexExpression;
	std::vector<RustParserExpression> indexExpressionDetail;
	RustParserAstNode endIndexExpression;
	std::vector<RustParserExpression> endIndexExpressionDetail;
	RustParserAstNode typeName;
	std::shared_ptr<RustParserTypeName> typeNameDetail;
	RustParserAstNode expressionType;
	std::vector<RustParserAstNode> arguments;
	std::vector<RustParserExpression> argumentDetails;
	std::vector<std::string> parameterNames;
	std::vector<langutil::SourceLocation> parameterNameLocations;
	langutil::SourceLocation memberNameLocation;
	std::vector<RustParserAstNode> components;
	std::vector<RustParserExpression> componentDetails;
	bool isInlineArray = false;
	std::uint32_t literalToken = 0;
	std::uint32_t literalSubdenomination = 0;
};

struct RustParserVariableDeclaration
{
	RustParserAstNode node;
	RustParserAstNode typeName;
	RustParserAstNode typeExpression;
	RustParserExpression typeExpressionDetail;
	RustParserAstNode documentation;
	RustParserAstNode overrides;
	std::vector<RustParserAstNode> overridePaths;
	std::vector<RustParserIdentifierPath> overridePathDetails;
	RustParserAstNode value;
	RustParserExpression valueDetail;
	std::uint32_t typeNameElementaryToken = 0;
	std::uint32_t typeNameElementaryFirstNumber = 0;
	std::uint32_t typeNameElementarySecondNumber = 0;
	bool typeNameHasStateMutability = false;
	std::uint8_t typeNameStateMutability = 0;
	RustParserAstNode typeNameUserDefinedPathNode;
	std::vector<std::string> typeNameUserDefinedPath;
	std::vector<langutil::SourceLocation> typeNameUserDefinedPathLocations;
	std::vector<RustParserAstNode> typeNameArrayBaseTypes;
	std::vector<RustParserAstNode> typeNameArrayLengths;
	std::vector<RustParserExpression> typeNameArrayLengthDetails;
	RustParserAstNode typeNameFunctionParameters;
	std::vector<RustParserAstNode> typeNameFunctionParameterDeclarations;
	std::vector<RustParserVariableDeclaration> typeNameFunctionParameterDetails;
	RustParserAstNode typeNameFunctionReturnParameters;
	std::vector<RustParserAstNode> typeNameFunctionReturnParameterDeclarations;
	std::vector<RustParserVariableDeclaration> typeNameFunctionReturnParameterDetails;
	std::uint8_t typeNameFunctionVisibility = 0;
	std::uint8_t typeNameFunctionStateMutability = 0;
	RustParserAstNode typeNameMappingKeyType;
	std::uint32_t typeNameMappingKeyElementaryToken = 0;
	std::uint32_t typeNameMappingKeyElementaryFirstNumber = 0;
	std::uint32_t typeNameMappingKeyElementarySecondNumber = 0;
	RustParserAstNode typeNameMappingKeyUserDefinedPathNode;
	std::vector<std::string> typeNameMappingKeyUserDefinedPath;
	std::vector<langutil::SourceLocation> typeNameMappingKeyUserDefinedPathLocations;
	std::string typeNameMappingKeyName;
	langutil::SourceLocation typeNameMappingKeyNameLocation;
	RustParserAstNode typeNameMappingValueType;
	std::uint32_t typeNameMappingValueElementaryToken = 0;
	std::uint32_t typeNameMappingValueElementaryFirstNumber = 0;
	std::uint32_t typeNameMappingValueElementarySecondNumber = 0;
	bool typeNameMappingValueHasStateMutability = false;
	std::uint8_t typeNameMappingValueStateMutability = 0;
	RustParserAstNode typeNameMappingValueUserDefinedPathNode;
	std::vector<std::string> typeNameMappingValueUserDefinedPath;
	std::vector<langutil::SourceLocation> typeNameMappingValueUserDefinedPathLocations;
	std::vector<RustParserAstNode> typeNameMappingValueArrayBaseTypes;
	std::vector<RustParserAstNode> typeNameMappingValueArrayLengths;
	std::vector<RustParserExpression> typeNameMappingValueArrayLengthDetails;
	RustParserAstNode typeNameMappingValueFunctionParameters;
	std::vector<RustParserAstNode> typeNameMappingValueFunctionParameterDeclarations;
	std::vector<RustParserVariableDeclaration> typeNameMappingValueFunctionParameterDetails;
	RustParserAstNode typeNameMappingValueFunctionReturnParameters;
	std::vector<RustParserAstNode> typeNameMappingValueFunctionReturnParameterDeclarations;
	std::vector<RustParserVariableDeclaration> typeNameMappingValueFunctionReturnParameterDetails;
	std::uint8_t typeNameMappingValueFunctionVisibility = 0;
	std::uint8_t typeNameMappingValueFunctionStateMutability = 0;
	std::string typeNameMappingValueName;
	langutil::SourceLocation typeNameMappingValueNameLocation;
	std::vector<RustParserMappingTypeName> typeNameMappingDetails;
	std::string name;
	langutil::SourceLocation nameLocation;
	std::uint8_t visibility = 0;
	std::uint8_t mutability = 0;
	std::uint8_t variableLocation = 0;
	bool indexed = false;
};

struct RustParserStructDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	std::vector<RustParserAstNode> members;
	std::vector<RustParserVariableDeclaration> memberDetails;
	RustParserAstNode documentation;
};

struct RustParserEventDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode documentation;
	RustParserAstNode parameters;
	std::vector<RustParserAstNode> parameterDeclarations;
	std::vector<RustParserVariableDeclaration> parameterDetails;
	bool anonymous = false;
};

struct RustParserErrorDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode documentation;
	RustParserAstNode parameters;
	std::vector<RustParserAstNode> parameterDeclarations;
	std::vector<RustParserVariableDeclaration> parameterDetails;
};

struct RustParserModifierInvocation
{
	RustParserAstNode node;
	RustParserAstNode modifierName;
	RustParserIdentifierPath modifierNameDetail;
	bool hasArguments = false;
	std::vector<RustParserAstNode> arguments;
	std::vector<RustParserExpression> argumentDetails;
};

struct RustParserTryCatchClause
{
	RustParserAstNode node;
	std::string errorName;
	RustParserAstNode errorParameters;
	std::vector<RustParserAstNode> errorParameterDeclarations;
	std::vector<RustParserVariableDeclaration> errorParameterDetails;
	RustParserAstNode block;
	bool blockUnchecked = false;
	std::vector<RustParserAstNode> blockStatements;
};

struct RustParserStatement
{
	RustParserAstNode node;
	bool blockUnchecked = false;
	std::vector<RustParserAstNode> blockStatements;
	std::vector<RustParserStatement> blockStatementDetails;
	std::vector<std::string> inlineAssemblyFlags;
	langutil::SourceLocation inlineAssemblyBlockLocation;
	RustParserAstNode conditionExpression;
	RustParserExpression conditionExpressionDetail;
	RustParserAstNode trueBody;
	std::vector<RustParserStatement> trueBodyDetail;
	RustParserAstNode falseBody;
	std::vector<RustParserStatement> falseBodyDetail;
	RustParserAstNode body;
	std::vector<RustParserStatement> bodyDetail;
	bool isDoWhile = false;
	RustParserAstNode externalCall;
	RustParserExpression externalCallDetail;
	std::vector<RustParserAstNode> clauses;
	std::vector<RustParserTryCatchClause> clauseDetails;
	std::vector<RustParserStatement> clauseBlockStatementDetails;
	RustParserAstNode initExpression;
	std::vector<RustParserStatement> initExpressionDetail;
	RustParserAstNode loopExpression;
	std::vector<RustParserStatement> loopExpressionDetail;
	RustParserAstNode eventCall;
	RustParserAstNode eventCallCallee;
	RustParserExpression eventCallCalleeDetail;
	std::vector<RustParserAstNode> eventCallArguments;
	std::vector<RustParserExpression> eventCallArgumentDetails;
	std::vector<std::string> eventCallParameterNames;
	std::vector<langutil::SourceLocation> eventCallParameterNameLocations;
	RustParserAstNode errorCall;
	RustParserAstNode errorCallCallee;
	RustParserExpression errorCallCalleeDetail;
	std::vector<RustParserAstNode> errorCallArguments;
	std::vector<RustParserExpression> errorCallArgumentDetails;
	std::vector<std::string> errorCallParameterNames;
	std::vector<langutil::SourceLocation> errorCallParameterNameLocations;
	RustParserAstNode expression;
	RustParserExpression expressionDetail;
	std::vector<RustParserAstNode> variables;
	std::vector<RustParserVariableDeclaration> variableDetails;
	RustParserAstNode initialValue;
	RustParserExpression initialValueDetail;
};

struct RustParserFunctionDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	std::uint8_t visibility = 0;
	std::uint8_t stateMutability = 0;
	bool isFreeFunction = false;
	std::uint32_t kind = 0;
	bool isVirtual = false;
	RustParserAstNode overrides;
	std::vector<RustParserAstNode> overridePaths;
	std::vector<RustParserIdentifierPath> overridePathDetails;
	RustParserAstNode documentation;
	RustParserAstNode parameters;
	std::vector<RustParserAstNode> parameterDeclarations;
	std::vector<RustParserVariableDeclaration> parameterDetails;
	std::vector<RustParserAstNode> modifiers;
	std::vector<RustParserModifierInvocation> modifierDetails;
	RustParserAstNode returnParameters;
	std::vector<RustParserAstNode> returnParameterDeclarations;
	std::vector<RustParserVariableDeclaration> returnParameterDetails;
	RustParserAstNode block;
	bool blockUnchecked = false;
	std::vector<RustParserAstNode> blockStatements;
	std::vector<RustParserStatement> blockStatementDetails;
	RustParserAstNode experimentalReturnExpression;
	RustParserExpression experimentalReturnExpressionDetail;
};

struct RustParserForAllQuantifier
{
	RustParserAstNode node;
	RustParserAstNode typeVariableDeclarations;
	std::vector<RustParserAstNode> typeVariableDeclarationParameters;
	std::vector<RustParserVariableDeclaration> typeVariableDeclarationDetails;
	RustParserAstNode quantifiedFunction;
	RustParserFunctionDefinition quantifiedFunctionDetail;
};

struct RustParserModifierDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode documentation;
	RustParserAstNode parameters;
	std::vector<RustParserAstNode> parameterDeclarations;
	std::vector<RustParserVariableDeclaration> parameterDetails;
	bool isVirtual = false;
	RustParserAstNode overrides;
	std::vector<RustParserAstNode> overridePaths;
	std::vector<RustParserIdentifierPath> overridePathDetails;
	RustParserAstNode block;
	bool blockUnchecked = false;
	std::vector<RustParserAstNode> blockStatements;
	std::vector<RustParserStatement> blockStatementDetails;
};

struct RustParserInheritanceSpecifier
{
	RustParserAstNode node;
	RustParserAstNode baseName;
	std::vector<std::string> baseNamePath;
	std::vector<langutil::SourceLocation> baseNamePathLocations;
	bool hasArguments = false;
	std::vector<RustParserAstNode> arguments;
	std::vector<RustParserExpression> argumentDetails;
};

struct RustParserUsingOperator
{
	bool present = false;
	std::uint32_t token = 0;
};

struct RustParserTypeName
{
	RustParserAstNode node;
	std::uint32_t elementaryToken = 0;
	std::uint32_t elementaryFirstNumber = 0;
	std::uint32_t elementarySecondNumber = 0;
	bool hasStateMutability = false;
	std::uint8_t stateMutability = 0;
	RustParserAstNode userDefinedPathNode;
	std::vector<std::string> userDefinedPath;
	std::vector<langutil::SourceLocation> userDefinedPathLocations;
	std::vector<RustParserAstNode> arrayBaseTypes;
	std::vector<RustParserAstNode> arrayLengths;
	std::vector<RustParserExpression> arrayLengthDetails;
	RustParserAstNode functionParameters;
	std::vector<RustParserAstNode> functionParameterDeclarations;
	std::vector<RustParserVariableDeclaration> functionParameterDetails;
	RustParserAstNode functionReturnParameters;
	std::vector<RustParserAstNode> functionReturnParameterDeclarations;
	std::vector<RustParserVariableDeclaration> functionReturnParameterDetails;
	std::uint8_t functionVisibility = 0;
	std::uint8_t functionStateMutability = 0;
	RustParserAstNode mappingKeyType;
	std::uint32_t mappingKeyElementaryToken = 0;
	std::uint32_t mappingKeyElementaryFirstNumber = 0;
	std::uint32_t mappingKeyElementarySecondNumber = 0;
	RustParserAstNode mappingKeyUserDefinedPathNode;
	std::vector<std::string> mappingKeyUserDefinedPath;
	std::vector<langutil::SourceLocation> mappingKeyUserDefinedPathLocations;
	std::string mappingKeyName;
	langutil::SourceLocation mappingKeyNameLocation;
	RustParserAstNode mappingValueType;
	std::uint32_t mappingValueElementaryToken = 0;
	std::uint32_t mappingValueElementaryFirstNumber = 0;
	std::uint32_t mappingValueElementarySecondNumber = 0;
	bool mappingValueHasStateMutability = false;
	std::uint8_t mappingValueStateMutability = 0;
	RustParserAstNode mappingValueUserDefinedPathNode;
	std::vector<std::string> mappingValueUserDefinedPath;
	std::vector<langutil::SourceLocation> mappingValueUserDefinedPathLocations;
	std::vector<RustParserAstNode> mappingValueArrayBaseTypes;
	std::vector<RustParserAstNode> mappingValueArrayLengths;
	std::vector<RustParserExpression> mappingValueArrayLengthDetails;
	RustParserAstNode mappingValueFunctionParameters;
	std::vector<RustParserAstNode> mappingValueFunctionParameterDeclarations;
	std::vector<RustParserVariableDeclaration> mappingValueFunctionParameterDetails;
	RustParserAstNode mappingValueFunctionReturnParameters;
	std::vector<RustParserAstNode> mappingValueFunctionReturnParameterDeclarations;
	std::vector<RustParserVariableDeclaration> mappingValueFunctionReturnParameterDetails;
	std::uint8_t mappingValueFunctionVisibility = 0;
	std::uint8_t mappingValueFunctionStateMutability = 0;
	std::string mappingValueName;
	langutil::SourceLocation mappingValueNameLocation;
	std::vector<RustParserMappingTypeName> mappingDetails;
};

struct RustParserUsingDirective
{
	RustParserAstNode node;
	std::vector<RustParserAstNode> functions;
	std::vector<RustParserIdentifierPath> functionDetails;
	std::vector<RustParserUsingOperator> operators;
	bool usesBraces = false;
	RustParserAstNode typeName;
	RustParserTypeName typeNameDetail;
	bool global = false;
};

struct RustParserUserDefinedValueTypeDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode typeName;
	std::uint32_t typeNameElementaryToken = 0;
	std::uint32_t typeNameElementaryFirstNumber = 0;
	std::uint32_t typeNameElementarySecondNumber = 0;
	bool typeNameHasStateMutability = false;
	std::uint8_t typeNameStateMutability = 0;
	RustParserTypeName typeNameDetail;
};

struct RustParserTypeDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode arguments;
	std::vector<RustParserAstNode> argumentParameters;
	std::vector<RustParserVariableDeclaration> argumentDetails;
	RustParserAstNode expression;
	RustParserExpression expressionDetail;
	bool hasBuiltinNameParameter = false;
	std::string builtinNameParameter;
	langutil::SourceLocation builtinNameParameterLocation;
};

struct RustParserTypeClassDefinition
{
	RustParserAstNode node;
	RustParserAstNode typeVariable;
	std::string typeVariableName;
	langutil::SourceLocation typeVariableNameLocation;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode documentation;
	std::vector<RustParserAstNode> subNodes;
	std::vector<RustParserFunctionDefinition> subNodeFunctionDetails;
};

struct RustParserTypeClassName
{
	RustParserAstNode node;
	bool isBuiltin = false;
	std::uint32_t builtinToken = 0;
	RustParserAstNode identifierPath;
	RustParserIdentifierPath identifierPathDetail;
};

struct RustParserTypeClassInstantiation
{
	RustParserAstNode node;
	RustParserAstNode typeConstructor;
	RustParserTypeName typeConstructorDetail;
	RustParserAstNode argumentSorts;
	std::vector<RustParserAstNode> argumentSortParameters;
	std::vector<RustParserVariableDeclaration> argumentSortDetails;
	RustParserAstNode typeClassName;
	RustParserTypeClassName typeClassNameDetail;
	std::vector<RustParserAstNode> subNodes;
	std::vector<RustParserFunctionDefinition> subNodeFunctionDetails;
};

struct RustParserContractDefinition
{
	RustParserAstNode node;
	std::string name;
	langutil::SourceLocation nameLocation;
	RustParserAstNode documentation;
	std::vector<RustParserAstNode> baseContracts;
	std::vector<RustParserInheritanceSpecifier> baseContractDetails;
	std::vector<RustParserAstNode> subNodes;
	std::vector<RustParserStructDefinition> subNodeStructs;
	std::vector<RustParserEnumDefinition> subNodeEnums;
	std::vector<RustParserUserDefinedValueTypeDefinition> subNodeUserDefinedValueTypes;
	std::vector<RustParserEventDefinition> subNodeEvents;
	std::vector<RustParserErrorDefinition> subNodeErrors;
	std::vector<RustParserFunctionDefinition> subNodeFunctions;
	std::vector<RustParserModifierDefinition> subNodeModifiers;
	std::vector<RustParserUsingDirective> subNodeUsingDirectives;
	std::vector<RustParserVariableDeclaration> subNodeVariableDeclarations;
	std::uint8_t contractKind = 0;
	bool isAbstract = false;
	RustParserAstNode storageLayoutSpecifier;
	RustParserAstNode storageLayoutBaseSlotExpression;
	RustParserExpression storageLayoutBaseSlotExpressionDetail;
};

struct RustParserResult
{
	bool ok = false;
	RustParserErrorCode errorCode = RustParserErrorCode::Unknown;
	std::string errorMessage;
	std::string source;
	std::string sourceName;
	langutil::EVMVersion evmVersion;
	RustParserAstNode sourceUnit;
	std::vector<RustParserAstNode> sourceUnitNodes;
	std::vector<RustParserPragmaDirective> sourceUnitPragmas;
	std::vector<RustParserImportDirective> sourceUnitImports;
	std::vector<RustParserUserDefinedValueTypeDefinition> sourceUnitUserDefinedValueTypes;
	std::vector<RustParserEnumDefinition> sourceUnitEnums;
	std::vector<RustParserStructDefinition> sourceUnitStructs;
	std::vector<RustParserEventDefinition> sourceUnitEvents;
	std::vector<RustParserErrorDefinition> sourceUnitErrors;
	std::vector<RustParserContractDefinition> sourceUnitContracts;
	std::vector<RustParserFunctionDefinition> sourceUnitFunctions;
	std::vector<RustParserForAllQuantifier> sourceUnitForAllQuantifiers;
	std::vector<RustParserTypeDefinition> sourceUnitTypeDefinitions;
	std::vector<RustParserTypeClassDefinition> sourceUnitTypeClassDefinitions;
	std::vector<RustParserTypeClassInstantiation> sourceUnitTypeClassInstantiations;
	std::vector<RustParserUsingDirective> sourceUnitUsingDirectives;
	std::vector<RustParserVariableDeclaration> sourceUnitVariableDeclarations;
	std::vector<RustParserDiagnostic> errors;
	std::vector<RustParserDiagnostic> warnings;
	bool hasLicense = false;
	std::string license;
	bool experimentalSolidity = false;
	std::int64_t maxID = 0;
};

struct RustParserSourceUnitResult
{
	RustParserResult result;
	ASTPointer<SourceUnit> sourceUnit;
};

RustParserResult parseSourceUnitWithRust(
	langutil::CharStream const& _charStream,
	langutil::EVMVersion _evmVersion,
	std::int64_t _currentNodeID = 0
);

RustParserSourceUnitResult parseSourceUnitWithRustIfSupported(
	langutil::CharStream const& _charStream,
	langutil::EVMVersion _evmVersion,
	std::int64_t _currentNodeID = 0
);

/// Parses a source unit with the Rust parser and reports diagnostics through
/// the regular C++ parser error reporter.
/// May throw langutil::FatalError if a fatal Rust parser diagnostic is encountered.
RustParserResult parseSourceUnitWithRust(
	langutil::CharStream const& _charStream,
	langutil::EVMVersion _evmVersion,
	langutil::ErrorReporter& _errorReporter,
	std::int64_t _currentNodeID = 0
);

ASTPointer<PragmaDirective> createPragmaDirectiveAstFromRust(RustParserPragmaDirective const& _pragma);
ASTPointer<ImportDirective> createImportDirectiveAstFromRust(RustParserImportDirective const& _import);
ASTPointer<EnumDefinition> createEnumDefinitionAstFromRust(RustParserEnumDefinition const& _enum);
ASTPointer<VariableDeclaration> createVariableDeclarationAstFromRust(RustParserVariableDeclaration const& _variable);
ASTPointer<StructDefinition> createStructDefinitionAstFromRust(RustParserStructDefinition const& _struct);
ASTPointer<EventDefinition> createEventDefinitionAstFromRust(RustParserEventDefinition const& _event);
ASTPointer<ErrorDefinition> createErrorDefinitionAstFromRust(RustParserErrorDefinition const& _error);
ASTPointer<FunctionDefinition> createFunctionDefinitionAstFromRust(RustParserFunctionDefinition const& _function);
ASTPointer<ForAllQuantifier> createForAllQuantifierAstFromRust(RustParserForAllQuantifier const& _quantifier);
ASTPointer<TypeDefinition> createTypeDefinitionAstFromRust(RustParserTypeDefinition const& _typeDefinition);
ASTPointer<TypeClassDefinition> createTypeClassDefinitionAstFromRust(
	RustParserTypeClassDefinition const& _typeClassDefinition
);
ASTPointer<TypeClassInstantiation> createTypeClassInstantiationAstFromRust(
	RustParserTypeClassInstantiation const& _typeClassInstantiation
);
ASTPointer<ModifierDefinition> createModifierDefinitionAstFromRust(RustParserModifierDefinition const& _modifier);
ASTPointer<UsingForDirective> createUsingDirectiveAstFromRust(RustParserUsingDirective const& _using);
ASTPointer<ContractDefinition> createContractDefinitionAstFromRust(RustParserContractDefinition const& _contract);
ASTPointer<UserDefinedValueTypeDefinition> createUserDefinedValueTypeDefinitionAstFromRust(
	RustParserUserDefinedValueTypeDefinition const& _typeDefinition
);

/// Creates a native C++ SourceUnit AST when every Rust-parsed child node
/// currently has a native bridge conversion. Returns nullptr for unsupported
/// source-unit child kinds.
ASTPointer<SourceUnit> createSourceUnitAstFromRustIfSupported(RustParserResult const& _result);

/// Reports Rust parser diagnostics through the regular C++ parser error reporter.
/// May throw langutil::FatalError if a fatal Rust parser diagnostic is encountered.
void reportRustParserDiagnostics(
	langutil::ErrorReporter& _errorReporter,
	RustParserResult const& _result
);

#endif

}
