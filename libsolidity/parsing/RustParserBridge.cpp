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

#include <libsolidity/parsing/RustParserBridge.h>

#if defined(SOLIDITY_USE_RUST_SOLIDITY_PARSER)

#include <solidity_parser_bridge/bridge.h>

#include <libsolidity/ast/AST.h>
#include <libsolidity/ast/UserDefinableOperators.h>
#include <libsolidity/interface/Version.h>

#include <libsolutil/Common.h>

#include <liblangutil/CharStream.h>
#include <liblangutil/ErrorReporter.h>
#include <liblangutil/Exceptions.h>
#include <liblangutil/Scanner.h>
#include <liblangutil/Token.h>

#include <libyul/AsmParser.h>
#include <libyul/backends/evm/EVMDialect.h>

#include <algorithm>
#include <functional>
#include <memory>
#include <optional>
#include <string_view>

using namespace solidity;
using namespace solidity::frontend;
using namespace solidity::langutil;

namespace
{

namespace rust_ffi = solidity::frontend::rust;

struct RustParserReconstructionContext
{
	std::string const& source;
	std::string const& sourceName;
	EVMVersion evmVersion;
	bool experimentalSolidity;
};

thread_local RustParserReconstructionContext const* currentRustParserReconstructionContext = nullptr;

constexpr std::uint8_t rustAstNodeKindStructuredDocumentation = 1;
constexpr std::uint8_t rustAstNodeKindIdentifier = 2;
constexpr std::uint8_t rustAstNodeKindIdentifierPath = 3;
constexpr std::uint8_t rustAstNodeKindUserDefinedTypeName = 4;
constexpr std::uint8_t rustAstNodeKindEnumValue = 5;
constexpr std::uint8_t rustAstNodeKindParameterList = 6;
constexpr std::uint8_t rustAstNodeKindOverrideSpecifier = 7;
constexpr std::uint8_t rustAstNodeKindTypeClassName = 8;
constexpr std::uint8_t rustAstNodeKindInheritanceSpecifier = 9;
constexpr std::uint8_t rustAstNodeKindModifierInvocation = 10;
constexpr std::uint8_t rustAstNodeKindElementaryTypeNameExpression = 11;
constexpr std::uint8_t rustAstNodeKindElementaryTypeName = 12;
constexpr std::uint8_t rustAstNodeKindArrayTypeName = 13;
constexpr std::uint8_t rustAstNodeKindMemberAccess = 14;
constexpr std::uint8_t rustAstNodeKindIndexAccess = 15;
constexpr std::uint8_t rustAstNodeKindIndexRangeAccess = 16;
constexpr std::uint8_t rustAstNodeKindPragmaDirective = 17;
constexpr std::uint8_t rustAstNodeKindImportDirective = 18;
constexpr std::uint8_t rustAstNodeKindContractDefinition = 19;
constexpr std::uint8_t rustAstNodeKindForAllQuantifier = 20;
constexpr std::uint8_t rustAstNodeKindFunctionDefinition = 21;
constexpr std::uint8_t rustAstNodeKindStructDefinition = 22;
constexpr std::uint8_t rustAstNodeKindEnumDefinition = 23;
constexpr std::uint8_t rustAstNodeKindVariableDeclaration = 24;
constexpr std::uint8_t rustAstNodeKindFunctionTypeName = 25;
constexpr std::uint8_t rustAstNodeKindModifierDefinition = 26;
constexpr std::uint8_t rustAstNodeKindEventDefinition = 27;
constexpr std::uint8_t rustAstNodeKindErrorDefinition = 28;
constexpr std::uint8_t rustAstNodeKindUsingForDirective = 29;
constexpr std::uint8_t rustAstNodeKindUserDefinedValueTypeDefinition = 30;
constexpr std::uint8_t rustAstNodeKindMapping = 31;
constexpr std::uint8_t rustAstNodeKindBlock = 32;
constexpr std::uint8_t rustAstNodeKindContinueStatement = 33;
constexpr std::uint8_t rustAstNodeKindBreakStatement = 34;
constexpr std::uint8_t rustAstNodeKindReturnStatement = 35;
constexpr std::uint8_t rustAstNodeKindThrowStatement = 36;
constexpr std::uint8_t rustAstNodeKindPlaceholderStatement = 37;
constexpr std::uint8_t rustAstNodeKindInlineAssembly = 38;
constexpr std::uint8_t rustAstNodeKindIfStatement = 39;
constexpr std::uint8_t rustAstNodeKindTryStatement = 40;
constexpr std::uint8_t rustAstNodeKindTryCatchClause = 41;
constexpr std::uint8_t rustAstNodeKindWhileStatement = 42;
constexpr std::uint8_t rustAstNodeKindForStatement = 43;
constexpr std::uint8_t rustAstNodeKindEmitStatement = 44;
constexpr std::uint8_t rustAstNodeKindRevertStatement = 45;
constexpr std::uint8_t rustAstNodeKindFunctionCall = 46;
constexpr std::uint8_t rustAstNodeKindVariableDeclarationStatement = 47;
constexpr std::uint8_t rustAstNodeKindTypeClassDefinition = 48;
constexpr std::uint8_t rustAstNodeKindTypeClassInstantiation = 49;
constexpr std::uint8_t rustAstNodeKindTypeDefinition = 50;
constexpr std::uint8_t rustAstNodeKindBuiltin = 51;
constexpr std::uint8_t rustAstNodeKindExpressionStatement = 52;
constexpr std::uint8_t rustAstNodeKindTupleExpression = 53;
constexpr std::uint8_t rustAstNodeKindAssignment = 54;
constexpr std::uint8_t rustAstNodeKindConditional = 55;
constexpr std::uint8_t rustAstNodeKindBinaryOperation = 56;
constexpr std::uint8_t rustAstNodeKindUnaryOperation = 57;
constexpr std::uint8_t rustAstNodeKindNewExpression = 58;
constexpr std::uint8_t rustAstNodeKindFunctionCallOptions = 59;
constexpr std::uint8_t rustAstNodeKindLiteral = 60;
constexpr std::uint8_t rustAstNodeKindStorageLayoutSpecifier = 61;
constexpr std::uint8_t rustAstNodeKindSourceUnit = 62;
constexpr std::uint8_t rustAstNodeKindDoWhileStatement = 63;
constexpr std::uint8_t rustAstNodeKindInlineArrayExpression = 64;
constexpr std::uint8_t rustAstNodeKindUncheckedBlock = 65;
constexpr size_t rustBridgeMaxArrayTypeDepth = 256;
constexpr std::uint8_t rustContractKindInterface = 0;
constexpr std::uint8_t rustContractKindContract = 1;
constexpr std::uint8_t rustContractKindLibrary = 2;
constexpr std::uint8_t rustStateMutabilityPure = 0;
constexpr std::uint8_t rustStateMutabilityView = 1;
constexpr std::uint8_t rustStateMutabilityNonPayable = 2;
constexpr std::uint8_t rustStateMutabilityPayable = 3;
constexpr std::uint8_t rustVisibilityDefault = 0;
constexpr std::uint8_t rustVisibilityPrivate = 1;
constexpr std::uint8_t rustVisibilityInternal = 2;
constexpr std::uint8_t rustVisibilityPublic = 3;
constexpr std::uint8_t rustVisibilityExternal = 4;
constexpr std::uint8_t rustVariableDeclarationMutabilityMutable = 0;
constexpr std::uint8_t rustVariableDeclarationMutabilityImmutable = 1;
constexpr std::uint8_t rustVariableDeclarationMutabilityConstant = 2;
constexpr std::uint8_t rustVariableDeclarationLocationUnspecified = 0;
constexpr std::uint8_t rustVariableDeclarationLocationStorage = 1;
constexpr std::uint8_t rustVariableDeclarationLocationTransient = 2;
constexpr std::uint8_t rustVariableDeclarationLocationMemory = 3;
constexpr std::uint8_t rustVariableDeclarationLocationCallData = 4;

bool rustAstNodesMatch(RustParserAstNode const& _left, RustParserAstNode const& _right)
{
	return
		_left.present == _right.present &&
		_left.nodeID == _right.nodeID &&
		_left.kind == _right.kind;
}

bool functionCallParameterNamesAreValid(
	size_t _arguments,
	std::vector<std::string> const& _parameterNames,
	std::vector<SourceLocation> const& _parameterNameLocations
)
{
	return
		_parameterNames.size() == _parameterNameLocations.size() &&
			(_parameterNames.empty() || _parameterNames.size() == _arguments);
}

bool pragmaTokensAreValid(std::vector<Token> const& _tokens)
{
	return std::all_of(_tokens.begin(), _tokens.end(), [](Token _token) {
		return
			static_cast<size_t>(_token) < TokenTraits::count() &&
			_token != Token::EOS &&
			_token != Token::Semicolon &&
			_token != Token::Illegal &&
			_token != Token::Whitespace;
	});
}

RustParserIdentifierPath identifierPathFromWire(
	rust_ffi::WireIdentifierPathResult const& _path,
	std::shared_ptr<std::string const> const& _sourceName
);

RustParserTypeName typeNameFromWire(
	rust_ffi::WireTypeNameResult const& _typeName,
	std::shared_ptr<std::string const> const& _sourceName
);

RustParserVariableDeclaration variableDeclarationFromWire(
	rust_ffi::WireVariableDeclarationResult const& _variable,
	std::shared_ptr<std::string const> const& _sourceName
);

ASTPointer<OverrideSpecifier> createOverrideSpecifierAstFromRust(
	RustParserAstNode const& _overrides,
	std::vector<RustParserAstNode> const& _overridePaths,
	std::vector<RustParserIdentifierPath> const& _overridePathDetails
);

::rust::Vec<std::uint8_t> rustBytes(std::string_view _bytes)
{
	::rust::Vec<std::uint8_t> output;
	output.reserve(_bytes.size());
	for (char byte: _bytes)
		output.push_back(static_cast<std::uint8_t>(byte));
	return output;
}

std::string cppString(rust_ffi::WireString const& _string)
{
	return std::string(_string.bytes.begin(), _string.bytes.end());
}

rust_ffi::WireString wireString(std::string_view _string)
{
	return rust_ffi::WireString{rustBytes(_string)};
}

ASTPointer<ASTString> astString(std::string _string)
{
	return std::make_shared<ASTString>(std::move(_string));
}

bool elementaryTypeSizingIsValid(Token _token, std::uint32_t _firstNumber, std::uint32_t _secondNumber)
{
	if (!TokenTraits::isElementaryTypeName(_token))
		return false;

	if (_token == Token::BytesM)
		return _secondNumber == 0 && _firstNumber <= 32;
	if (_token == Token::UIntM || _token == Token::IntM)
		return _secondNumber == 0 && _firstNumber <= 256 && _firstNumber % 8 == 0;
	if (_token == Token::UFixedMxN || _token == Token::FixedMxN)
		return _firstNumber >= 8 && _firstNumber <= 256 && _firstNumber % 8 == 0 && _secondNumber <= 80;
	return _firstNumber == 0 && _secondNumber == 0;
}

std::optional<StateMutability> stateMutabilityFromRust(std::uint8_t _stateMutability)
{
	switch (_stateMutability)
	{
	case rustStateMutabilityPure:
		return StateMutability::Pure;
	case rustStateMutabilityView:
		return StateMutability::View;
	case rustStateMutabilityNonPayable:
		return StateMutability::NonPayable;
	case rustStateMutabilityPayable:
		return StateMutability::Payable;
	default:
		return std::nullopt;
	}
}

std::optional<ContractKind> contractKindFromRust(std::uint8_t _contractKind)
{
	switch (_contractKind)
	{
	case rustContractKindInterface:
		return ContractKind::Interface;
	case rustContractKindContract:
		return ContractKind::Contract;
	case rustContractKindLibrary:
		return ContractKind::Library;
	default:
		return std::nullopt;
	}
}

std::optional<Visibility> visibilityFromRust(std::uint8_t _visibility)
{
	switch (_visibility)
	{
	case rustVisibilityDefault:
		return Visibility::Default;
	case rustVisibilityPrivate:
		return Visibility::Private;
	case rustVisibilityInternal:
		return Visibility::Internal;
	case rustVisibilityPublic:
		return Visibility::Public;
	case rustVisibilityExternal:
		return Visibility::External;
	default:
		return std::nullopt;
	}
}

std::optional<VariableDeclaration::Mutability> variableDeclarationMutabilityFromRust(std::uint8_t _mutability)
{
	switch (_mutability)
	{
	case rustVariableDeclarationMutabilityMutable:
		return VariableDeclaration::Mutability::Mutable;
	case rustVariableDeclarationMutabilityImmutable:
		return VariableDeclaration::Mutability::Immutable;
	case rustVariableDeclarationMutabilityConstant:
		return VariableDeclaration::Mutability::Constant;
	default:
		return std::nullopt;
	}
}

std::optional<VariableDeclaration::Location> variableDeclarationLocationFromRust(std::uint8_t _location)
{
	switch (_location)
	{
	case rustVariableDeclarationLocationUnspecified:
		return VariableDeclaration::Location::Unspecified;
	case rustVariableDeclarationLocationStorage:
		return VariableDeclaration::Location::Storage;
	case rustVariableDeclarationLocationTransient:
		return VariableDeclaration::Location::Transient;
	case rustVariableDeclarationLocationMemory:
		return VariableDeclaration::Location::Memory;
	case rustVariableDeclarationLocationCallData:
		return VariableDeclaration::Location::CallData;
	default:
		return std::nullopt;
	}
}

SourceLocation sourceLocation(
	rust_ffi::WireSourceLocation const& _location,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (_location.source_id < 0)
		return SourceLocation{
			static_cast<int>(_location.start),
			static_cast<int>(_location.end),
			nullptr,
		};

	return SourceLocation{
		static_cast<int>(_location.start),
		static_cast<int>(_location.end),
		_sourceName,
	};
}

rust_ffi::WireSourceLocation wireSourceLocation(SourceLocation const& _location)
{
	return rust_ffi::WireSourceLocation{
		static_cast<std::int64_t>(_location.start),
		static_cast<std::int64_t>(_location.end),
		_location.sourceName ? 0 : -1,
	};
}

RustParserErrorCode errorCodeFromWire(std::uint8_t _code)
{
	switch (_code)
	{
	case 0:
		return RustParserErrorCode::None;
	case 1:
		return RustParserErrorCode::Parse;
	default:
		return RustParserErrorCode::Unknown;
	}
}

RustParserAstNode astNodeFromWire(
	rust_ffi::WireAstNode const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserAstNode{
		_node.present,
		_node.node_id,
		_node.kind,
		sourceLocation(_node.location, _sourceName),
		cppString(_node.text),
	};
}

RustParserDiagnostic diagnosticFromWire(
	rust_ffi::WireParserError const& _error,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserDiagnostic diagnostic{
		_error.error_id,
		std::string(_error.message),
		sourceLocation(_error.location, _sourceName),
		{},
		_error.syntax,
		_error.fatal,
	};
	diagnostic.secondaryLocations.reserve(_error.secondary_locations.size());
	for (auto const& secondaryLocation: _error.secondary_locations)
		diagnostic.secondaryLocations.emplace_back(
			std::string(secondaryLocation.message),
			sourceLocation(secondaryLocation.location, _sourceName)
		);
	return diagnostic;
}

SecondarySourceLocation secondarySourceLocationFromRust(RustParserDiagnostic const& _diagnostic)
{
	SecondarySourceLocation secondarySourceLocation;
	for (auto const& [message, location]: _diagnostic.secondaryLocations)
		secondarySourceLocation.append(message, location);
	return secondarySourceLocation;
}

void reportRustParserDiagnostic(
	ErrorReporter& _errorReporter,
	RustParserDiagnostic const& _diagnostic,
	bool _warning
)
{
	ErrorId const errorID{_diagnostic.errorID};
	SecondarySourceLocation secondaryLocation = secondarySourceLocationFromRust(_diagnostic);
	if (_warning)
	{
		if (secondaryLocation.infos.empty())
			_errorReporter.warning(errorID, _diagnostic.location, _diagnostic.message);
		else
			_errorReporter.warning(errorID, _diagnostic.location, _diagnostic.message, secondaryLocation);
		return;
	}

	if (_diagnostic.syntax)
	{
		_errorReporter.syntaxError(errorID, _diagnostic.location, _diagnostic.message);
		return;
	}

	if (_diagnostic.fatal)
	{
		if (secondaryLocation.infos.empty())
			_errorReporter.fatalParserError(errorID, _diagnostic.location, _diagnostic.message);
		else
		{
			_errorReporter.parserError(errorID, _diagnostic.location, secondaryLocation, _diagnostic.message);
			solThrow(FatalError, _diagnostic.message);
		}
		return;
	}

	if (secondaryLocation.infos.empty())
		_errorReporter.parserError(errorID, _diagnostic.location, _diagnostic.message);
	else
		_errorReporter.parserError(errorID, _diagnostic.location, secondaryLocation, _diagnostic.message);
}

rust_ffi::WireLocatedToken wireLocatedToken(Scanner const& _scanner)
{
	std::string error;
	if (_scanner.currentToken() == Token::Illegal)
		error = to_string(_scanner.currentError());
	auto const [firstNumber, secondNumber] = _scanner.currentTokenInfo();

	return rust_ffi::WireLocatedToken{
		static_cast<std::uint32_t>(_scanner.currentToken()),
		wireString(_scanner.currentLiteral()),
		std::string{},
		firstNumber,
		secondNumber,
		std::move(error),
		wireSourceLocation(_scanner.currentLocation()),
	};
}

void appendCurrentComment(
	Scanner const& _scanner,
	::rust::Vec<rust_ffi::WireString>& _commentLiterals,
	::rust::Vec<rust_ffi::WireSourceLocation>& _commentLocations
)
{
	_commentLiterals.push_back(wireString(_scanner.currentCommentLiteral()));
	_commentLocations.push_back(wireSourceLocation(_scanner.currentCommentLocation()));
}

void appendTokensAndComments(
	Scanner& _scanner,
	::rust::Vec<rust_ffi::WireLocatedToken>& _tokens,
	::rust::Vec<rust_ffi::WireString>& _commentLiterals,
	::rust::Vec<rust_ffi::WireSourceLocation>& _commentLocations
)
{
	while (true)
	{
		_tokens.push_back(wireLocatedToken(_scanner));
		appendCurrentComment(_scanner, _commentLiterals, _commentLocations);
		if (_scanner.currentToken() == Token::EOS)
			break;
		_scanner.next();
	}
}

template <class T, class Items, class Convert>
std::vector<T> cppVectorFromRust(Items const& _items, Convert _convert)
{
	std::vector<T> output;
	output.reserve(_items.size());
	for (auto const& item: _items)
		output.push_back(_convert(item));
	return output;
}

RustParserExpression expressionFromWire(
	rust_ffi::WireExpressionResult const& _expression,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserExpression{
		astNodeFromWire(_expression.expression, _sourceName),
		astNodeFromWire(_expression.left_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.left_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.right_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.right_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.condition_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.condition_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.true_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.true_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.false_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.false_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.sub_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.sub_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		_expression.is_prefix_operation,
		astNodeFromWire(_expression.base_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.base_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.index_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.index_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.end_index_expression, _sourceName),
		cppVectorFromRust<RustParserExpression>(
			_expression.end_index_expression_detail,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		),
		astNodeFromWire(_expression.type_name, _sourceName),
		std::make_shared<RustParserTypeName>(typeNameFromWire(_expression.type_name_detail, _sourceName)),
		astNodeFromWire(_expression.expression_type, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_expression.arguments,
			[&](rust_ffi::WireAstNode const& _argument)
			{
				return astNodeFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_expression.argument_details,
			[&](rust_ffi::WireExpressionResult const& _argument)
			{
				return expressionFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<std::string>(
			_expression.parameter_names,
			[&](rust_ffi::WireString const& _parameterName)
			{
				return cppString(_parameterName);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_expression.parameter_name_locations,
			[&](rust_ffi::WireSourceLocation const& _parameterNameLocation)
			{
				return sourceLocation(_parameterNameLocation, _sourceName);
			}
		),
		sourceLocation(_expression.member_name_location, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_expression.components,
			[&](rust_ffi::WireAstNode const& _component)
			{
				return astNodeFromWire(_component, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_expression.component_details,
			[&](rust_ffi::WireExpressionResult const& _component)
			{
				return expressionFromWire(_component, _sourceName);
			}
		),
		_expression.is_inline_array,
		_expression.literal_token,
		_expression.literal_subdenomination,
	};
}

RustParserPragmaDirective pragmaDirectiveFromWire(
	rust_ffi::WirePragmaDirectiveResult const& _pragma,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserPragmaDirective{
		astNodeFromWire(_pragma.pragma_directive, _sourceName),
		cppVectorFromRust<Token>(
			_pragma.tokens,
			[](std::uint32_t _token) { return static_cast<Token>(_token); }
		),
		cppVectorFromRust<std::string>(
			_pragma.literals,
			[](rust_ffi::WireString const& _literal) { return cppString(_literal); }
		),
	};
}

RustParserImportSymbolAlias importSymbolAliasFromWire(
	rust_ffi::WireImportSymbolAlias const& _alias,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserImportSymbolAlias{
		astNodeFromWire(_alias.symbol, _sourceName),
		_alias.has_alias,
		cppString(_alias.alias),
		sourceLocation(_alias.location, _sourceName),
	};
}

RustParserImportDirective importDirectiveFromWire(
	rust_ffi::WireImportDirectiveResult const& _import,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserImportDirective{
		astNodeFromWire(_import.import_directive, _sourceName),
		cppString(_import.path),
		cppString(_import.unit_alias),
		sourceLocation(_import.unit_alias_location, _sourceName),
		cppVectorFromRust<RustParserImportSymbolAlias>(
			_import.symbol_aliases,
			[&](rust_ffi::WireImportSymbolAlias const& _alias)
			{
				return importSymbolAliasFromWire(_alias, _sourceName);
			}
		),
	};
}

RustParserEnumValue enumValueFromWire(
	rust_ffi::WireEnumValueResult const& _value,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserEnumValue{
		astNodeFromWire(_value.enum_value, _sourceName),
		cppString(_value.name),
		sourceLocation(_value.name_location, _sourceName),
		astNodeFromWire(_value.documentation, _sourceName),
	};
}

RustParserEnumDefinition enumDefinitionFromWire(
	rust_ffi::WireEnumDefinitionResult const& _enum,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserEnumDefinition{
		astNodeFromWire(_enum.enum_definition, _sourceName),
		cppString(_enum.name),
		sourceLocation(_enum.name_location, _sourceName),
		cppVectorFromRust<RustParserEnumValue>(
			_enum.member_details,
			[&](rust_ffi::WireEnumValueResult const& _value)
			{
				return enumValueFromWire(_value, _sourceName);
			}
		),
		astNodeFromWire(_enum.documentation, _sourceName),
	};
}

RustParserMappingTypeName mappingTypeNameFromWire(
	rust_ffi::WireMappingTypeName const& _mapping,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserMappingTypeName{
		astNodeFromWire(_mapping.mapping, _sourceName),
		astNodeFromWire(_mapping.key_type, _sourceName),
		_mapping.key_type_elementary_token,
		_mapping.key_type_elementary_first_number,
		_mapping.key_type_elementary_second_number,
		astNodeFromWire(_mapping.key_type_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_mapping.key_type_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_mapping.key_type_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppString(_mapping.key_name),
		sourceLocation(_mapping.key_name_location, _sourceName),
		astNodeFromWire(_mapping.value_type, _sourceName),
		_mapping.value_type_elementary_token,
		_mapping.value_type_elementary_first_number,
		_mapping.value_type_elementary_second_number,
		_mapping.value_type_has_state_mutability,
		_mapping.value_type_state_mutability,
		astNodeFromWire(_mapping.value_type_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_mapping.value_type_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_mapping.value_type_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_mapping.value_type_array_base_types,
			[&](rust_ffi::WireAstNode const& _baseType)
			{
				return astNodeFromWire(_baseType, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_mapping.value_type_array_lengths,
			[&](rust_ffi::WireAstNode const& _length)
			{
				return astNodeFromWire(_length, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_mapping.value_type_array_length_details,
			[&](rust_ffi::WireExpressionResult const& _length)
			{
				return expressionFromWire(_length, _sourceName);
			}
		),
		astNodeFromWire(_mapping.value_type_function_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_mapping.value_type_function_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_mapping.value_type_function_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_mapping.value_type_function_return_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_mapping.value_type_function_return_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_mapping.value_type_function_return_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_mapping.value_type_function_visibility,
		_mapping.value_type_function_state_mutability,
		cppString(_mapping.value_name),
		sourceLocation(_mapping.value_name_location, _sourceName),
	};
}

RustParserVariableDeclaration variableDeclarationFromWire(
	rust_ffi::WireVariableDeclarationResult const& _variable,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserVariableDeclaration{
		astNodeFromWire(_variable.variable_declaration, _sourceName),
		astNodeFromWire(_variable.type_name, _sourceName),
		astNodeFromWire(_variable.type_expression, _sourceName),
		expressionFromWire(_variable.type_expression_detail, _sourceName),
		astNodeFromWire(_variable.documentation, _sourceName),
		astNodeFromWire(_variable.overrides, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_variable.override_paths,
			[&](rust_ffi::WireAstNode const& _overridePath)
			{
				return astNodeFromWire(_overridePath, _sourceName);
			}
		),
		cppVectorFromRust<RustParserIdentifierPath>(
			_variable.override_path_details,
			[&](rust_ffi::WireIdentifierPathResult const& _overridePath)
			{
				return identifierPathFromWire(_overridePath, _sourceName);
			}
		),
		astNodeFromWire(_variable.value, _sourceName),
		expressionFromWire(_variable.value_detail, _sourceName),
		_variable.type_name_elementary_token,
		_variable.type_name_elementary_first_number,
		_variable.type_name_elementary_second_number,
		_variable.type_name_has_state_mutability,
		_variable.type_name_state_mutability,
		astNodeFromWire(_variable.type_name_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_variable.type_name_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_variable.type_name_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_array_base_types,
			[&](rust_ffi::WireAstNode const& _baseType)
			{
				return astNodeFromWire(_baseType, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_array_lengths,
			[&](rust_ffi::WireAstNode const& _length)
			{
				return astNodeFromWire(_length, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_variable.type_name_array_length_details,
			[&](rust_ffi::WireExpressionResult const& _length)
			{
				return expressionFromWire(_length, _sourceName);
			}
		),
		astNodeFromWire(_variable.type_name_function_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_function_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_variable.type_name_function_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_variable.type_name_function_return_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_function_return_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_variable.type_name_function_return_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_variable.type_name_function_visibility,
		_variable.type_name_function_state_mutability,
		astNodeFromWire(_variable.type_name_mapping_key_type, _sourceName),
		_variable.type_name_mapping_key_elementary_token,
		_variable.type_name_mapping_key_elementary_first_number,
		_variable.type_name_mapping_key_elementary_second_number,
		astNodeFromWire(_variable.type_name_mapping_key_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_variable.type_name_mapping_key_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_variable.type_name_mapping_key_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppString(_variable.type_name_mapping_key_name),
		sourceLocation(_variable.type_name_mapping_key_name_location, _sourceName),
		astNodeFromWire(_variable.type_name_mapping_value_type, _sourceName),
		_variable.type_name_mapping_value_elementary_token,
		_variable.type_name_mapping_value_elementary_first_number,
		_variable.type_name_mapping_value_elementary_second_number,
		_variable.type_name_mapping_value_has_state_mutability,
		_variable.type_name_mapping_value_state_mutability,
		astNodeFromWire(_variable.type_name_mapping_value_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_variable.type_name_mapping_value_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_variable.type_name_mapping_value_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_mapping_value_array_base_types,
			[&](rust_ffi::WireAstNode const& _baseType)
			{
				return astNodeFromWire(_baseType, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_mapping_value_array_lengths,
			[&](rust_ffi::WireAstNode const& _length)
			{
				return astNodeFromWire(_length, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_variable.type_name_mapping_value_array_length_details,
			[&](rust_ffi::WireExpressionResult const& _length)
			{
				return expressionFromWire(_length, _sourceName);
			}
		),
		astNodeFromWire(_variable.type_name_mapping_value_function_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_mapping_value_function_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_variable.type_name_mapping_value_function_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_variable.type_name_mapping_value_function_return_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_variable.type_name_mapping_value_function_return_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_variable.type_name_mapping_value_function_return_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_variable.type_name_mapping_value_function_visibility,
		_variable.type_name_mapping_value_function_state_mutability,
		cppString(_variable.type_name_mapping_value_name),
		sourceLocation(_variable.type_name_mapping_value_name_location, _sourceName),
		cppVectorFromRust<RustParserMappingTypeName>(
			_variable.type_name_mapping_details,
			[&](rust_ffi::WireMappingTypeName const& _mapping)
			{
				return mappingTypeNameFromWire(_mapping, _sourceName);
			}
		),
		cppString(_variable.name),
		sourceLocation(_variable.name_location, _sourceName),
		_variable.visibility,
		_variable.mutability,
		_variable.variable_location,
		_variable.indexed,
	};
}

RustParserStructDefinition structDefinitionFromWire(
	rust_ffi::WireStructDefinitionResult const& _struct,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserStructDefinition{
		astNodeFromWire(_struct.struct_definition, _sourceName),
		cppString(_struct.name),
		sourceLocation(_struct.name_location, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_struct.members,
			[&](rust_ffi::WireAstNode const& _member)
			{
				return astNodeFromWire(_member, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_struct.member_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _member)
			{
				return variableDeclarationFromWire(_member, _sourceName);
			}
		),
		astNodeFromWire(_struct.documentation, _sourceName),
	};
}

RustParserEventDefinition eventDefinitionFromWire(
	rust_ffi::WireEventDefinitionResult const& _event,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserEventDefinition{
		astNodeFromWire(_event.event_definition, _sourceName),
		cppString(_event.name),
		sourceLocation(_event.name_location, _sourceName),
		astNodeFromWire(_event.documentation, _sourceName),
		astNodeFromWire(_event.parameters, _sourceName),
			cppVectorFromRust<RustParserAstNode>(
				_event.parameter_declarations,
				[&](rust_ffi::WireAstNode const& _parameter)
				{
					return astNodeFromWire(_parameter, _sourceName);
				}
			),
			cppVectorFromRust<RustParserVariableDeclaration>(
				_event.parameter_details,
				[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
				{
					return variableDeclarationFromWire(_parameter, _sourceName);
				}
			),
			_event.anonymous,
		};
	}

RustParserErrorDefinition errorDefinitionFromWire(
	rust_ffi::WireErrorDefinitionResult const& _error,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserErrorDefinition{
		astNodeFromWire(_error.error_definition, _sourceName),
		cppString(_error.name),
		sourceLocation(_error.name_location, _sourceName),
		astNodeFromWire(_error.documentation, _sourceName),
		astNodeFromWire(_error.parameters, _sourceName),
			cppVectorFromRust<RustParserAstNode>(
				_error.parameter_declarations,
				[&](rust_ffi::WireAstNode const& _parameter)
				{
					return astNodeFromWire(_parameter, _sourceName);
				}
			),
			cppVectorFromRust<RustParserVariableDeclaration>(
				_error.parameter_details,
				[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
				{
					return variableDeclarationFromWire(_parameter, _sourceName);
				}
			),
		};
	}

RustParserIdentifierPath identifierPathFromWire(
	rust_ffi::WireIdentifierPathResult const& _path,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserIdentifierPath{
		astNodeFromWire(_path.identifier_path, _sourceName),
		cppVectorFromRust<std::string>(
			_path.path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_path.path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
	};
}

RustParserModifierInvocation modifierInvocationFromWire(
	rust_ffi::WireModifierInvocationResult const& _modifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserModifierInvocation{
		astNodeFromWire(_modifier.modifier_invocation, _sourceName),
		astNodeFromWire(_modifier.modifier_name, _sourceName),
		identifierPathFromWire(_modifier.modifier_name_detail, _sourceName),
		_modifier.has_arguments,
		cppVectorFromRust<RustParserAstNode>(
			_modifier.arguments,
			[&](rust_ffi::WireAstNode const& _argument)
			{
				return astNodeFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_modifier.argument_details,
			[&](rust_ffi::WireExpressionResult const& _argument)
			{
				return expressionFromWire(_argument, _sourceName);
			}
		),
	};
}

RustParserStatement statementFromWire(
	rust_ffi::WireStatementResult const& _statement,
	std::shared_ptr<std::string const> const& _sourceName
);

RustParserTryCatchClause tryCatchClauseFromWire(
	rust_ffi::WireTryCatchClauseResult const& _clause,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserTryCatchClause{
		astNodeFromWire(_clause.try_catch_clause, _sourceName),
		cppString(_clause.error_name),
		astNodeFromWire(_clause.error_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_clause.error_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_clause.error_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_clause.block, _sourceName),
		_clause.block_unchecked,
		cppVectorFromRust<RustParserAstNode>(
			_clause.block_statements,
			[&](rust_ffi::WireAstNode const& _statementNode)
			{
				return astNodeFromWire(_statementNode, _sourceName);
			}
		),
	};
}

std::vector<RustParserStatement> statementsFromWire(
	::rust::Vec<rust_ffi::WireStatementResult> const& _statements,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return cppVectorFromRust<RustParserStatement>(
		_statements,
		[&](rust_ffi::WireStatementResult const& _statement)
		{
			return statementFromWire(_statement, _sourceName);
		}
	);
}

RustParserStatement statementFromWire(
	rust_ffi::WireStatementResult const& _statement,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserStatement{
		astNodeFromWire(_statement.statement, _sourceName),
		_statement.block_unchecked,
		cppVectorFromRust<RustParserAstNode>(
			_statement.block_statements,
			[&](rust_ffi::WireAstNode const& _statementNode)
			{
				return astNodeFromWire(_statementNode, _sourceName);
			}
		),
		statementsFromWire(_statement.block_statement_details, _sourceName),
		cppVectorFromRust<std::string>(
			_statement.inline_assembly_flags,
			[&](rust_ffi::WireString const& _flag)
			{
				return cppString(_flag);
			}
		),
		sourceLocation(_statement.inline_assembly_block_location, _sourceName),
		astNodeFromWire(_statement.condition_expression, _sourceName),
		expressionFromWire(_statement.condition_expression_detail, _sourceName),
		astNodeFromWire(_statement.true_body, _sourceName),
		statementsFromWire(_statement.true_body_detail, _sourceName),
		astNodeFromWire(_statement.false_body, _sourceName),
		statementsFromWire(_statement.false_body_detail, _sourceName),
		astNodeFromWire(_statement.body, _sourceName),
		statementsFromWire(_statement.body_detail, _sourceName),
		_statement.is_do_while,
		astNodeFromWire(_statement.external_call, _sourceName),
		expressionFromWire(_statement.external_call_detail, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_statement.clauses,
			[&](rust_ffi::WireAstNode const& _clause)
			{
				return astNodeFromWire(_clause, _sourceName);
			}
		),
		cppVectorFromRust<RustParserTryCatchClause>(
			_statement.clause_details,
			[&](rust_ffi::WireTryCatchClauseResult const& _clause)
			{
				return tryCatchClauseFromWire(_clause, _sourceName);
			}
		),
		statementsFromWire(_statement.clause_block_statement_details, _sourceName),
		astNodeFromWire(_statement.init_expression, _sourceName),
		statementsFromWire(_statement.init_expression_detail, _sourceName),
		astNodeFromWire(_statement.loop_expression, _sourceName),
		statementsFromWire(_statement.loop_expression_detail, _sourceName),
		astNodeFromWire(_statement.event_call, _sourceName),
		astNodeFromWire(_statement.event_call_callee, _sourceName),
		expressionFromWire(_statement.event_call_callee_detail, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_statement.event_call_arguments,
			[&](rust_ffi::WireAstNode const& _argument)
			{
				return astNodeFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_statement.event_call_argument_details,
			[&](rust_ffi::WireExpressionResult const& _argument)
			{
				return expressionFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<std::string>(
			_statement.event_call_parameter_names,
			[&](rust_ffi::WireString const& _parameterName)
			{
				return cppString(_parameterName);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_statement.event_call_parameter_name_locations,
			[&](rust_ffi::WireSourceLocation const& _parameterNameLocation)
			{
				return sourceLocation(_parameterNameLocation, _sourceName);
			}
		),
		astNodeFromWire(_statement.error_call, _sourceName),
		astNodeFromWire(_statement.error_call_callee, _sourceName),
		expressionFromWire(_statement.error_call_callee_detail, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_statement.error_call_arguments,
			[&](rust_ffi::WireAstNode const& _argument)
			{
				return astNodeFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_statement.error_call_argument_details,
			[&](rust_ffi::WireExpressionResult const& _argument)
			{
				return expressionFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<std::string>(
			_statement.error_call_parameter_names,
			[&](rust_ffi::WireString const& _parameterName)
			{
				return cppString(_parameterName);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_statement.error_call_parameter_name_locations,
			[&](rust_ffi::WireSourceLocation const& _parameterNameLocation)
			{
				return sourceLocation(_parameterNameLocation, _sourceName);
			}
		),
		astNodeFromWire(_statement.expression, _sourceName),
		expressionFromWire(_statement.expression_detail, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_statement.variables,
			[&](rust_ffi::WireAstNode const& _variable)
			{
				return astNodeFromWire(_variable, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_statement.variable_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _variable)
			{
				return variableDeclarationFromWire(_variable, _sourceName);
			}
		),
		astNodeFromWire(_statement.initial_value, _sourceName),
		expressionFromWire(_statement.initial_value_detail, _sourceName),
	};
}

RustParserFunctionDefinition functionDefinitionFromWire(
	rust_ffi::WireFunctionDefinitionResult const& _function,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserFunctionDefinition{
		astNodeFromWire(_function.function_definition, _sourceName),
		cppString(_function.name),
		sourceLocation(_function.name_location, _sourceName),
		_function.visibility,
		_function.state_mutability,
		_function.is_free_function,
		_function.kind,
		_function.is_virtual,
		astNodeFromWire(_function.overrides, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_function.override_paths,
			[&](rust_ffi::WireAstNode const& _overridePath)
			{
				return astNodeFromWire(_overridePath, _sourceName);
			}
		),
		cppVectorFromRust<RustParserIdentifierPath>(
			_function.override_path_details,
			[&](rust_ffi::WireIdentifierPathResult const& _overridePath)
			{
				return identifierPathFromWire(_overridePath, _sourceName);
			}
		),
		astNodeFromWire(_function.documentation, _sourceName),
		astNodeFromWire(_function.parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_function.parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_function.parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_function.modifiers,
			[&](rust_ffi::WireAstNode const& _modifier)
			{
				return astNodeFromWire(_modifier, _sourceName);
			}
		),
		cppVectorFromRust<RustParserModifierInvocation>(
			_function.modifier_details,
			[&](rust_ffi::WireModifierInvocationResult const& _modifier)
			{
				return modifierInvocationFromWire(_modifier, _sourceName);
			}
		),
		astNodeFromWire(_function.return_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_function.return_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_function.return_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_function.block, _sourceName),
		_function.block_unchecked,
		cppVectorFromRust<RustParserAstNode>(
			_function.block_statements,
			[&](rust_ffi::WireAstNode const& _statement)
			{
				return astNodeFromWire(_statement, _sourceName);
			}
		),
		cppVectorFromRust<RustParserStatement>(
			_function.block_statement_details,
			[&](rust_ffi::WireStatementResult const& _statement)
			{
				return statementFromWire(_statement, _sourceName);
			}
		),
		astNodeFromWire(_function.experimental_return_expression, _sourceName),
		expressionFromWire(_function.experimental_return_expression_detail, _sourceName),
	};
}

RustParserForAllQuantifier forAllQuantifierFromWire(
	rust_ffi::WireForAllQuantifierResult const& _quantifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserForAllQuantifier{
		astNodeFromWire(_quantifier.for_all_quantifier, _sourceName),
		astNodeFromWire(_quantifier.type_variable_declarations, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_quantifier.type_variable_declaration_parameters,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_quantifier.type_variable_declaration_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_quantifier.quantified_function, _sourceName),
		functionDefinitionFromWire(_quantifier.quantified_function_detail, _sourceName),
	};
}

RustParserModifierDefinition modifierDefinitionFromWire(
	rust_ffi::WireModifierDefinitionResult const& _modifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserModifierDefinition{
		astNodeFromWire(_modifier.modifier_definition, _sourceName),
		cppString(_modifier.name),
		sourceLocation(_modifier.name_location, _sourceName),
		astNodeFromWire(_modifier.documentation, _sourceName),
		astNodeFromWire(_modifier.parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_modifier.parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_modifier.parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_modifier.is_virtual,
		astNodeFromWire(_modifier.overrides, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_modifier.override_paths,
			[&](rust_ffi::WireAstNode const& _overridePath)
			{
				return astNodeFromWire(_overridePath, _sourceName);
			}
		),
		cppVectorFromRust<RustParserIdentifierPath>(
			_modifier.override_path_details,
			[&](rust_ffi::WireIdentifierPathResult const& _overridePath)
			{
				return identifierPathFromWire(_overridePath, _sourceName);
			}
		),
		astNodeFromWire(_modifier.block, _sourceName),
		_modifier.block_unchecked,
		cppVectorFromRust<RustParserAstNode>(
			_modifier.block_statements,
			[&](rust_ffi::WireAstNode const& _statement)
			{
				return astNodeFromWire(_statement, _sourceName);
			}
		),
		cppVectorFromRust<RustParserStatement>(
			_modifier.block_statement_details,
			[&](rust_ffi::WireStatementResult const& _statement)
			{
				return statementFromWire(_statement, _sourceName);
			}
		),
	};
}

RustParserUserDefinedValueTypeDefinition userDefinedValueTypeDefinitionFromWire(
	rust_ffi::WireUserDefinedValueTypeDefinitionResult const& _typeDefinition,
	std::shared_ptr<std::string const> const& _sourceName
);

RustParserInheritanceSpecifier inheritanceSpecifierFromWire(
	rust_ffi::WireInheritanceSpecifierResult const& _inheritance,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserInheritanceSpecifier{
		astNodeFromWire(_inheritance.inheritance_specifier, _sourceName),
		astNodeFromWire(_inheritance.base_name, _sourceName),
		cppVectorFromRust<std::string>(
			_inheritance.base_name_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_inheritance.base_name_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		_inheritance.has_arguments,
		cppVectorFromRust<RustParserAstNode>(
			_inheritance.arguments,
			[&](rust_ffi::WireAstNode const& _argument)
			{
				return astNodeFromWire(_argument, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_inheritance.argument_details,
			[&](rust_ffi::WireExpressionResult const& _argument)
			{
				return expressionFromWire(_argument, _sourceName);
			}
		),
	};
}

RustParserUsingOperator usingOperatorFromWire(rust_ffi::WireUsingOperator const& _operator)
{
	return RustParserUsingOperator{
		_operator.present,
		_operator.token,
	};
}

RustParserTypeName typeNameFromWire(
	rust_ffi::WireTypeNameResult const& _typeName,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserTypeName{
		astNodeFromWire(_typeName.type_name, _sourceName),
		_typeName.elementary_type_token,
		_typeName.elementary_type_first_number,
		_typeName.elementary_type_second_number,
		_typeName.has_state_mutability,
		_typeName.state_mutability,
		astNodeFromWire(_typeName.user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_typeName.user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_typeName.user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.array_base_types,
			[&](rust_ffi::WireAstNode const& _baseType)
			{
				return astNodeFromWire(_baseType, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.array_lengths,
			[&](rust_ffi::WireAstNode const& _length)
			{
				return astNodeFromWire(_length, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_typeName.array_length_details,
			[&](rust_ffi::WireExpressionResult const& _length)
			{
				return expressionFromWire(_length, _sourceName);
			}
		),
		astNodeFromWire(_typeName.function_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.function_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_typeName.function_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_typeName.function_return_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.function_return_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_typeName.function_return_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_typeName.function_visibility,
		_typeName.function_state_mutability,
		astNodeFromWire(_typeName.mapping_key_type, _sourceName),
		_typeName.mapping_key_elementary_token,
		_typeName.mapping_key_elementary_first_number,
		_typeName.mapping_key_elementary_second_number,
		astNodeFromWire(_typeName.mapping_key_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_typeName.mapping_key_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_typeName.mapping_key_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppString(_typeName.mapping_key_name),
		sourceLocation(_typeName.mapping_key_name_location, _sourceName),
		astNodeFromWire(_typeName.mapping_value_type, _sourceName),
		_typeName.mapping_value_elementary_token,
		_typeName.mapping_value_elementary_first_number,
		_typeName.mapping_value_elementary_second_number,
		_typeName.mapping_value_has_state_mutability,
		_typeName.mapping_value_state_mutability,
		astNodeFromWire(_typeName.mapping_value_user_defined_path_node, _sourceName),
		cppVectorFromRust<std::string>(
			_typeName.mapping_value_user_defined_path,
			[&](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		cppVectorFromRust<SourceLocation>(
			_typeName.mapping_value_user_defined_path_locations,
			[&](rust_ffi::WireSourceLocation const& _pathLocation)
			{
				return sourceLocation(_pathLocation, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.mapping_value_array_base_types,
			[&](rust_ffi::WireAstNode const& _baseType)
			{
				return astNodeFromWire(_baseType, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.mapping_value_array_lengths,
			[&](rust_ffi::WireAstNode const& _length)
			{
				return astNodeFromWire(_length, _sourceName);
			}
		),
		cppVectorFromRust<RustParserExpression>(
			_typeName.mapping_value_array_length_details,
			[&](rust_ffi::WireExpressionResult const& _length)
			{
				return expressionFromWire(_length, _sourceName);
			}
		),
		astNodeFromWire(_typeName.mapping_value_function_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.mapping_value_function_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_typeName.mapping_value_function_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_typeName.mapping_value_function_return_parameters, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeName.mapping_value_function_return_parameter_declarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_typeName.mapping_value_function_return_parameter_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_typeName.mapping_value_function_visibility,
		_typeName.mapping_value_function_state_mutability,
		cppString(_typeName.mapping_value_name),
		sourceLocation(_typeName.mapping_value_name_location, _sourceName),
		cppVectorFromRust<RustParserMappingTypeName>(
			_typeName.mapping_details,
			[&](rust_ffi::WireMappingTypeName const& _mapping)
			{
				return mappingTypeNameFromWire(_mapping, _sourceName);
			}
		),
	};
}

RustParserUsingDirective usingDirectiveFromWire(
	rust_ffi::WireUsingDirectiveResult const& _using,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserUsingDirective{
		astNodeFromWire(_using.using_directive, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_using.functions,
			[&](rust_ffi::WireAstNode const& _function)
			{
				return astNodeFromWire(_function, _sourceName);
			}
		),
		cppVectorFromRust<RustParserIdentifierPath>(
			_using.function_details,
			[&](rust_ffi::WireIdentifierPathResult const& _function)
			{
				return identifierPathFromWire(_function, _sourceName);
			}
		),
		cppVectorFromRust<RustParserUsingOperator>(
			_using.operators,
			[&](rust_ffi::WireUsingOperator const& _operator)
			{
				return usingOperatorFromWire(_operator);
			}
		),
		_using.uses_braces,
		astNodeFromWire(_using.type_name, _sourceName),
		typeNameFromWire(_using.type_name_detail, _sourceName),
		_using.global,
	};
}

RustParserContractDefinition contractDefinitionFromWire(
	rust_ffi::WireContractDefinitionResult const& _contract,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserContractDefinition{
		astNodeFromWire(_contract.contract_definition, _sourceName),
		cppString(_contract.name),
		sourceLocation(_contract.name_location, _sourceName),
		astNodeFromWire(_contract.documentation, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_contract.base_contracts,
			[&](rust_ffi::WireAstNode const& _baseContract)
			{
				return astNodeFromWire(_baseContract, _sourceName);
			}
		),
		cppVectorFromRust<RustParserInheritanceSpecifier>(
			_contract.base_contract_details,
			[&](rust_ffi::WireInheritanceSpecifierResult const& _baseContract)
			{
				return inheritanceSpecifierFromWire(_baseContract, _sourceName);
			}
		),
		cppVectorFromRust<RustParserAstNode>(
			_contract.sub_nodes,
			[&](rust_ffi::WireAstNode const& _subNode)
			{
				return astNodeFromWire(_subNode, _sourceName);
			}
		),
		cppVectorFromRust<RustParserStructDefinition>(
			_contract.sub_node_structs,
			[&](rust_ffi::WireStructDefinitionResult const& _struct)
			{
				return structDefinitionFromWire(_struct, _sourceName);
			}
		),
		cppVectorFromRust<RustParserEnumDefinition>(
			_contract.sub_node_enums,
			[&](rust_ffi::WireEnumDefinitionResult const& _enum)
			{
				return enumDefinitionFromWire(_enum, _sourceName);
			}
		),
		cppVectorFromRust<RustParserUserDefinedValueTypeDefinition>(
			_contract.sub_node_user_defined_value_types,
			[&](rust_ffi::WireUserDefinedValueTypeDefinitionResult const& _typeDefinition)
			{
				return userDefinedValueTypeDefinitionFromWire(_typeDefinition, _sourceName);
			}
		),
		cppVectorFromRust<RustParserEventDefinition>(
			_contract.sub_node_events,
			[&](rust_ffi::WireEventDefinitionResult const& _event)
			{
				return eventDefinitionFromWire(_event, _sourceName);
			}
		),
			cppVectorFromRust<RustParserErrorDefinition>(
				_contract.sub_node_errors,
				[&](rust_ffi::WireErrorDefinitionResult const& _error)
				{
					return errorDefinitionFromWire(_error, _sourceName);
				}
			),
			cppVectorFromRust<RustParserFunctionDefinition>(
				_contract.sub_node_functions,
				[&](rust_ffi::WireFunctionDefinitionResult const& _function)
				{
					return functionDefinitionFromWire(_function, _sourceName);
				}
			),
			cppVectorFromRust<RustParserModifierDefinition>(
				_contract.sub_node_modifiers,
				[&](rust_ffi::WireModifierDefinitionResult const& _modifier)
				{
					return modifierDefinitionFromWire(_modifier, _sourceName);
				}
			),
			cppVectorFromRust<RustParserUsingDirective>(
				_contract.sub_node_using_directives,
				[&](rust_ffi::WireUsingDirectiveResult const& _using)
				{
					return usingDirectiveFromWire(_using, _sourceName);
				}
			),
			cppVectorFromRust<RustParserVariableDeclaration>(
				_contract.sub_node_variable_declarations,
				[&](rust_ffi::WireVariableDeclarationResult const& _variable)
				{
					return variableDeclarationFromWire(_variable, _sourceName);
				}
			),
			_contract.contract_kind,
			_contract.is_abstract,
			astNodeFromWire(_contract.storage_layout_specifier, _sourceName),
			astNodeFromWire(_contract.storage_layout_base_slot_expression, _sourceName),
			expressionFromWire(_contract.storage_layout_base_slot_expression_detail, _sourceName),
	};
}

RustParserUserDefinedValueTypeDefinition userDefinedValueTypeDefinitionFromWire(
	rust_ffi::WireUserDefinedValueTypeDefinitionResult const& _typeDefinition,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserUserDefinedValueTypeDefinition{
		astNodeFromWire(_typeDefinition.user_defined_value_type_definition, _sourceName),
		cppString(_typeDefinition.name),
		sourceLocation(_typeDefinition.name_location, _sourceName),
		astNodeFromWire(_typeDefinition.type_name, _sourceName),
		_typeDefinition.type_name_elementary_token,
		_typeDefinition.type_name_elementary_first_number,
		_typeDefinition.type_name_elementary_second_number,
		_typeDefinition.type_name_has_state_mutability,
		_typeDefinition.type_name_state_mutability,
		typeNameFromWire(_typeDefinition.type_name_detail, _sourceName),
	};
}

RustParserTypeDefinition typeDefinitionFromWire(
	rust_ffi::WireTypeDefinitionResult const& _typeDefinition,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserTypeDefinition{
		astNodeFromWire(_typeDefinition.type_definition, _sourceName),
		cppString(_typeDefinition.name),
		sourceLocation(_typeDefinition.name_location, _sourceName),
		astNodeFromWire(_typeDefinition.arguments, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeDefinition.argument_parameters,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_typeDefinition.argument_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_typeDefinition.expression, _sourceName),
		expressionFromWire(_typeDefinition.expression_detail, _sourceName),
		_typeDefinition.has_builtin_name_parameter,
		cppString(_typeDefinition.builtin_name_parameter),
		sourceLocation(_typeDefinition.builtin_name_parameter_location, _sourceName),
	};
}

RustParserTypeClassDefinition typeClassDefinitionFromWire(
	rust_ffi::WireTypeClassDefinitionResult const& _typeClassDefinition,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserTypeClassDefinition{
		astNodeFromWire(_typeClassDefinition.type_class_definition, _sourceName),
		astNodeFromWire(_typeClassDefinition.type_variable, _sourceName),
		cppString(_typeClassDefinition.type_variable_name),
		sourceLocation(_typeClassDefinition.type_variable_name_location, _sourceName),
		cppString(_typeClassDefinition.name),
		sourceLocation(_typeClassDefinition.name_location, _sourceName),
		astNodeFromWire(_typeClassDefinition.documentation, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeClassDefinition.sub_nodes,
			[&](rust_ffi::WireAstNode const& _node)
			{
				return astNodeFromWire(_node, _sourceName);
			}
		),
		cppVectorFromRust<RustParserFunctionDefinition>(
			_typeClassDefinition.sub_node_function_details,
			[&](rust_ffi::WireFunctionDefinitionResult const& _function)
			{
				return functionDefinitionFromWire(_function, _sourceName);
			}
		),
	};
}

RustParserTypeClassName typeClassNameFromWire(
	rust_ffi::WireTypeClassNameResult const& _typeClassName,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserTypeClassName{
		astNodeFromWire(_typeClassName.type_class_name, _sourceName),
		_typeClassName.is_builtin,
		_typeClassName.builtin_token,
		astNodeFromWire(_typeClassName.identifier_path, _sourceName),
		identifierPathFromWire(_typeClassName.identifier_path_detail, _sourceName),
	};
}

RustParserTypeClassInstantiation typeClassInstantiationFromWire(
	rust_ffi::WireTypeClassInstantiationResult const& _typeClassInstantiation,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserTypeClassInstantiation{
		astNodeFromWire(_typeClassInstantiation.type_class_instantiation, _sourceName),
		astNodeFromWire(_typeClassInstantiation.type_constructor, _sourceName),
		typeNameFromWire(_typeClassInstantiation.type_constructor_detail, _sourceName),
		astNodeFromWire(_typeClassInstantiation.argument_sorts, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeClassInstantiation.argument_sort_parameters,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_typeClassInstantiation.argument_sort_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		astNodeFromWire(_typeClassInstantiation.type_class_name, _sourceName),
		typeClassNameFromWire(_typeClassInstantiation.type_class_name_detail, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_typeClassInstantiation.sub_nodes,
			[&](rust_ffi::WireAstNode const& _node)
			{
				return astNodeFromWire(_node, _sourceName);
			}
		),
		cppVectorFromRust<RustParserFunctionDefinition>(
			_typeClassInstantiation.sub_node_function_details,
			[&](rust_ffi::WireFunctionDefinitionResult const& _function)
			{
				return functionDefinitionFromWire(_function, _sourceName);
			}
		),
	};
}

ASTPointer<StructuredDocumentation> createStructuredDocumentationAstFromRust(RustParserAstNode const& _documentation)
{
	if (!_documentation.present)
		return nullptr;
	if (_documentation.kind != rustAstNodeKindStructuredDocumentation)
		return nullptr;

	return std::make_shared<StructuredDocumentation>(
		_documentation.nodeID,
		_documentation.location,
		astString(_documentation.text)
	);
}

bool structuredDocumentationIsSupported(RustParserAstNode const& _documentation)
{
	return !_documentation.present || _documentation.kind == rustAstNodeKindStructuredDocumentation;
}

enum class RustParserParameterContext
{
	Default,
	Event,
	LocationAllowed
};

bool rustParserExperimentalMode()
{
	if (RustParserReconstructionContext const* context = currentRustParserReconstructionContext)
		return context->experimentalSolidity;
	return false;
}

bool variableDeclarationHasNoOverrideData(RustParserVariableDeclaration const& _variable)
{
	return !_variable.overrides.present && _variable.overridePaths.empty() && _variable.overridePathDetails.empty();
}

bool variableDeclarationHasNoValueData(RustParserVariableDeclaration const& _variable)
{
	return !_variable.value.present && !_variable.valueDetail.node.present;
}

bool variableDeclarationHasDefaultVisibility(RustParserVariableDeclaration const& _variable)
{
	return _variable.visibility == rustVisibilityDefault;
}

bool variableDeclarationHasUnspecifiedLocation(RustParserVariableDeclaration const& _variable)
{
	return _variable.variableLocation == rustVariableDeclarationLocationUnspecified;
}

bool variableDeclarationIsParameterContextCompatible(
	RustParserVariableDeclaration const& _variable,
	RustParserParameterContext _context
)
{
	bool const experimentalMode = rustParserExperimentalMode();
	bool const allowIndexed = _context == RustParserParameterContext::Event && !experimentalMode;
	bool const allowLocationSpecifier = _context == RustParserParameterContext::LocationAllowed && !experimentalMode;
	if (!allowIndexed && _variable.indexed)
		return false;
	if (!allowLocationSpecifier && !variableDeclarationHasUnspecifiedLocation(_variable))
		return false;
	if (_variable.variableLocation == rustVariableDeclarationLocationTransient)
		return false;

	return
		variableDeclarationHasNoOverrideData(_variable) &&
		variableDeclarationHasNoValueData(_variable) &&
		variableDeclarationHasDefaultVisibility(_variable);
}

bool variableDeclarationIsFileLevelCompatible(RustParserVariableDeclaration const& _variable)
{
	return
		!_variable.indexed &&
		variableDeclarationHasNoOverrideData(_variable) &&
		variableDeclarationHasDefaultVisibility(_variable) &&
		variableDeclarationHasUnspecifiedLocation(_variable);
}

bool variableDeclarationIsStateVariableCompatible(RustParserVariableDeclaration const& _variable)
{
	return
		!_variable.indexed &&
		(
			_variable.variableLocation == rustVariableDeclarationLocationUnspecified ||
			_variable.variableLocation == rustVariableDeclarationLocationTransient
		);
}

bool variableDeclarationIsStructMemberCompatible(RustParserVariableDeclaration const& _variable)
{
	return
		!_variable.name.empty() &&
		variableDeclarationIsParameterContextCompatible(_variable, RustParserParameterContext::Default);
}

bool variableDeclarationIsLocalStatementCompatible(RustParserVariableDeclaration const& _variable)
{
	if (
		_variable.indexed ||
		!variableDeclarationHasNoOverrideData(_variable) ||
		!variableDeclarationHasNoValueData(_variable) ||
		!variableDeclarationHasDefaultVisibility(_variable)
	)
		return false;
	if (_variable.variableLocation == rustVariableDeclarationLocationTransient)
		return false;
	if (rustParserExperimentalMode() && !variableDeclarationHasUnspecifiedLocation(_variable))
		return false;

	return true;
}

ASTPointer<Expression> createExpressionAstFromRust(
	RustParserAstNode const& _expressionNode,
	RustParserExpression const& _expression
);

ASTPointer<ParameterList> createParameterListAstFromRust(
	RustParserAstNode const& _parameterList,
	std::vector<RustParserAstNode> const& _parameterDeclarations,
	std::vector<RustParserVariableDeclaration> const& _parameterDetails,
	RustParserParameterContext _context = RustParserParameterContext::Default
)
{
	if (!_parameterList.present || _parameterList.kind != rustAstNodeKindParameterList)
		return nullptr;
	if (_parameterDeclarations.size() != _parameterDetails.size())
		return nullptr;

	std::vector<ASTPointer<VariableDeclaration>> parameters;
	parameters.reserve(_parameterDeclarations.size());
	for (RustParserAstNode const& parameter: _parameterDeclarations)
	{
		auto parameterDetail = std::find_if(
			_parameterDetails.begin(),
			_parameterDetails.end(),
			[&](RustParserVariableDeclaration const& _parameter)
			{
				return rustAstNodesMatch(_parameter.node, parameter);
			}
		);
		if (parameterDetail == _parameterDetails.end())
			return nullptr;
		if (!variableDeclarationIsParameterContextCompatible(*parameterDetail, _context))
			return nullptr;

		parameters.push_back(createVariableDeclarationAstFromRust(*parameterDetail));
		if (!parameters.back())
			return nullptr;
	}

	return std::make_shared<ParameterList>(
		_parameterList.nodeID,
		_parameterList.location,
		std::move(parameters)
	);
}

ASTPointer<EnumValue> createEnumValueAstFromRust(RustParserEnumValue const& _value)
{
	if (!_value.node.present || _value.node.kind != rustAstNodeKindEnumValue)
		return nullptr;
	if (!structuredDocumentationIsSupported(_value.documentation))
		return nullptr;

	return std::make_shared<EnumValue>(
		_value.node.nodeID,
		_value.node.location,
		astString(_value.name),
		createStructuredDocumentationAstFromRust(_value.documentation)
	);
}

ASTPointer<ElementaryTypeName> createElementaryTypeNameAstFromRust(
	RustParserAstNode const& _typeName,
	std::uint32_t _elementaryToken,
	std::uint32_t _firstNumber,
	std::uint32_t _secondNumber,
	bool _hasStateMutability,
	std::uint8_t _stateMutability
)
{
	if (!_typeName.present || _typeName.kind != rustAstNodeKindElementaryTypeName)
		return nullptr;

	Token token = static_cast<Token>(_elementaryToken);
	if (!elementaryTypeSizingIsValid(token, _firstNumber, _secondNumber))
		return nullptr;

	std::optional<StateMutability> stateMutability;
	if (_hasStateMutability)
	{
		stateMutability = stateMutabilityFromRust(_stateMutability);
		if (!stateMutability.has_value() || token != Token::Address)
			return nullptr;
	}

	return std::make_shared<ElementaryTypeName>(
		_typeName.nodeID,
		_typeName.location,
		ElementaryTypeNameToken(token, _firstNumber, _secondNumber),
		stateMutability
	);
}

ASTPointer<IdentifierPath> createIdentifierPathAstFromRust(
	RustParserAstNode const& _pathNode,
	std::vector<std::string> const& _path,
	std::vector<SourceLocation> const& _pathLocations
)
{
	if (!_pathNode.present || _pathNode.kind != rustAstNodeKindIdentifierPath)
		return nullptr;
	if (_path.empty() || _path.size() != _pathLocations.size())
		return nullptr;

	std::vector<ASTString> path;
	path.reserve(_path.size());
	for (std::string const& component: _path)
		path.emplace_back(component);

	return std::make_shared<IdentifierPath>(
		_pathNode.nodeID,
		_pathNode.location,
		std::move(path),
		_pathLocations
	);
}

ASTPointer<UserDefinedTypeName> createUserDefinedTypeNameAstFromRust(
	RustParserAstNode const& _typeName,
	RustParserAstNode const& _pathNode,
	std::vector<std::string> const& _path,
	std::vector<SourceLocation> const& _pathLocations
)
{
	if (!_typeName.present || _typeName.kind != rustAstNodeKindUserDefinedTypeName)
		return nullptr;

	ASTPointer<IdentifierPath> path = createIdentifierPathAstFromRust(_pathNode, _path, _pathLocations);
	if (!path)
		return nullptr;

	return std::make_shared<UserDefinedTypeName>(
		_typeName.nodeID,
		_typeName.location,
		std::move(path)
	);
}

ASTPointer<TypeName> createNonArrayTypeNameAstFromRust(
	RustParserAstNode const& _typeName,
	std::uint32_t _elementaryToken,
	std::uint32_t _firstNumber,
	std::uint32_t _secondNumber,
	bool _hasStateMutability,
	std::uint8_t _stateMutability,
	RustParserAstNode const& _userDefinedPathNode,
	std::vector<std::string> const& _userDefinedPath,
	std::vector<SourceLocation> const& _userDefinedPathLocations
)
{
	if (_typeName.kind == rustAstNodeKindElementaryTypeName)
		return createElementaryTypeNameAstFromRust(
			_typeName,
			_elementaryToken,
			_firstNumber,
			_secondNumber,
			_hasStateMutability,
			_stateMutability
		);
	if (_typeName.kind == rustAstNodeKindUserDefinedTypeName)
		return createUserDefinedTypeNameAstFromRust(
			_typeName,
			_userDefinedPathNode,
			_userDefinedPath,
			_userDefinedPathLocations
		);
	return nullptr;
}

ASTPointer<FunctionTypeName> createFunctionTypeNameAstFromRust(
	RustParserAstNode const& _typeName,
	RustParserAstNode const& _parameters,
	std::vector<RustParserAstNode> const& _parameterDeclarations,
	std::vector<RustParserVariableDeclaration> const& _parameterDetails,
	RustParserAstNode const& _returnParameters,
	std::vector<RustParserAstNode> const& _returnParameterDeclarations,
	std::vector<RustParserVariableDeclaration> const& _returnParameterDetails,
	std::uint8_t _visibility,
	std::uint8_t _stateMutability
)
{
	if (!_typeName.present || _typeName.kind != rustAstNodeKindFunctionTypeName)
		return nullptr;

	ASTPointer<ParameterList> parameters = createParameterListAstFromRust(
		_parameters,
		_parameterDeclarations,
		_parameterDetails,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<ParameterList> returnParameters = createParameterListAstFromRust(
		_returnParameters,
		_returnParameterDeclarations,
		_returnParameterDetails,
		RustParserParameterContext::LocationAllowed
	);
	if (!returnParameters)
		return nullptr;

	std::optional<Visibility> visibility = visibilityFromRust(_visibility);
	std::optional<StateMutability> stateMutability = stateMutabilityFromRust(_stateMutability);
	if (!visibility || !stateMutability)
		return nullptr;

	return std::make_shared<FunctionTypeName>(
		_typeName.nodeID,
		_typeName.location,
		std::move(parameters),
		std::move(returnParameters),
		*visibility,
		*stateMutability
	);
}

ASTPointer<TypeName> createTypeNameAstFromRust(
	RustParserAstNode const& _typeName,
	std::uint32_t _elementaryToken,
	std::uint32_t _firstNumber,
	std::uint32_t _secondNumber,
	bool _hasStateMutability,
	std::uint8_t _stateMutability,
	RustParserAstNode const& _userDefinedPathNode,
	std::vector<std::string> const& _userDefinedPath,
	std::vector<SourceLocation> const& _userDefinedPathLocations,
	RustParserAstNode const& _functionParameters,
	std::vector<RustParserAstNode> const& _functionParameterDeclarations,
	std::vector<RustParserVariableDeclaration> const& _functionParameterDetails,
	RustParserAstNode const& _functionReturnParameters,
	std::vector<RustParserAstNode> const& _functionReturnParameterDeclarations,
	std::vector<RustParserVariableDeclaration> const& _functionReturnParameterDetails,
	std::uint8_t _functionVisibility,
	std::uint8_t _functionStateMutability,
	std::vector<RustParserAstNode> const& _arrayBaseTypes,
	std::vector<RustParserAstNode> const& _arrayLengths,
	std::vector<RustParserExpression> const& _arrayLengthDetails,
	std::function<ASTPointer<TypeName>(RustParserAstNode const&)> const& _createMappingTypeName
)
{
	if (_typeName.kind != rustAstNodeKindArrayTypeName)
	{
		if (_typeName.kind == rustAstNodeKindFunctionTypeName)
			return createFunctionTypeNameAstFromRust(
				_typeName,
				_functionParameters,
				_functionParameterDeclarations,
				_functionParameterDetails,
				_functionReturnParameters,
				_functionReturnParameterDeclarations,
				_functionReturnParameterDetails,
				_functionVisibility,
				_functionStateMutability
			);
		if (_typeName.kind == rustAstNodeKindMapping)
			return _createMappingTypeName(_typeName);

		return createNonArrayTypeNameAstFromRust(
			_typeName,
			_elementaryToken,
			_firstNumber,
			_secondNumber,
			_hasStateMutability,
			_stateMutability,
			_userDefinedPathNode,
			_userDefinedPath,
			_userDefinedPathLocations
		);
	}

	if (
		_arrayBaseTypes.empty() ||
		_arrayBaseTypes.size() > rustBridgeMaxArrayTypeDepth ||
		_arrayBaseTypes.size() != _arrayLengths.size() ||
		_arrayBaseTypes.size() != _arrayLengthDetails.size()
	)
		return nullptr;

	ASTPointer<TypeName> currentType;
	if (_arrayBaseTypes.front().kind == rustAstNodeKindFunctionTypeName)
		currentType = createFunctionTypeNameAstFromRust(
			_arrayBaseTypes.front(),
			_functionParameters,
			_functionParameterDeclarations,
			_functionParameterDetails,
			_functionReturnParameters,
			_functionReturnParameterDeclarations,
			_functionReturnParameterDetails,
			_functionVisibility,
			_functionStateMutability
		);
	else if (_arrayBaseTypes.front().kind == rustAstNodeKindMapping)
		currentType = _createMappingTypeName(_arrayBaseTypes.front());
	else
		currentType = createNonArrayTypeNameAstFromRust(
			_arrayBaseTypes.front(),
			_elementaryToken,
			_firstNumber,
			_secondNumber,
			_hasStateMutability,
			_stateMutability,
			_userDefinedPathNode,
			_userDefinedPath,
			_userDefinedPathLocations
		);
	if (!currentType)
		return nullptr;

	for (size_t index = 0; index < _arrayBaseTypes.size(); ++index)
	{
		ASTPointer<Expression> length;
		if (_arrayLengths[index].present)
		{
			length = createExpressionAstFromRust(_arrayLengths[index], _arrayLengthDetails[index]);
			if (!length)
				return nullptr;
		}
		else if (_arrayLengthDetails[index].node.present)
			return nullptr;

		RustParserAstNode const& arrayType =
			index + 1 < _arrayBaseTypes.size() ? _arrayBaseTypes[index + 1] : _typeName;
		if (!arrayType.present || arrayType.kind != rustAstNodeKindArrayTypeName)
			return nullptr;

		currentType = std::make_shared<ArrayTypeName>(
			arrayType.nodeID,
			arrayType.location,
			std::move(currentType),
			std::move(length)
		);
	}

	return currentType;
}

RustParserMappingTypeName const* findMappingTypeNameDetail(
	RustParserAstNode const& _mapping,
	std::vector<RustParserMappingTypeName> const& _mappingDetails
)
{
	for (RustParserMappingTypeName const& mappingDetail: _mappingDetails)
		if (rustAstNodesMatch(mappingDetail.mapping, _mapping))
			return &mappingDetail;
	return nullptr;
}

ASTPointer<Mapping> createMappingTypeNameAstFromRust(
	RustParserAstNode const& _mapping,
	std::vector<RustParserMappingTypeName> const& _mappingDetails
)
{
	if (!_mapping.present || _mapping.kind != rustAstNodeKindMapping)
		return nullptr;

	RustParserMappingTypeName const* mappingDetail = findMappingTypeNameDetail(_mapping, _mappingDetails);
	if (!mappingDetail)
		return nullptr;

	ASTPointer<TypeName> keyType = createNonArrayTypeNameAstFromRust(
		mappingDetail->keyType,
		mappingDetail->keyElementaryToken,
		mappingDetail->keyElementaryFirstNumber,
		mappingDetail->keyElementarySecondNumber,
		false,
		rustStateMutabilityNonPayable,
		mappingDetail->keyUserDefinedPathNode,
		mappingDetail->keyUserDefinedPath,
		mappingDetail->keyUserDefinedPathLocations
	);
	if (!keyType)
		return nullptr;

	std::function<ASTPointer<TypeName>(RustParserAstNode const&)> createMappingTypeName =
		[&](RustParserAstNode const& _nestedMapping) -> ASTPointer<TypeName>
		{
			return createMappingTypeNameAstFromRust(_nestedMapping, _mappingDetails);
		};

	ASTPointer<TypeName> valueType = createTypeNameAstFromRust(
		mappingDetail->valueType,
		mappingDetail->valueElementaryToken,
		mappingDetail->valueElementaryFirstNumber,
		mappingDetail->valueElementarySecondNumber,
		mappingDetail->valueHasStateMutability,
		mappingDetail->valueStateMutability,
		mappingDetail->valueUserDefinedPathNode,
		mappingDetail->valueUserDefinedPath,
		mappingDetail->valueUserDefinedPathLocations,
		mappingDetail->valueFunctionParameters,
		mappingDetail->valueFunctionParameterDeclarations,
		mappingDetail->valueFunctionParameterDetails,
		mappingDetail->valueFunctionReturnParameters,
		mappingDetail->valueFunctionReturnParameterDeclarations,
		mappingDetail->valueFunctionReturnParameterDetails,
		mappingDetail->valueFunctionVisibility,
		mappingDetail->valueFunctionStateMutability,
		mappingDetail->valueArrayBaseTypes,
		mappingDetail->valueArrayLengths,
		mappingDetail->valueArrayLengthDetails,
		createMappingTypeName
	);
	if (!valueType)
		return nullptr;

	return std::make_shared<Mapping>(
		_mapping.nodeID,
		_mapping.location,
		std::move(keyType),
		astString(mappingDetail->keyName),
		mappingDetail->keyNameLocation,
		std::move(valueType),
		astString(mappingDetail->valueName),
		mappingDetail->valueNameLocation
	);
}

ASTPointer<TypeName> createTypeNameAstFromRust(RustParserTypeName const& _typeName)
{
	if (!_typeName.node.present)
		return nullptr;

	auto createMappingTypeName = [&](RustParserAstNode const& _mapping) -> ASTPointer<TypeName>
	{
		return createMappingTypeNameAstFromRust(_mapping, _typeName.mappingDetails);
	};

	if (_typeName.node.kind == rustAstNodeKindMapping)
		return createMappingTypeName(_typeName.node);

	return createTypeNameAstFromRust(
		_typeName.node,
		_typeName.elementaryToken,
		_typeName.elementaryFirstNumber,
		_typeName.elementarySecondNumber,
		_typeName.hasStateMutability,
		_typeName.stateMutability,
		_typeName.userDefinedPathNode,
		_typeName.userDefinedPath,
		_typeName.userDefinedPathLocations,
		_typeName.functionParameters,
		_typeName.functionParameterDeclarations,
		_typeName.functionParameterDetails,
		_typeName.functionReturnParameters,
		_typeName.functionReturnParameterDeclarations,
		_typeName.functionReturnParameterDetails,
		_typeName.functionVisibility,
		_typeName.functionStateMutability,
		_typeName.arrayBaseTypes,
		_typeName.arrayLengths,
		_typeName.arrayLengthDetails,
		createMappingTypeName
	);
}

}

ASTPointer<PragmaDirective> solidity::frontend::createPragmaDirectiveAstFromRust(
	RustParserPragmaDirective const& _pragma
)
{
	if (!_pragma.node.present || _pragma.node.kind != rustAstNodeKindPragmaDirective)
		return nullptr;
	if (_pragma.tokens.size() != _pragma.literals.size())
		return nullptr;
	if (!pragmaTokensAreValid(_pragma.tokens))
		return nullptr;

	return std::make_shared<PragmaDirective>(
		_pragma.node.nodeID,
		_pragma.node.location,
		_pragma.tokens,
		_pragma.literals
	);
}

ASTPointer<ImportDirective> solidity::frontend::createImportDirectiveAstFromRust(
	RustParserImportDirective const& _import
)
{
	if (!_import.node.present || _import.node.kind != rustAstNodeKindImportDirective)
		return nullptr;
	if (_import.path.empty())
		return nullptr;
	if (!_import.symbolAliases.empty() && !_import.unitAlias.empty())
		return nullptr;

	ImportDirective::SymbolAliasList symbolAliases;
	symbolAliases.reserve(_import.symbolAliases.size());
	for (RustParserImportSymbolAlias const& symbolAlias: _import.symbolAliases)
	{
		if (!symbolAlias.symbol.present || symbolAlias.symbol.kind != rustAstNodeKindIdentifier || symbolAlias.symbol.text.empty())
			return nullptr;
		if (symbolAlias.hasAlias == symbolAlias.alias.empty())
			return nullptr;

		symbolAliases.push_back(ImportDirective::SymbolAlias{
			std::make_shared<Identifier>(
				symbolAlias.symbol.nodeID,
				symbolAlias.symbol.location,
				astString(symbolAlias.symbol.text)
			),
			symbolAlias.hasAlias ? astString(symbolAlias.alias) : nullptr,
			symbolAlias.location
		});
	}

	return std::make_shared<ImportDirective>(
		_import.node.nodeID,
		_import.node.location,
		astString(_import.path),
		astString(_import.unitAlias),
		_import.unitAliasLocation,
		std::move(symbolAliases)
	);
}

ASTPointer<EnumDefinition> solidity::frontend::createEnumDefinitionAstFromRust(
	RustParserEnumDefinition const& _enum
)
{
	if (!_enum.node.present || _enum.node.kind != rustAstNodeKindEnumDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_enum.documentation))
		return nullptr;

	std::vector<ASTPointer<EnumValue>> members;
	members.reserve(_enum.members.size());
	for (RustParserEnumValue const& value: _enum.members)
	{
		members.push_back(createEnumValueAstFromRust(value));
		if (!members.back())
			return nullptr;
	}

	return std::make_shared<EnumDefinition>(
		_enum.node.nodeID,
		_enum.node.location,
		astString(_enum.name),
		_enum.nameLocation,
		std::move(members),
		createStructuredDocumentationAstFromRust(_enum.documentation)
	);
}

namespace
{

ASTPointer<Expression> createIdentifierExpressionAstFromRust(RustParserAstNode const& _expression)
{
	if (!_expression.present || _expression.kind != rustAstNodeKindIdentifier)
		return nullptr;
	if (_expression.text.empty())
		return nullptr;

	return std::make_shared<Identifier>(
		_expression.nodeID,
		_expression.location,
		astString(_expression.text)
	);
}

ASTPointer<Expression> createExpressionAstFromRust(
	RustParserAstNode const& _expressionNode,
	RustParserExpression const& _expression
);

ASTPointer<Expression> createSingleChildExpressionAstFromRust(
	RustParserAstNode const& _expressionNode,
	std::vector<RustParserExpression> const& _expressionDetails
)
{
	if (!_expressionNode.present || _expressionDetails.size() != 1)
		return nullptr;
	return createExpressionAstFromRust(_expressionNode, _expressionDetails.front());
}

bool expressionIsIdentifierPathFromRust(
	RustParserAstNode const& _expressionNode,
	RustParserExpression const& _expression
)
{
	if (!_expressionNode.present || !_expression.node.present)
		return false;
	if (!rustAstNodesMatch(_expressionNode, _expression.node))
		return false;
	if (_expression.node.kind == rustAstNodeKindIdentifier)
		return !_expression.node.text.empty();
	if (_expression.node.kind != rustAstNodeKindMemberAccess || _expression.node.text.empty())
		return false;
	if (_expression.baseExpressionDetail.size() != 1)
		return false;
	return expressionIsIdentifierPathFromRust(
		_expression.baseExpression,
		_expression.baseExpressionDetail.front()
	);
}

std::optional<Token> scanSingleTokenFromRust(std::string const& _tokenText)
{
	CharStream charStream(_tokenText, "");
	Scanner scanner{charStream};
	if (scanner.peekNextToken() != Token::EOS)
		return std::nullopt;
	return scanner.currentToken();
}

ASTPointer<ElementaryTypeName> createElementaryTypeNameFromExpressionTypeAst(
	RustParserAstNode const& _expressionType
)
{
	if (!_expressionType.present || _expressionType.kind != rustAstNodeKindElementaryTypeName)
		return nullptr;

	Token token;
	unsigned int firstNumber = 0;
	unsigned int secondNumber = 0;
	std::optional<StateMutability> stateMutability;
	if (_expressionType.text == "address payable")
	{
		token = Token::Address;
		stateMutability = StateMutability::Payable;
	}
	else
		std::tie(token, firstNumber, secondNumber) = TokenTraits::fromIdentifierOrKeyword(_expressionType.text);

	if (!TokenTraits::isElementaryTypeName(token))
		return nullptr;

	return std::make_shared<ElementaryTypeName>(
		_expressionType.nodeID,
		_expressionType.location,
		ElementaryTypeNameToken(token, firstNumber, secondNumber),
		stateMutability
	);
}

ASTPointer<Expression> createExpressionAstFromRust(
	RustParserAstNode const& _expressionNode,
	RustParserExpression const& _expression
)
{
	if (!_expressionNode.present || !_expression.node.present)
		return nullptr;
	if (!rustAstNodesMatch(_expressionNode, _expression.node))
		return nullptr;

	if (_expression.node.kind == rustAstNodeKindIdentifier)
		return createIdentifierExpressionAstFromRust(_expression.node);

	if (_expression.node.kind == rustAstNodeKindElementaryTypeNameExpression)
	{
		ASTPointer<ElementaryTypeName> typeName = createElementaryTypeNameFromExpressionTypeAst(
			_expression.expressionType
		);
		if (!typeName)
			return nullptr;

		return std::make_shared<ElementaryTypeNameExpression>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(typeName)
		);
	}

	if (_expression.node.kind == rustAstNodeKindAssignment)
	{
		std::optional<Token> assignmentOperator = scanSingleTokenFromRust(_expression.node.text);
		if (!assignmentOperator || !TokenTraits::isAssignmentOp(*assignmentOperator))
			return nullptr;

		ASTPointer<Expression> leftExpression = createSingleChildExpressionAstFromRust(
			_expression.leftExpression,
			_expression.leftExpressionDetail
		);
		if (!leftExpression)
			return nullptr;

		ASTPointer<Expression> rightExpression = createSingleChildExpressionAstFromRust(
			_expression.rightExpression,
			_expression.rightExpressionDetail
		);
		if (!rightExpression)
			return nullptr;

		return std::make_shared<Assignment>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(leftExpression),
			*assignmentOperator,
			std::move(rightExpression)
		);
	}

	if (_expression.node.kind == rustAstNodeKindConditional)
	{
		ASTPointer<Expression> conditionExpression = createSingleChildExpressionAstFromRust(
			_expression.conditionExpression,
			_expression.conditionExpressionDetail
		);
		if (!conditionExpression)
			return nullptr;

		ASTPointer<Expression> trueExpression = createSingleChildExpressionAstFromRust(
			_expression.trueExpression,
			_expression.trueExpressionDetail
		);
		if (!trueExpression)
			return nullptr;

		ASTPointer<Expression> falseExpression = createSingleChildExpressionAstFromRust(
			_expression.falseExpression,
			_expression.falseExpressionDetail
		);
		if (!falseExpression)
			return nullptr;

		return std::make_shared<Conditional>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(conditionExpression),
			std::move(trueExpression),
			std::move(falseExpression)
		);
	}

	if (_expression.node.kind == rustAstNodeKindBinaryOperation)
	{
		std::optional<Token> operation = scanSingleTokenFromRust(_expression.node.text);
		if (
			!operation ||
			!(
				TokenTraits::isBinaryOp(*operation) ||
				TokenTraits::isCompareOp(*operation) ||
				*operation == Token::Colon ||
				*operation == Token::RightArrow
			)
		)
			return nullptr;

		ASTPointer<Expression> leftExpression = createSingleChildExpressionAstFromRust(
			_expression.leftExpression,
			_expression.leftExpressionDetail
		);
		if (!leftExpression)
			return nullptr;

		ASTPointer<Expression> rightExpression = createSingleChildExpressionAstFromRust(
			_expression.rightExpression,
			_expression.rightExpressionDetail
		);
		if (!rightExpression)
			return nullptr;

		return std::make_shared<BinaryOperation>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(leftExpression),
			*operation,
			std::move(rightExpression)
		);
	}

	if (_expression.node.kind == rustAstNodeKindUnaryOperation)
	{
		std::optional<Token> operation = scanSingleTokenFromRust(_expression.node.text);
		if (!operation || !TokenTraits::isUnaryOp(*operation))
			return nullptr;

		ASTPointer<Expression> subExpression = createSingleChildExpressionAstFromRust(
			_expression.subExpression,
			_expression.subExpressionDetail
		);
		if (!subExpression)
			return nullptr;

		return std::make_shared<UnaryOperation>(
			_expression.node.nodeID,
			_expression.node.location,
			*operation,
			std::move(subExpression),
			_expression.isPrefixOperation
		);
	}

	if (_expression.node.kind == rustAstNodeKindTupleExpression || _expression.node.kind == rustAstNodeKindInlineArrayExpression)
	{
		if ((_expression.node.kind == rustAstNodeKindInlineArrayExpression) != _expression.isInlineArray)
			return nullptr;
		if (_expression.components.size() != _expression.componentDetails.size())
			return nullptr;

		std::vector<ASTPointer<Expression>> components;
		components.reserve(_expression.components.size());
		for (size_t i = 0; i < _expression.components.size(); ++i)
		{
			if (!_expression.components[i].present)
			{
				if (_expression.isInlineArray)
					return nullptr;
				if (_expression.componentDetails[i].node.present)
					return nullptr;
				components.push_back(nullptr);
				continue;
			}

			components.push_back(createExpressionAstFromRust(
				_expression.components[i],
				_expression.componentDetails[i]
			));
			if (!components.back())
				return nullptr;
		}

		return std::make_shared<TupleExpression>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(components),
			_expression.isInlineArray
		);
	}

	if (_expression.node.kind == rustAstNodeKindNewExpression)
	{
		if (!_expression.typeName.present || !_expression.typeNameDetail)
			return nullptr;
		if (!rustAstNodesMatch(_expression.typeName, _expression.typeNameDetail->node))
			return nullptr;

		ASTPointer<TypeName> typeName = createTypeNameAstFromRust(*_expression.typeNameDetail);
		if (!typeName)
			return nullptr;

		return std::make_shared<NewExpression>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(typeName)
		);
	}

	if (_expression.node.kind == rustAstNodeKindIndexAccess)
	{
		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRust(
			_expression.baseExpression,
			_expression.baseExpressionDetail
		);
		if (!baseExpression)
			return nullptr;

		ASTPointer<Expression> indexExpression;
		if (_expression.indexExpression.present)
		{
			indexExpression = createSingleChildExpressionAstFromRust(
				_expression.indexExpression,
				_expression.indexExpressionDetail
			);
			if (!indexExpression)
				return nullptr;
		}
		else if (!_expression.indexExpressionDetail.empty())
			return nullptr;

		return std::make_shared<IndexAccess>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(baseExpression),
			std::move(indexExpression)
		);
	}

	if (_expression.node.kind == rustAstNodeKindIndexRangeAccess)
	{
		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRust(
			_expression.baseExpression,
			_expression.baseExpressionDetail
		);
		if (!baseExpression)
			return nullptr;

		ASTPointer<Expression> startExpression;
		if (_expression.indexExpression.present)
		{
			startExpression = createSingleChildExpressionAstFromRust(
				_expression.indexExpression,
				_expression.indexExpressionDetail
			);
			if (!startExpression)
				return nullptr;
		}
		else if (!_expression.indexExpressionDetail.empty())
			return nullptr;

		ASTPointer<Expression> endExpression;
		if (_expression.endIndexExpression.present)
		{
			endExpression = createSingleChildExpressionAstFromRust(
				_expression.endIndexExpression,
				_expression.endIndexExpressionDetail
			);
			if (!endExpression)
				return nullptr;
		}
		else if (!_expression.endIndexExpressionDetail.empty())
			return nullptr;

		return std::make_shared<IndexRangeAccess>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(baseExpression),
			std::move(startExpression),
			std::move(endExpression)
		);
	}

	if (_expression.node.kind == rustAstNodeKindMemberAccess)
	{
		if (_expression.node.text.empty())
			return nullptr;

		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRust(
			_expression.baseExpression,
			_expression.baseExpressionDetail
		);
		if (!baseExpression)
			return nullptr;

		return std::make_shared<MemberAccess>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(baseExpression),
			astString(_expression.node.text),
			_expression.memberNameLocation
		);
	}

	if (_expression.node.kind == rustAstNodeKindFunctionCallOptions)
	{
		if (_expression.arguments.empty())
			return nullptr;
		if (_expression.arguments.size() != _expression.argumentDetails.size())
			return nullptr;
		if (!functionCallParameterNamesAreValid(
			_expression.arguments.size(),
			_expression.parameterNames,
			_expression.parameterNameLocations
		))
			return nullptr;
		if (_expression.parameterNames.size() != _expression.arguments.size())
			return nullptr;

		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRust(
			_expression.baseExpression,
			_expression.baseExpressionDetail
		);
		if (!baseExpression)
			return nullptr;

		std::vector<ASTPointer<Expression>> options;
		options.reserve(_expression.arguments.size());
		for (size_t i = 0; i < _expression.arguments.size(); ++i)
		{
			options.push_back(createExpressionAstFromRust(
				_expression.arguments[i],
				_expression.argumentDetails[i]
			));
			if (!options.back())
				return nullptr;
		}

		std::vector<ASTPointer<ASTString>> names;
		names.reserve(_expression.parameterNames.size());
		for (std::string const& name: _expression.parameterNames)
			names.push_back(astString(name));

		return std::make_shared<FunctionCallOptions>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(baseExpression),
			std::move(options),
			std::move(names)
		);
	}

	if (_expression.node.kind == rustAstNodeKindFunctionCall)
	{
		if (_expression.arguments.size() != _expression.argumentDetails.size())
			return nullptr;
		if (!functionCallParameterNamesAreValid(
			_expression.arguments.size(),
			_expression.parameterNames,
			_expression.parameterNameLocations
		))
			return nullptr;

		ASTPointer<Expression> callee = createSingleChildExpressionAstFromRust(
			_expression.baseExpression,
			_expression.baseExpressionDetail
		);
		if (!callee)
			return nullptr;

		std::vector<ASTPointer<Expression>> arguments;
		arguments.reserve(_expression.arguments.size());
		for (size_t i = 0; i < _expression.arguments.size(); ++i)
		{
			arguments.push_back(createExpressionAstFromRust(
				_expression.arguments[i],
				_expression.argumentDetails[i]
			));
			if (!arguments.back())
				return nullptr;
		}

		std::vector<ASTPointer<ASTString>> parameterNames;
		parameterNames.reserve(_expression.parameterNames.size());
		for (std::string const& parameterName: _expression.parameterNames)
			parameterNames.push_back(astString(parameterName));

		return std::make_shared<FunctionCall>(
			_expression.node.nodeID,
			_expression.node.location,
			std::move(callee),
			std::move(arguments),
			std::move(parameterNames),
			_expression.parameterNameLocations
		);
	}

	if (_expression.node.kind != rustAstNodeKindLiteral)
		return nullptr;

	Token literalToken = static_cast<Token>(_expression.literalToken);
	if (
		literalToken != Token::TrueLiteral &&
		literalToken != Token::FalseLiteral &&
		literalToken != Token::Number &&
		literalToken != Token::StringLiteral &&
		literalToken != Token::UnicodeStringLiteral &&
		literalToken != Token::HexStringLiteral
	)
		return nullptr;

	Literal::SubDenomination subdenomination = Literal::SubDenomination::None;
	if (_expression.literalSubdenomination != static_cast<std::uint32_t>(Token::Illegal))
	{
		Token subdenominationToken = static_cast<Token>(_expression.literalSubdenomination);
		if (
			literalToken != Token::Number ||
			!(
				TokenTraits::isEtherSubdenomination(subdenominationToken) ||
				TokenTraits::isTimeSubdenomination(subdenominationToken)
			)
		)
			return nullptr;
		subdenomination = static_cast<Literal::SubDenomination>(subdenominationToken);
	}

	return std::make_shared<Literal>(
		_expression.node.nodeID,
		_expression.node.location,
		literalToken,
		astString(_expression.node.text),
		subdenomination
	);
}

ASTPointer<FunctionCall> createFunctionCallAstFromRust(
	RustParserAstNode const& _functionCall,
	RustParserAstNode const& _calleeNode,
	RustParserExpression const& _callee,
	std::vector<RustParserAstNode> const& _arguments,
	std::vector<RustParserExpression> const& _argumentDetails,
	std::vector<std::string> const& _parameterNames,
	std::vector<SourceLocation> const& _parameterNameLocations
)
{
	if (!_functionCall.present || _functionCall.kind != rustAstNodeKindFunctionCall)
		return nullptr;
	if (_arguments.size() != _argumentDetails.size())
		return nullptr;
	if (!functionCallParameterNamesAreValid(_arguments.size(), _parameterNames, _parameterNameLocations))
		return nullptr;

	ASTPointer<Expression> callee = createExpressionAstFromRust(_calleeNode, _callee);
	if (!callee)
		return nullptr;

	std::vector<ASTPointer<Expression>> arguments;
	arguments.reserve(_arguments.size());
	for (size_t i = 0; i < _arguments.size(); ++i)
	{
		arguments.push_back(createExpressionAstFromRust(_arguments[i], _argumentDetails[i]));
		if (!arguments.back())
			return nullptr;
	}

	std::vector<ASTPointer<ASTString>> parameterNames;
	parameterNames.reserve(_parameterNames.size());
	for (std::string const& parameterName: _parameterNames)
		parameterNames.push_back(astString(parameterName));

	return std::make_shared<FunctionCall>(
		_functionCall.nodeID,
		_functionCall.location,
		std::move(callee),
		std::move(arguments),
		std::move(parameterNames),
		_parameterNameLocations
	);
}

}

ASTPointer<VariableDeclaration> solidity::frontend::createVariableDeclarationAstFromRust(
	RustParserVariableDeclaration const& _variable
)
{
	if (!_variable.node.present || _variable.node.kind != rustAstNodeKindVariableDeclaration)
		return nullptr;
	if (!structuredDocumentationIsSupported(_variable.documentation))
		return nullptr;
	if (_variable.typeName.present && _variable.typeExpression.present)
		return nullptr;

	ASTPointer<OverrideSpecifier> overrides;
	if (_variable.overrides.present)
	{
		overrides = createOverrideSpecifierAstFromRust(
			_variable.overrides,
			_variable.overridePaths,
			_variable.overridePathDetails
		);
		if (!overrides)
			return nullptr;
	}
	else if (!_variable.overridePaths.empty() || !_variable.overridePathDetails.empty())
		return nullptr;

	ASTPointer<TypeName> typeName;
	if (_variable.typeName.present)
	{
		auto createMappingTypeName = [&](RustParserAstNode const& _mapping) -> ASTPointer<TypeName>
		{
			return createMappingTypeNameAstFromRust(_mapping, _variable.typeNameMappingDetails);
		};

		if (_variable.typeName.kind == rustAstNodeKindMapping)
			typeName = createMappingTypeName(_variable.typeName);
		else
			typeName = createTypeNameAstFromRust(
				_variable.typeName,
				_variable.typeNameElementaryToken,
				_variable.typeNameElementaryFirstNumber,
				_variable.typeNameElementarySecondNumber,
				_variable.typeNameHasStateMutability,
				_variable.typeNameStateMutability,
				_variable.typeNameUserDefinedPathNode,
				_variable.typeNameUserDefinedPath,
				_variable.typeNameUserDefinedPathLocations,
				_variable.typeNameFunctionParameters,
				_variable.typeNameFunctionParameterDeclarations,
				_variable.typeNameFunctionParameterDetails,
				_variable.typeNameFunctionReturnParameters,
				_variable.typeNameFunctionReturnParameterDeclarations,
				_variable.typeNameFunctionReturnParameterDetails,
				_variable.typeNameFunctionVisibility,
				_variable.typeNameFunctionStateMutability,
				_variable.typeNameArrayBaseTypes,
				_variable.typeNameArrayLengths,
				_variable.typeNameArrayLengthDetails,
				createMappingTypeName
			);
		if (!typeName)
			return nullptr;
	}

	ASTPointer<Expression> typeExpression;
	if (_variable.typeExpression.present)
	{
		typeExpression = createExpressionAstFromRust(
			_variable.typeExpression,
			_variable.typeExpressionDetail
		);
		if (!typeExpression)
			return nullptr;
	}
	else if (_variable.typeExpressionDetail.node.present)
		return nullptr;

	ASTPointer<Expression> value;
	if (_variable.value.present)
	{
		value = createExpressionAstFromRust(_variable.value, _variable.valueDetail);
		if (!value)
			return nullptr;
	}
	else if (_variable.valueDetail.node.present)
		return nullptr;

	std::optional<Visibility> visibility = visibilityFromRust(_variable.visibility);
	std::optional<VariableDeclaration::Mutability> mutability =
		variableDeclarationMutabilityFromRust(_variable.mutability);
	std::optional<VariableDeclaration::Location> location =
		variableDeclarationLocationFromRust(_variable.variableLocation);
	if (!visibility.has_value() || !mutability.has_value() || !location.has_value())
		return nullptr;

	return std::make_shared<VariableDeclaration>(
		_variable.node.nodeID,
		_variable.node.location,
		std::move(typeName),
		astString(_variable.name),
		_variable.nameLocation,
		std::move(value),
		*visibility,
		createStructuredDocumentationAstFromRust(_variable.documentation),
		_variable.indexed,
		*mutability,
		overrides,
		*location,
		std::move(typeExpression)
	);
}

ASTPointer<StructDefinition> solidity::frontend::createStructDefinitionAstFromRust(
	RustParserStructDefinition const& _struct
)
{
	if (!_struct.node.present || _struct.node.kind != rustAstNodeKindStructDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_struct.documentation))
		return nullptr;
	if (_struct.members.size() != _struct.memberDetails.size())
		return nullptr;

	std::vector<ASTPointer<VariableDeclaration>> members;
	members.reserve(_struct.members.size());
	for (RustParserAstNode const& member: _struct.members)
	{
		auto memberDetail = std::find_if(
			_struct.memberDetails.begin(),
			_struct.memberDetails.end(),
			[&](RustParserVariableDeclaration const& _member)
			{
				return rustAstNodesMatch(_member.node, member);
			}
		);
		if (memberDetail == _struct.memberDetails.end())
			return nullptr;
		if (!variableDeclarationIsStructMemberCompatible(*memberDetail))
			return nullptr;

		members.push_back(createVariableDeclarationAstFromRust(*memberDetail));
		if (!members.back())
			return nullptr;
	}

	return std::make_shared<StructDefinition>(
		_struct.node.nodeID,
		_struct.node.location,
		astString(_struct.name),
		_struct.nameLocation,
		std::move(members),
		createStructuredDocumentationAstFromRust(_struct.documentation)
	);
}

ASTPointer<EventDefinition> solidity::frontend::createEventDefinitionAstFromRust(
	RustParserEventDefinition const& _event
)
{
	if (!_event.node.present || _event.node.kind != rustAstNodeKindEventDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_event.documentation))
		return nullptr;

	ASTPointer<ParameterList> parameters = createParameterListAstFromRust(
		_event.parameters,
		_event.parameterDeclarations,
		_event.parameterDetails,
		RustParserParameterContext::Event
	);
	if (!parameters)
		return nullptr;

	return std::make_shared<EventDefinition>(
		_event.node.nodeID,
		_event.node.location,
		astString(_event.name),
		_event.nameLocation,
		createStructuredDocumentationAstFromRust(_event.documentation),
		std::move(parameters),
		_event.anonymous
	);
}

ASTPointer<ErrorDefinition> solidity::frontend::createErrorDefinitionAstFromRust(
	RustParserErrorDefinition const& _error
)
{
	if (!_error.node.present || _error.node.kind != rustAstNodeKindErrorDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_error.documentation))
		return nullptr;

	ASTPointer<ParameterList> parameters = createParameterListAstFromRust(
		_error.parameters,
		_error.parameterDeclarations,
		_error.parameterDetails
	);
	if (!parameters)
		return nullptr;

	return std::make_shared<ErrorDefinition>(
		_error.node.nodeID,
		_error.node.location,
		astString(_error.name),
		_error.nameLocation,
		createStructuredDocumentationAstFromRust(_error.documentation),
		std::move(parameters)
	);
}

namespace
{

ASTPointer<ASTString> documentationStringFromRust(RustParserAstNode const& _node)
{
	if (_node.text.empty())
		return nullptr;
	return astString(_node.text);
}

ASTPointer<Statement> createStatementAstFromRust(RustParserStatement const& _statement);

ASTPointer<Block> createBlockAstFromRust(
	RustParserAstNode const& _block,
	bool _unchecked,
	std::vector<RustParserAstNode> const& _statements,
	std::vector<RustParserStatement> const& _statementDetails
);

ASTPointer<Statement> createSingleStatementAstFromRust(
	RustParserAstNode const& _node,
	std::vector<RustParserStatement> const& _statementDetails
)
{
	if (!_node.present || _statementDetails.size() != 1)
		return nullptr;
	RustParserStatement const& statement = _statementDetails.front();
	if (!rustAstNodesMatch(statement.node, _node))
		return nullptr;
	return createStatementAstFromRust(statement);
}

std::shared_ptr<yul::AST> parseInlineAssemblyOperationsFromRust(SourceLocation const& _blockLocation)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context)
		return nullptr;
	if (
		_blockLocation.start < 0 ||
		_blockLocation.end < _blockLocation.start ||
		static_cast<size_t>(_blockLocation.end) > context->source.size()
	)
		return nullptr;

	CharStream source(context->source, context->sourceName);
	auto scanner = std::make_shared<Scanner>(source);
	while (scanner->currentToken() != Token::EOS && scanner->currentLocation().start < _blockLocation.start)
		scanner->next();

	if (scanner->currentToken() != Token::LBrace || scanner->currentLocation().start != _blockLocation.start)
		return nullptr;

	ErrorList errors;
	ErrorReporter errorReporter(errors);
	yul::Dialect const& dialect = yul::EVMDialect::strictAssemblyForEVM(context->evmVersion);
	std::unique_ptr<yul::AST> parsedOperations = yul::Parser(errorReporter, dialect).parseInline(scanner);
	if (!parsedOperations)
		return nullptr;

	SourceLocation const operationsLocation = nativeLocationOf(parsedOperations->root());
	if (operationsLocation.start != _blockLocation.start || operationsLocation.end != _blockLocation.end)
		return nullptr;

	return std::shared_ptr<yul::AST>(std::move(parsedOperations));
}

ASTPointer<InlineAssembly> createInlineAssemblyAstFromRust(
	RustParserStatement const& _statement,
	ASTPointer<ASTString> _documentation
)
{
	if (!_statement.node.present || _statement.node.kind != rustAstNodeKindInlineAssembly)
		return nullptr;

	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context)
		return nullptr;

	yul::Dialect const& dialect = yul::EVMDialect::strictAssemblyForEVM(context->evmVersion);
	std::shared_ptr<yul::AST> operations = parseInlineAssemblyOperationsFromRust(_statement.inlineAssemblyBlockLocation);
	if (!operations)
		return nullptr;

	ASTPointer<std::vector<ASTPointer<ASTString>>> flags;
	if (!_statement.inlineAssemblyFlags.empty())
	{
		flags = std::make_shared<std::vector<ASTPointer<ASTString>>>();
		flags->reserve(_statement.inlineAssemblyFlags.size());
		for (std::string const& flag: _statement.inlineAssemblyFlags)
			flags->push_back(astString(flag));
	}

	return std::make_shared<InlineAssembly>(
		_statement.node.nodeID,
		_statement.node.location,
		std::move(_documentation),
		dialect,
		std::move(flags),
		std::move(operations)
	);
}

ASTPointer<TryCatchClause> createTryCatchClauseAstFromRust(
	RustParserTryCatchClause const& _clause,
	std::vector<RustParserStatement> const& _blockStatementDetails
)
{
	if (!_clause.node.present || _clause.node.kind != rustAstNodeKindTryCatchClause)
		return nullptr;

	ASTPointer<ParameterList> parameters;
	if (_clause.errorParameters.present)
	{
		parameters = createParameterListAstFromRust(
			_clause.errorParameters,
			_clause.errorParameterDeclarations,
			_clause.errorParameterDetails,
			RustParserParameterContext::LocationAllowed
		);
		if (!parameters)
			return nullptr;
	}
	else if (!_clause.errorParameterDeclarations.empty() || !_clause.errorParameterDetails.empty())
		return nullptr;
	if (!_clause.errorName.empty() && !_clause.errorParameters.present)
		return nullptr;

	std::vector<RustParserStatement> blockStatementDetails;
	blockStatementDetails.reserve(_clause.blockStatements.size());
	for (RustParserAstNode const& statement: _clause.blockStatements)
	{
		auto statementDetail = std::find_if(
			_blockStatementDetails.begin(),
			_blockStatementDetails.end(),
			[&](RustParserStatement const& _statement)
			{
				return rustAstNodesMatch(_statement.node, statement);
			}
		);
		if (statementDetail == _blockStatementDetails.end())
			return nullptr;
		blockStatementDetails.push_back(*statementDetail);
	}

	ASTPointer<Block> block = createBlockAstFromRust(
		_clause.block,
		_clause.blockUnchecked,
		_clause.blockStatements,
		blockStatementDetails
	);
	if (!block)
		return nullptr;

	return std::make_shared<TryCatchClause>(
		_clause.node.nodeID,
		_clause.node.location,
		astString(_clause.errorName),
		std::move(parameters),
		std::move(block)
	);
}

ASTPointer<Statement> createStatementAstFromRust(RustParserStatement const& _statement)
{
	if (!_statement.node.present)
		return nullptr;

	ASTPointer<ASTString> documentation = documentationStringFromRust(_statement.node);
	switch (_statement.node.kind)
	{
	case rustAstNodeKindBlock:
	case rustAstNodeKindUncheckedBlock:
		return createBlockAstFromRust(
			_statement.node,
			_statement.blockUnchecked,
			_statement.blockStatements,
			_statement.blockStatementDetails
		);
	case rustAstNodeKindPlaceholderStatement:
		return std::make_shared<PlaceholderStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation)
		);
	case rustAstNodeKindContinueStatement:
		return std::make_shared<Continue>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation)
		);
	case rustAstNodeKindBreakStatement:
		return std::make_shared<Break>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation)
		);
	case rustAstNodeKindReturnStatement:
	{
		ASTPointer<Expression> expression;
		if (_statement.expression.present)
		{
			expression = createExpressionAstFromRust(_statement.expression, _statement.expressionDetail);
			if (!expression)
				return nullptr;
		}
		else if (_statement.expressionDetail.node.present)
			return nullptr;
		return std::make_shared<Return>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(expression)
		);
	}
	case rustAstNodeKindThrowStatement:
		return std::make_shared<Throw>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation)
		);
	case rustAstNodeKindEmitStatement:
	{
		if (!expressionIsIdentifierPathFromRust(_statement.eventCallCallee, _statement.eventCallCalleeDetail))
			return nullptr;

		ASTPointer<FunctionCall> eventCall = createFunctionCallAstFromRust(
			_statement.eventCall,
			_statement.eventCallCallee,
			_statement.eventCallCalleeDetail,
			_statement.eventCallArguments,
			_statement.eventCallArgumentDetails,
			_statement.eventCallParameterNames,
			_statement.eventCallParameterNameLocations
		);
		if (!eventCall)
			return nullptr;

		return std::make_shared<EmitStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(eventCall)
		);
	}
	case rustAstNodeKindRevertStatement:
	{
		if (!expressionIsIdentifierPathFromRust(_statement.errorCallCallee, _statement.errorCallCalleeDetail))
			return nullptr;

		ASTPointer<FunctionCall> errorCall = createFunctionCallAstFromRust(
			_statement.errorCall,
			_statement.errorCallCallee,
			_statement.errorCallCalleeDetail,
			_statement.errorCallArguments,
			_statement.errorCallArgumentDetails,
			_statement.errorCallParameterNames,
			_statement.errorCallParameterNameLocations
		);
		if (!errorCall)
			return nullptr;

		return std::make_shared<RevertStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(errorCall)
		);
	}
	case rustAstNodeKindInlineAssembly:
		return createInlineAssemblyAstFromRust(
			_statement,
			std::move(documentation)
		);
	case rustAstNodeKindIfStatement:
	{
		ASTPointer<Expression> condition = createExpressionAstFromRust(
			_statement.conditionExpression,
			_statement.conditionExpressionDetail
		);
		if (!condition)
			return nullptr;

		ASTPointer<Statement> trueBody = createSingleStatementAstFromRust(
			_statement.trueBody,
			_statement.trueBodyDetail
		);
		if (!trueBody)
			return nullptr;

		ASTPointer<Statement> falseBody;
		if (_statement.falseBody.present)
		{
			falseBody = createSingleStatementAstFromRust(
				_statement.falseBody,
				_statement.falseBodyDetail
			);
			if (!falseBody)
				return nullptr;
		}
		else if (!_statement.falseBodyDetail.empty())
			return nullptr;

		return std::make_shared<IfStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(condition),
			std::move(trueBody),
			std::move(falseBody)
		);
	}
	case rustAstNodeKindTryStatement:
	{
		ASTPointer<Expression> externalCall = createExpressionAstFromRust(
			_statement.externalCall,
			_statement.externalCallDetail
		);
		if (!externalCall)
			return nullptr;
		if (_statement.clauses.size() != _statement.clauseDetails.size())
			return nullptr;
		if (_statement.clauses.size() < 2)
			return nullptr;

		auto findClauseDetail = [&](RustParserAstNode const& _clause)
		{
			return std::find_if(
				_statement.clauseDetails.begin(),
				_statement.clauseDetails.end(),
				[&](RustParserTryCatchClause const& _clauseDetail)
				{
					return rustAstNodesMatch(_clauseDetail.node, _clause);
				}
			);
		};

		auto firstClauseDetail = findClauseDetail(_statement.clauses.front());
		if (firstClauseDetail == _statement.clauseDetails.end() || !firstClauseDetail->errorName.empty())
			return nullptr;

		std::vector<ASTPointer<TryCatchClause>> clauses;
		clauses.reserve(_statement.clauses.size());
		for (RustParserAstNode const& clause: _statement.clauses)
		{
			auto clauseDetail = findClauseDetail(clause);
			if (clauseDetail == _statement.clauseDetails.end())
				return nullptr;

			clauses.push_back(createTryCatchClauseAstFromRust(
				*clauseDetail,
				_statement.clauseBlockStatementDetails
			));
			if (!clauses.back())
				return nullptr;
		}

		return std::make_shared<TryStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(externalCall),
			std::move(clauses)
		);
	}
	case rustAstNodeKindWhileStatement:
	case rustAstNodeKindDoWhileStatement:
	{
		if ((_statement.node.kind == rustAstNodeKindDoWhileStatement) != _statement.isDoWhile)
			return nullptr;

		ASTPointer<Expression> condition = createExpressionAstFromRust(
			_statement.conditionExpression,
			_statement.conditionExpressionDetail
		);
		if (!condition)
			return nullptr;

		ASTPointer<Statement> body = createSingleStatementAstFromRust(
			_statement.body,
			_statement.bodyDetail
		);
		if (!body)
			return nullptr;

		return std::make_shared<WhileStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(condition),
			std::move(body),
			_statement.isDoWhile
		);
	}
	case rustAstNodeKindForStatement:
	{
		ASTPointer<Statement> initExpression;
		if (_statement.initExpression.present)
		{
			initExpression = createSingleStatementAstFromRust(
				_statement.initExpression,
				_statement.initExpressionDetail
			);
			if (!initExpression)
				return nullptr;
		}
		else if (!_statement.initExpressionDetail.empty())
			return nullptr;

		ASTPointer<Expression> conditionExpression;
		if (_statement.conditionExpression.present)
		{
			conditionExpression = createExpressionAstFromRust(
				_statement.conditionExpression,
				_statement.conditionExpressionDetail
			);
			if (!conditionExpression)
				return nullptr;
		}
		else if (_statement.conditionExpressionDetail.node.present)
			return nullptr;

		ASTPointer<ExpressionStatement> loopExpression;
		if (_statement.loopExpression.present)
		{
			if (_statement.loopExpression.kind != rustAstNodeKindExpressionStatement)
				return nullptr;

			ASTPointer<Statement> loopStatement = createSingleStatementAstFromRust(
				_statement.loopExpression,
				_statement.loopExpressionDetail
			);
			loopExpression = std::dynamic_pointer_cast<ExpressionStatement>(loopStatement);
			if (!loopExpression)
				return nullptr;
		}
		else if (!_statement.loopExpressionDetail.empty())
			return nullptr;

		ASTPointer<Statement> body = createSingleStatementAstFromRust(
			_statement.body,
			_statement.bodyDetail
		);
		if (!body)
			return nullptr;

		return std::make_shared<ForStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(initExpression),
			std::move(conditionExpression),
			std::move(loopExpression),
			std::move(body)
		);
	}
	case rustAstNodeKindExpressionStatement:
	{
		if (!_statement.expression.present)
			return nullptr;
		ASTPointer<Expression> expression = createExpressionAstFromRust(
			_statement.expression,
			_statement.expressionDetail
		);
		if (!expression)
			return nullptr;
		return std::make_shared<ExpressionStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(expression)
		);
	}
		case rustAstNodeKindVariableDeclarationStatement:
		{
			size_t presentVariables = 0;
			for (RustParserAstNode const& variable: _statement.variables)
				if (variable.present)
					++presentVariables;
			if (_statement.variables.empty() || presentVariables == 0)
				return nullptr;
			if (!_statement.initialValue.present && _statement.variables.size() != 1)
				return nullptr;
			if (presentVariables != _statement.variableDetails.size())
				return nullptr;

			std::vector<ASTPointer<VariableDeclaration>> variables;
			variables.reserve(_statement.variables.size());
			for (RustParserAstNode const& variable: _statement.variables)
			{
				if (!variable.present)
				{
					variables.push_back(nullptr);
					continue;
				}

				auto variableDetail = std::find_if(
					_statement.variableDetails.begin(),
					_statement.variableDetails.end(),
					[&](RustParserVariableDeclaration const& _variable)
					{
						return rustAstNodesMatch(_variable.node, variable);
					}
					);
					if (variableDetail == _statement.variableDetails.end())
						return nullptr;
					if (!variableDeclarationIsLocalStatementCompatible(*variableDetail))
						return nullptr;

					variables.push_back(createVariableDeclarationAstFromRust(*variableDetail));
					if (!variables.back())
					return nullptr;
			}

		ASTPointer<Expression> initialValue;
		if (_statement.initialValue.present)
		{
			initialValue = createExpressionAstFromRust(
				_statement.initialValue,
				_statement.initialValueDetail
			);
			if (!initialValue)
				return nullptr;
		}
		else if (_statement.initialValueDetail.node.present)
			return nullptr;

		return std::make_shared<VariableDeclarationStatement>(
			_statement.node.nodeID,
			_statement.node.location,
			std::move(documentation),
			std::move(variables),
			std::move(initialValue)
		);
	}
	default:
		return nullptr;
	}
}

ASTPointer<Block> createBlockAstFromRust(
	RustParserAstNode const& _block,
	bool _unchecked,
	std::vector<RustParserAstNode> const& _statements,
	std::vector<RustParserStatement> const& _statementDetails
)
{
	if (!_block.present)
		return nullptr;
	if (_block.kind == rustAstNodeKindBlock && _unchecked)
		return nullptr;
	if (_block.kind == rustAstNodeKindUncheckedBlock && !_unchecked)
		return nullptr;
	if (_block.kind != rustAstNodeKindBlock && _block.kind != rustAstNodeKindUncheckedBlock)
		return nullptr;
	if (_statements.size() != _statementDetails.size())
		return nullptr;

	std::vector<ASTPointer<Statement>> statements;
	statements.reserve(_statements.size());
	for (RustParserAstNode const& statement: _statements)
	{
		auto statementDetail = std::find_if(
			_statementDetails.begin(),
			_statementDetails.end(),
			[&](RustParserStatement const& _statement)
			{
				return rustAstNodesMatch(_statement.node, statement);
			}
		);
		if (statementDetail == _statementDetails.end())
			return nullptr;

		statements.push_back(createStatementAstFromRust(*statementDetail));
		if (!statements.back())
			return nullptr;
	}

	return std::make_shared<Block>(
		_block.nodeID,
		_block.location,
		documentationStringFromRust(_block),
		_unchecked,
		std::move(statements)
	);
}

ASTPointer<OverrideSpecifier> createOverrideSpecifierAstFromRust(
	RustParserAstNode const& _overrides,
	std::vector<RustParserAstNode> const& _overridePaths,
	std::vector<RustParserIdentifierPath> const& _overridePathDetails
)
{
	if (!_overrides.present)
		return nullptr;
	if (_overrides.kind != rustAstNodeKindOverrideSpecifier || _overridePaths.size() != _overridePathDetails.size())
		return nullptr;

	std::vector<ASTPointer<IdentifierPath>> overrides;
	overrides.reserve(_overridePaths.size());
	for (RustParserAstNode const& overridePath: _overridePaths)
	{
		auto overridePathDetail = std::find_if(
			_overridePathDetails.begin(),
			_overridePathDetails.end(),
			[&](RustParserIdentifierPath const& _overridePath)
			{
				return rustAstNodesMatch(_overridePath.node, overridePath);
			}
		);
		if (overridePathDetail == _overridePathDetails.end())
			return nullptr;

		overrides.push_back(createIdentifierPathAstFromRust(
			overridePathDetail->node,
			overridePathDetail->path,
			overridePathDetail->pathLocations
		));
		if (!overrides.back())
			return nullptr;
	}

	return std::make_shared<OverrideSpecifier>(
		_overrides.nodeID,
		_overrides.location,
		std::move(overrides)
	);
}

ASTPointer<ModifierInvocation> createModifierInvocationAstFromRust(RustParserModifierInvocation const& _modifier)
{
	if (!_modifier.node.present || _modifier.node.kind != rustAstNodeKindModifierInvocation)
		return nullptr;
	if (!_modifier.modifierName.present || !rustAstNodesMatch(_modifier.modifierName, _modifier.modifierNameDetail.node))
		return nullptr;

	ASTPointer<IdentifierPath> modifierName = createIdentifierPathAstFromRust(
		_modifier.modifierNameDetail.node,
		_modifier.modifierNameDetail.path,
		_modifier.modifierNameDetail.pathLocations
	);
	if (!modifierName)
		return nullptr;

	std::unique_ptr<std::vector<ASTPointer<Expression>>> arguments;
	if (_modifier.hasArguments)
	{
		if (_modifier.arguments.size() != _modifier.argumentDetails.size())
			return nullptr;
		arguments = std::make_unique<std::vector<ASTPointer<Expression>>>();
		arguments->reserve(_modifier.arguments.size());
		for (RustParserAstNode const& argument: _modifier.arguments)
		{
			auto argumentDetail = std::find_if(
				_modifier.argumentDetails.begin(),
				_modifier.argumentDetails.end(),
				[&](RustParserExpression const& _argument)
				{
					return rustAstNodesMatch(_argument.node, argument);
				}
			);
			if (argumentDetail == _modifier.argumentDetails.end())
				return nullptr;

			arguments->push_back(createExpressionAstFromRust(argument, *argumentDetail));
			if (!arguments->back())
				return nullptr;
		}
	}
	else if (!_modifier.arguments.empty() || !_modifier.argumentDetails.empty())
		return nullptr;

	return std::make_shared<ModifierInvocation>(
		_modifier.node.nodeID,
		_modifier.node.location,
		std::move(modifierName),
		std::move(arguments)
	);
}

}

ASTPointer<FunctionDefinition> solidity::frontend::createFunctionDefinitionAstFromRust(
	RustParserFunctionDefinition const& _function
)
{
	if (!_function.node.present || _function.node.kind != rustAstNodeKindFunctionDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_function.documentation))
		return nullptr;
	if (_function.modifiers.size() != _function.modifierDetails.size())
		return nullptr;

	ASTPointer<OverrideSpecifier> overrides;
	if (_function.overrides.present)
	{
		overrides = createOverrideSpecifierAstFromRust(
			_function.overrides,
			_function.overridePaths,
			_function.overridePathDetails
		);
		if (!overrides)
			return nullptr;
	}
	else if (!_function.overridePaths.empty() || !_function.overridePathDetails.empty())
		return nullptr;

	std::optional<Visibility> visibility = visibilityFromRust(_function.visibility);
	if (!visibility.has_value())
		return nullptr;

	std::optional<StateMutability> stateMutability = stateMutabilityFromRust(_function.stateMutability);
	if (!stateMutability.has_value())
		return nullptr;

	Token kind = static_cast<Token>(_function.kind);
	if (kind != Token::Function && kind != Token::Constructor && kind != Token::Fallback && kind != Token::Receive)
		return nullptr;
	if (_function.isFreeFunction && kind != Token::Function)
		return nullptr;
	if ((kind == Token::Function) != !_function.name.empty())
		return nullptr;
	if (RustParserReconstructionContext const* context = currentRustParserReconstructionContext)
	{
		if (context->experimentalSolidity)
		{
			if (_function.returnParameters.present || !_function.returnParameterDeclarations.empty() || !_function.returnParameterDetails.empty())
				return nullptr;
		}
		else if (!_function.returnParameters.present)
			return nullptr;
	}

	ASTPointer<ParameterList> parameters = createParameterListAstFromRust(
		_function.parameters,
		_function.parameterDeclarations,
		_function.parameterDetails,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<ParameterList> returnParameters;
	if (_function.returnParameters.present)
	{
		returnParameters = createParameterListAstFromRust(
			_function.returnParameters,
			_function.returnParameterDeclarations,
			_function.returnParameterDetails,
			RustParserParameterContext::LocationAllowed
		);
		if (!returnParameters)
			return nullptr;
	}
	else if (!_function.returnParameterDeclarations.empty() || !_function.returnParameterDetails.empty())
		return nullptr;

	ASTPointer<Expression> experimentalReturnExpression;
	if (_function.experimentalReturnExpression.present)
	{
		experimentalReturnExpression = createExpressionAstFromRust(
			_function.experimentalReturnExpression,
			_function.experimentalReturnExpressionDetail
		);
		if (!experimentalReturnExpression)
			return nullptr;
	}
	else if (_function.experimentalReturnExpressionDetail.node.present)
		return nullptr;

	ASTPointer<Block> body;
	if (_function.block.present)
	{
		body = createBlockAstFromRust(
			_function.block,
			_function.blockUnchecked,
			_function.blockStatements,
			_function.blockStatementDetails
		);
		if (!body)
			return nullptr;
	}
	else if (!_function.blockStatements.empty() || !_function.blockStatementDetails.empty())
		return nullptr;

	std::vector<ASTPointer<ModifierInvocation>> modifiers;
	modifiers.reserve(_function.modifiers.size());
	for (RustParserAstNode const& modifier: _function.modifiers)
	{
		auto modifierDetail = std::find_if(
			_function.modifierDetails.begin(),
			_function.modifierDetails.end(),
			[&](RustParserModifierInvocation const& _modifier)
			{
				return rustAstNodesMatch(_modifier.node, modifier);
			}
		);
		if (modifierDetail == _function.modifierDetails.end())
			return nullptr;

		modifiers.push_back(createModifierInvocationAstFromRust(*modifierDetail));
		if (!modifiers.back())
			return nullptr;
	}

	return std::make_shared<FunctionDefinition>(
		_function.node.nodeID,
		_function.node.location,
		astString(_function.name),
		_function.nameLocation,
		*visibility,
		*stateMutability,
		_function.isFreeFunction,
		kind,
		_function.isVirtual,
		overrides,
		createStructuredDocumentationAstFromRust(_function.documentation),
		parameters,
		std::move(modifiers),
		returnParameters,
		body,
		experimentalReturnExpression
	);
}

ASTPointer<ForAllQuantifier> solidity::frontend::createForAllQuantifierAstFromRust(
	RustParserForAllQuantifier const& _quantifier
)
{
	if (!_quantifier.node.present || _quantifier.node.kind != rustAstNodeKindForAllQuantifier)
		return nullptr;
	if (!rustAstNodesMatch(_quantifier.quantifiedFunction, _quantifier.quantifiedFunctionDetail.node))
		return nullptr;
	if (!_quantifier.quantifiedFunctionDetail.isFreeFunction)
		return nullptr;

	ASTPointer<ParameterList> typeVariableDeclarations = createParameterListAstFromRust(
		_quantifier.typeVariableDeclarations,
		_quantifier.typeVariableDeclarationParameters,
		_quantifier.typeVariableDeclarationDetails
	);
	if (!typeVariableDeclarations)
		return nullptr;

	ASTPointer<FunctionDefinition> quantifiedFunction = createFunctionDefinitionAstFromRust(
		_quantifier.quantifiedFunctionDetail
	);
	if (!quantifiedFunction)
		return nullptr;

	return std::make_shared<ForAllQuantifier>(
		_quantifier.node.nodeID,
		_quantifier.node.location,
		std::move(typeVariableDeclarations),
		std::move(quantifiedFunction)
	);
}

ASTPointer<TypeDefinition> solidity::frontend::createTypeDefinitionAstFromRust(
	RustParserTypeDefinition const& _typeDefinition
)
{
	if (!_typeDefinition.node.present || _typeDefinition.node.kind != rustAstNodeKindTypeDefinition)
		return nullptr;

	ASTPointer<ParameterList> arguments;
	if (_typeDefinition.arguments.present)
	{
		arguments = createParameterListAstFromRust(
			_typeDefinition.arguments,
			_typeDefinition.argumentParameters,
			_typeDefinition.argumentDetails
		);
		if (!arguments)
			return nullptr;
	}
	else if (!_typeDefinition.argumentParameters.empty() || !_typeDefinition.argumentDetails.empty())
		return nullptr;

	ASTPointer<Expression> expression;
	if (_typeDefinition.expression.present)
	{
		if (_typeDefinition.expression.kind == rustAstNodeKindBuiltin)
		{
			if (!_typeDefinition.hasBuiltinNameParameter)
				return nullptr;

			expression = std::make_shared<Builtin>(
				_typeDefinition.expression.nodeID,
				_typeDefinition.expression.location,
				astString(_typeDefinition.builtinNameParameter),
				_typeDefinition.builtinNameParameterLocation
			);
		}
		else
		{
			if (_typeDefinition.hasBuiltinNameParameter)
				return nullptr;

			expression = createExpressionAstFromRust(
				_typeDefinition.expression,
				_typeDefinition.expressionDetail
			);
			if (!expression)
				return nullptr;
		}
	}
	else if (_typeDefinition.expressionDetail.node.present || _typeDefinition.hasBuiltinNameParameter)
		return nullptr;

	return std::make_shared<TypeDefinition>(
		_typeDefinition.node.nodeID,
		_typeDefinition.node.location,
		astString(_typeDefinition.name),
		_typeDefinition.nameLocation,
		std::move(arguments),
		std::move(expression)
	);
}

ASTPointer<TypeClassDefinition> solidity::frontend::createTypeClassDefinitionAstFromRust(
	RustParserTypeClassDefinition const& _typeClassDefinition
)
{
	if (!_typeClassDefinition.node.present || _typeClassDefinition.node.kind != rustAstNodeKindTypeClassDefinition)
		return nullptr;
	if (!_typeClassDefinition.typeVariable.present || _typeClassDefinition.typeVariable.kind != rustAstNodeKindVariableDeclaration)
		return nullptr;
	if (!structuredDocumentationIsSupported(_typeClassDefinition.documentation))
		return nullptr;
	if (_typeClassDefinition.subNodes.size() != _typeClassDefinition.subNodeFunctionDetails.size())
		return nullptr;

	ASTPointer<VariableDeclaration> typeVariable = std::make_shared<VariableDeclaration>(
		_typeClassDefinition.typeVariable.nodeID,
		_typeClassDefinition.typeVariable.location,
		nullptr,
		astString(_typeClassDefinition.typeVariableName),
		_typeClassDefinition.typeVariableNameLocation,
		nullptr,
		Visibility::Default,
		nullptr
	);

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_typeClassDefinition.subNodes.size());
	for (RustParserAstNode const& subNode: _typeClassDefinition.subNodes)
	{
		auto functionDetail = std::find_if(
			_typeClassDefinition.subNodeFunctionDetails.begin(),
			_typeClassDefinition.subNodeFunctionDetails.end(),
			[&](RustParserFunctionDefinition const& _function)
			{
				return rustAstNodesMatch(_function.node, subNode);
			}
		);
		if (functionDetail == _typeClassDefinition.subNodeFunctionDetails.end())
			return nullptr;
		if (functionDetail->isFreeFunction || functionDetail->block.present)
			return nullptr;
		if (!functionDetail->blockStatements.empty() || !functionDetail->blockStatementDetails.empty())
			return nullptr;

		subNodes.push_back(createFunctionDefinitionAstFromRust(*functionDetail));
		if (!subNodes.back())
			return nullptr;
	}

	return std::make_shared<TypeClassDefinition>(
		_typeClassDefinition.node.nodeID,
		_typeClassDefinition.node.location,
		std::move(typeVariable),
		astString(_typeClassDefinition.name),
		_typeClassDefinition.nameLocation,
		createStructuredDocumentationAstFromRust(_typeClassDefinition.documentation),
		std::move(subNodes)
	);
}

static ASTPointer<TypeClassName> createTypeClassNameAstFromRust(RustParserTypeClassName const& _typeClassName)
{
	if (!_typeClassName.node.present || _typeClassName.node.kind != rustAstNodeKindTypeClassName)
		return nullptr;

	std::variant<Token, ASTPointer<IdentifierPath>> name;
	if (_typeClassName.isBuiltin)
	{
		Token token = static_cast<Token>(_typeClassName.builtinToken);
		if (!TokenTraits::isBuiltinTypeClassName(token))
			return nullptr;
		if (_typeClassName.identifierPath.present || _typeClassName.identifierPathDetail.node.present)
			return nullptr;
		name = token;
	}
	else
	{
		if (!rustAstNodesMatch(_typeClassName.identifierPath, _typeClassName.identifierPathDetail.node))
			return nullptr;

		ASTPointer<IdentifierPath> identifierPath = createIdentifierPathAstFromRust(
			_typeClassName.identifierPathDetail.node,
			_typeClassName.identifierPathDetail.path,
			_typeClassName.identifierPathDetail.pathLocations
		);
		if (!identifierPath)
			return nullptr;
		name = std::move(identifierPath);
	}

	return std::make_shared<TypeClassName>(
		_typeClassName.node.nodeID,
		_typeClassName.node.location,
		std::move(name)
	);
}

ASTPointer<TypeClassInstantiation> solidity::frontend::createTypeClassInstantiationAstFromRust(
	RustParserTypeClassInstantiation const& _typeClassInstantiation
)
{
	if (!_typeClassInstantiation.node.present || _typeClassInstantiation.node.kind != rustAstNodeKindTypeClassInstantiation)
		return nullptr;
	if (!rustAstNodesMatch(_typeClassInstantiation.typeConstructor, _typeClassInstantiation.typeConstructorDetail.node))
		return nullptr;
	if (!rustAstNodesMatch(_typeClassInstantiation.typeClassName, _typeClassInstantiation.typeClassNameDetail.node))
		return nullptr;
	if (_typeClassInstantiation.subNodes.size() != _typeClassInstantiation.subNodeFunctionDetails.size())
		return nullptr;

	ASTPointer<TypeName> typeConstructor = createTypeNameAstFromRust(_typeClassInstantiation.typeConstructorDetail);
	if (!typeConstructor)
		return nullptr;

	ASTPointer<ParameterList> argumentSorts;
	if (_typeClassInstantiation.argumentSorts.present)
	{
		argumentSorts = createParameterListAstFromRust(
			_typeClassInstantiation.argumentSorts,
			_typeClassInstantiation.argumentSortParameters,
			_typeClassInstantiation.argumentSortDetails
		);
		if (!argumentSorts)
			return nullptr;
	}
	else if (!_typeClassInstantiation.argumentSortParameters.empty() || !_typeClassInstantiation.argumentSortDetails.empty())
		return nullptr;

	ASTPointer<TypeClassName> typeClassName = createTypeClassNameAstFromRust(
		_typeClassInstantiation.typeClassNameDetail
	);
	if (!typeClassName)
		return nullptr;

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_typeClassInstantiation.subNodes.size());
	for (RustParserAstNode const& subNode: _typeClassInstantiation.subNodes)
	{
		auto functionDetail = std::find_if(
			_typeClassInstantiation.subNodeFunctionDetails.begin(),
			_typeClassInstantiation.subNodeFunctionDetails.end(),
			[&](RustParserFunctionDefinition const& _function)
			{
				return rustAstNodesMatch(_function.node, subNode);
			}
		);
		if (functionDetail == _typeClassInstantiation.subNodeFunctionDetails.end())
			return nullptr;
		if (functionDetail->isFreeFunction)
			return nullptr;

		subNodes.push_back(createFunctionDefinitionAstFromRust(*functionDetail));
		if (!subNodes.back())
			return nullptr;
	}

	return std::make_shared<TypeClassInstantiation>(
		_typeClassInstantiation.node.nodeID,
		_typeClassInstantiation.node.location,
		std::move(typeConstructor),
		std::move(argumentSorts),
		std::move(typeClassName),
		std::move(subNodes)
	);
}

ASTPointer<ModifierDefinition> solidity::frontend::createModifierDefinitionAstFromRust(
	RustParserModifierDefinition const& _modifier
)
{
	if (!_modifier.node.present || _modifier.node.kind != rustAstNodeKindModifierDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_modifier.documentation))
		return nullptr;

	ASTPointer<ParameterList> parameters = createParameterListAstFromRust(
		_modifier.parameters,
		_modifier.parameterDeclarations,
		_modifier.parameterDetails,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<OverrideSpecifier> overrides;
	if (_modifier.overrides.present)
	{
		overrides = createOverrideSpecifierAstFromRust(
			_modifier.overrides,
			_modifier.overridePaths,
			_modifier.overridePathDetails
		);
		if (!overrides)
			return nullptr;
	}
	else if (!_modifier.overridePaths.empty() || !_modifier.overridePathDetails.empty())
		return nullptr;

	ASTPointer<Block> body;
	if (_modifier.block.present)
	{
		body = createBlockAstFromRust(
			_modifier.block,
			_modifier.blockUnchecked,
			_modifier.blockStatements,
			_modifier.blockStatementDetails
		);
		if (!body)
			return nullptr;
	}
	else if (!_modifier.blockStatements.empty() || !_modifier.blockStatementDetails.empty())
		return nullptr;

	return std::make_shared<ModifierDefinition>(
		_modifier.node.nodeID,
		_modifier.node.location,
		astString(_modifier.name),
		_modifier.nameLocation,
		createStructuredDocumentationAstFromRust(_modifier.documentation),
		parameters,
		_modifier.isVirtual,
		overrides,
		body
	);
}

ASTPointer<UsingForDirective> solidity::frontend::createUsingDirectiveAstFromRust(
	RustParserUsingDirective const& _using
)
{
	if (!_using.node.present || _using.node.kind != rustAstNodeKindUsingForDirective)
		return nullptr;
	if (_using.typeName.present != _using.typeNameDetail.node.present)
		return nullptr;
	if (_using.functions.size() != _using.functionDetails.size() || _using.functions.size() != _using.operators.size())
		return nullptr;
	if (_using.usesBraces)
	{
		if (_using.functions.empty())
			return nullptr;
	}
	else if (_using.functions.size() != 1 || _using.operators.front().present)
		return nullptr;

	std::vector<ASTPointer<IdentifierPath>> functions;
	functions.reserve(_using.functions.size());
	for (RustParserAstNode const& function: _using.functions)
	{
		auto functionDetail = std::find_if(
			_using.functionDetails.begin(),
			_using.functionDetails.end(),
			[&](RustParserIdentifierPath const& _function)
			{
				return rustAstNodesMatch(_function.node, function);
			}
		);
		if (functionDetail == _using.functionDetails.end())
			return nullptr;

		functions.push_back(createIdentifierPathAstFromRust(
			functionDetail->node,
			functionDetail->path,
			functionDetail->pathLocations
		));
		if (!functions.back())
			return nullptr;
	}

	std::vector<std::optional<Token>> operators;
	operators.reserve(_using.operators.size());
	for (RustParserUsingOperator const& usingOperator: _using.operators)
	{
		if (usingOperator.present)
		{
			Token token = static_cast<Token>(usingOperator.token);
			if (
				static_cast<size_t>(token) >= TokenTraits::count() ||
				token == Token::EOS ||
				token == Token::Illegal ||
				token == Token::Whitespace
			)
				return nullptr;
			operators.emplace_back(token);
		}
		else
			operators.emplace_back(std::nullopt);
	}

	ASTPointer<TypeName> typeName;
	if (_using.typeName.present)
	{
		if (!rustAstNodesMatch(_using.typeName, _using.typeNameDetail.node))
			return nullptr;
		typeName = createTypeNameAstFromRust(_using.typeNameDetail);
		if (!typeName)
			return nullptr;
	}

	return std::make_shared<UsingForDirective>(
		_using.node.nodeID,
		_using.node.location,
		std::move(functions),
		std::move(operators),
		_using.usesBraces,
		std::move(typeName),
		_using.global
	);
}

namespace
{

ASTPointer<InheritanceSpecifier> createInheritanceSpecifierAstFromRust(RustParserInheritanceSpecifier const& _inheritance)
{
	if (!_inheritance.node.present || _inheritance.node.kind != rustAstNodeKindInheritanceSpecifier)
		return nullptr;

	ASTPointer<IdentifierPath> baseName = createIdentifierPathAstFromRust(
		_inheritance.baseName,
		_inheritance.baseNamePath,
		_inheritance.baseNamePathLocations
	);
	if (!baseName)
		return nullptr;

	std::unique_ptr<std::vector<ASTPointer<Expression>>> arguments;
	if (_inheritance.hasArguments)
	{
		if (_inheritance.arguments.size() != _inheritance.argumentDetails.size())
			return nullptr;
		arguments = std::make_unique<std::vector<ASTPointer<Expression>>>();
		arguments->reserve(_inheritance.arguments.size());
		for (RustParserAstNode const& argument: _inheritance.arguments)
		{
			auto argumentDetail = std::find_if(
				_inheritance.argumentDetails.begin(),
				_inheritance.argumentDetails.end(),
				[&](RustParserExpression const& _argument)
				{
					return rustAstNodesMatch(_argument.node, argument);
				}
			);
			if (argumentDetail == _inheritance.argumentDetails.end())
				return nullptr;

			arguments->push_back(createExpressionAstFromRust(argument, *argumentDetail));
			if (!arguments->back())
				return nullptr;
		}
	}
	else if (!_inheritance.arguments.empty() || !_inheritance.argumentDetails.empty())
		return nullptr;

	return std::make_shared<InheritanceSpecifier>(
		_inheritance.node.nodeID,
		_inheritance.node.location,
		std::move(baseName),
		std::move(arguments)
	);
}

ASTPointer<StorageLayoutSpecifier> createStorageLayoutSpecifierAstFromRust(
	RustParserAstNode const& _storageLayoutSpecifier,
	RustParserAstNode const& _baseSlotExpressionNode,
	RustParserExpression const& _baseSlotExpressionDetail
)
{
	if (!_storageLayoutSpecifier.present || _storageLayoutSpecifier.kind != rustAstNodeKindStorageLayoutSpecifier)
		return nullptr;

	ASTPointer<Expression> baseSlotExpression = createExpressionAstFromRust(
		_baseSlotExpressionNode,
		_baseSlotExpressionDetail
	);
	if (!baseSlotExpression)
		return nullptr;

	return std::make_shared<StorageLayoutSpecifier>(
		_storageLayoutSpecifier.nodeID,
		_storageLayoutSpecifier.location,
		std::move(baseSlotExpression)
	);
}

}

ASTPointer<ContractDefinition> solidity::frontend::createContractDefinitionAstFromRust(
	RustParserContractDefinition const& _contract
)
{
	if (!_contract.node.present || _contract.node.kind != rustAstNodeKindContractDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(_contract.documentation))
		return nullptr;
	if (_contract.baseContracts.size() != _contract.baseContractDetails.size())
		return nullptr;

	std::optional<ContractKind> contractKind = contractKindFromRust(_contract.contractKind);
	if (!contractKind.has_value())
		return nullptr;

	ASTPointer<StorageLayoutSpecifier> storageLayoutSpecifier;
	if (_contract.storageLayoutSpecifier.present)
	{
		if (*contractKind != ContractKind::Contract)
			return nullptr;

		storageLayoutSpecifier = createStorageLayoutSpecifierAstFromRust(
			_contract.storageLayoutSpecifier,
			_contract.storageLayoutBaseSlotExpression,
			_contract.storageLayoutBaseSlotExpressionDetail
		);
		if (!storageLayoutSpecifier)
			return nullptr;
	}
	else if (_contract.storageLayoutBaseSlotExpression.present || _contract.storageLayoutBaseSlotExpressionDetail.node.present)
		return nullptr;

	auto createSupportedSubNode = [&](RustParserAstNode const& _node) -> ASTPointer<ASTNode>
	{
		auto structDefinition = std::find_if(
			_contract.subNodeStructs.begin(),
			_contract.subNodeStructs.end(),
			[&](RustParserStructDefinition const& _struct)
			{
			return rustAstNodesMatch(_struct.node, _node);
			}
		);
		if (structDefinition != _contract.subNodeStructs.end())
			return createStructDefinitionAstFromRust(*structDefinition);

		auto enumDefinition = std::find_if(
			_contract.subNodeEnums.begin(),
			_contract.subNodeEnums.end(),
			[&](RustParserEnumDefinition const& _enum)
			{
			return rustAstNodesMatch(_enum.node, _node);
			}
		);
		if (enumDefinition != _contract.subNodeEnums.end())
			return createEnumDefinitionAstFromRust(*enumDefinition);

		auto valueType = std::find_if(
			_contract.subNodeUserDefinedValueTypes.begin(),
			_contract.subNodeUserDefinedValueTypes.end(),
			[&](RustParserUserDefinedValueTypeDefinition const& _typeDefinition)
			{
			return rustAstNodesMatch(_typeDefinition.node, _node);
			}
		);
		if (valueType != _contract.subNodeUserDefinedValueTypes.end())
			return createUserDefinedValueTypeDefinitionAstFromRust(*valueType);

		auto eventDefinition = std::find_if(
			_contract.subNodeEvents.begin(),
			_contract.subNodeEvents.end(),
			[&](RustParserEventDefinition const& _event)
			{
			return rustAstNodesMatch(_event.node, _node);
			}
		);
		if (eventDefinition != _contract.subNodeEvents.end())
			return createEventDefinitionAstFromRust(*eventDefinition);

		auto errorDefinition = std::find_if(
			_contract.subNodeErrors.begin(),
			_contract.subNodeErrors.end(),
			[&](RustParserErrorDefinition const& _error)
			{
			return rustAstNodesMatch(_error.node, _node);
			}
		);
		if (errorDefinition != _contract.subNodeErrors.end())
			return createErrorDefinitionAstFromRust(*errorDefinition);

		auto functionDefinition = std::find_if(
			_contract.subNodeFunctions.begin(),
			_contract.subNodeFunctions.end(),
			[&](RustParserFunctionDefinition const& _function)
			{
			return rustAstNodesMatch(_function.node, _node);
			}
		);
		if (functionDefinition != _contract.subNodeFunctions.end())
		{
			if (functionDefinition->isFreeFunction)
				return nullptr;
			return createFunctionDefinitionAstFromRust(*functionDefinition);
		}

		auto modifierDefinition = std::find_if(
			_contract.subNodeModifiers.begin(),
			_contract.subNodeModifiers.end(),
			[&](RustParserModifierDefinition const& _modifier)
			{
			return rustAstNodesMatch(_modifier.node, _node);
			}
		);
		if (modifierDefinition != _contract.subNodeModifiers.end())
			return createModifierDefinitionAstFromRust(*modifierDefinition);

		auto usingDirective = std::find_if(
			_contract.subNodeUsingDirectives.begin(),
			_contract.subNodeUsingDirectives.end(),
			[&](RustParserUsingDirective const& _using)
			{
			return rustAstNodesMatch(_using.node, _node);
			}
		);
		if (usingDirective != _contract.subNodeUsingDirectives.end())
			return createUsingDirectiveAstFromRust(*usingDirective);

		auto variableDeclaration = std::find_if(
			_contract.subNodeVariableDeclarations.begin(),
			_contract.subNodeVariableDeclarations.end(),
			[&](RustParserVariableDeclaration const& _variable)
			{
			return rustAstNodesMatch(_variable.node, _node);
			}
		);
		if (variableDeclaration != _contract.subNodeVariableDeclarations.end())
		{
			if (!variableDeclarationIsStateVariableCompatible(*variableDeclaration))
				return nullptr;
			return createVariableDeclarationAstFromRust(*variableDeclaration);
		}

		return nullptr;
	};

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_contract.subNodes.size());
	for (RustParserAstNode const& subNode: _contract.subNodes)
	{
		subNodes.push_back(createSupportedSubNode(subNode));
		if (!subNodes.back())
			return nullptr;
	}

	std::vector<ASTPointer<InheritanceSpecifier>> baseContracts;
	baseContracts.reserve(_contract.baseContracts.size());
	for (RustParserAstNode const& baseContract: _contract.baseContracts)
	{
		auto baseContractDetail = std::find_if(
			_contract.baseContractDetails.begin(),
			_contract.baseContractDetails.end(),
			[&](RustParserInheritanceSpecifier const& _baseContract)
			{
				return rustAstNodesMatch(_baseContract.node, baseContract);
			}
		);
		if (baseContractDetail == _contract.baseContractDetails.end())
			return nullptr;

		baseContracts.push_back(createInheritanceSpecifierAstFromRust(*baseContractDetail));
		if (!baseContracts.back())
			return nullptr;
	}

	return std::make_shared<ContractDefinition>(
		_contract.node.nodeID,
		_contract.node.location,
		astString(_contract.name),
		_contract.nameLocation,
		createStructuredDocumentationAstFromRust(_contract.documentation),
		std::move(baseContracts),
		std::move(subNodes),
		*contractKind,
		_contract.isAbstract,
		std::move(storageLayoutSpecifier)
	);
}

ASTPointer<UserDefinedValueTypeDefinition> solidity::frontend::createUserDefinedValueTypeDefinitionAstFromRust(
	RustParserUserDefinedValueTypeDefinition const& _typeDefinition
)
{
	if (!_typeDefinition.node.present || _typeDefinition.node.kind != rustAstNodeKindUserDefinedValueTypeDefinition)
		return nullptr;
	if (!rustAstNodesMatch(_typeDefinition.typeNameDetail.node, _typeDefinition.typeName))
		return nullptr;

	ASTPointer<TypeName> underlyingType = createTypeNameAstFromRust(_typeDefinition.typeNameDetail);
	if (!underlyingType)
		return nullptr;

	return std::make_shared<UserDefinedValueTypeDefinition>(
		_typeDefinition.node.nodeID,
		_typeDefinition.node.location,
		astString(_typeDefinition.name),
		_typeDefinition.nameLocation,
		std::move(underlyingType)
	);
}

ASTPointer<SourceUnit> solidity::frontend::createSourceUnitAstFromRustIfSupported(
	RustParserResult const& _result
)
{
	if (!_result.ok || !_result.sourceUnit.present || _result.sourceUnit.kind != rustAstNodeKindSourceUnit)
		return nullptr;

	RustParserReconstructionContext context{
		_result.source,
		_result.sourceName,
		_result.evmVersion,
		_result.experimentalSolidity,
	};
	RustParserReconstructionContext const* previousContext = currentRustParserReconstructionContext;
	currentRustParserReconstructionContext = &context;
	ScopeGuard resetContext([&] { currentRustParserReconstructionContext = previousContext; });

	std::vector<ASTPointer<ASTNode>> nodes;
	nodes.reserve(_result.sourceUnitNodes.size());
	for (RustParserAstNode const& node: _result.sourceUnitNodes)
	{
		auto nodeMatches = [&](auto const& _detail)
		{
			return rustAstNodesMatch(_detail.node, node);
		};

		auto pragma = std::find_if(
			_result.sourceUnitPragmas.begin(),
			_result.sourceUnitPragmas.end(),
			nodeMatches
		);
		if (pragma != _result.sourceUnitPragmas.end())
		{
			nodes.push_back(createPragmaDirectiveAstFromRust(*pragma));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto import = std::find_if(
			_result.sourceUnitImports.begin(),
			_result.sourceUnitImports.end(),
			nodeMatches
		);
		if (import != _result.sourceUnitImports.end())
		{
			nodes.push_back(createImportDirectiveAstFromRust(*import));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto contractDefinition = std::find_if(
			_result.sourceUnitContracts.begin(),
			_result.sourceUnitContracts.end(),
			nodeMatches
		);
			if (contractDefinition != _result.sourceUnitContracts.end())
			{
				nodes.push_back(createContractDefinitionAstFromRust(*contractDefinition));
				if (!nodes.back())
					return nullptr;
				continue;
			}

		auto structDefinition = std::find_if(
			_result.sourceUnitStructs.begin(),
			_result.sourceUnitStructs.end(),
			nodeMatches
		);
		if (structDefinition != _result.sourceUnitStructs.end())
		{
			nodes.push_back(createStructDefinitionAstFromRust(*structDefinition));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto enumDefinition = std::find_if(
			_result.sourceUnitEnums.begin(),
			_result.sourceUnitEnums.end(),
			nodeMatches
		);
		if (enumDefinition != _result.sourceUnitEnums.end())
		{
			nodes.push_back(createEnumDefinitionAstFromRust(*enumDefinition));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto eventDefinition = std::find_if(
			_result.sourceUnitEvents.begin(),
			_result.sourceUnitEvents.end(),
			nodeMatches
		);
		if (eventDefinition != _result.sourceUnitEvents.end())
		{
			nodes.push_back(createEventDefinitionAstFromRust(*eventDefinition));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto errorDefinition = std::find_if(
			_result.sourceUnitErrors.begin(),
			_result.sourceUnitErrors.end(),
			nodeMatches
		);
		if (errorDefinition != _result.sourceUnitErrors.end())
		{
			nodes.push_back(createErrorDefinitionAstFromRust(*errorDefinition));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto functionDefinition = std::find_if(
			_result.sourceUnitFunctions.begin(),
			_result.sourceUnitFunctions.end(),
			nodeMatches
		);
			if (functionDefinition != _result.sourceUnitFunctions.end())
			{
				if (!functionDefinition->isFreeFunction)
					return nullptr;
				nodes.push_back(createFunctionDefinitionAstFromRust(*functionDefinition));
				if (!nodes.back())
					return nullptr;
				continue;
			}

		auto forAllQuantifier = std::find_if(
			_result.sourceUnitForAllQuantifiers.begin(),
			_result.sourceUnitForAllQuantifiers.end(),
			nodeMatches
		);
		if (forAllQuantifier != _result.sourceUnitForAllQuantifiers.end())
		{
			nodes.push_back(createForAllQuantifierAstFromRust(*forAllQuantifier));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto typeDefinition = std::find_if(
			_result.sourceUnitTypeDefinitions.begin(),
			_result.sourceUnitTypeDefinitions.end(),
			nodeMatches
		);
		if (typeDefinition != _result.sourceUnitTypeDefinitions.end())
		{
			nodes.push_back(createTypeDefinitionAstFromRust(*typeDefinition));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto typeClassDefinition = std::find_if(
			_result.sourceUnitTypeClassDefinitions.begin(),
			_result.sourceUnitTypeClassDefinitions.end(),
			nodeMatches
		);
		if (typeClassDefinition != _result.sourceUnitTypeClassDefinitions.end())
		{
			nodes.push_back(createTypeClassDefinitionAstFromRust(*typeClassDefinition));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto typeClassInstantiation = std::find_if(
			_result.sourceUnitTypeClassInstantiations.begin(),
			_result.sourceUnitTypeClassInstantiations.end(),
			nodeMatches
		);
		if (typeClassInstantiation != _result.sourceUnitTypeClassInstantiations.end())
		{
			nodes.push_back(createTypeClassInstantiationAstFromRust(*typeClassInstantiation));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto usingDirective = std::find_if(
			_result.sourceUnitUsingDirectives.begin(),
			_result.sourceUnitUsingDirectives.end(),
			nodeMatches
		);
		if (usingDirective != _result.sourceUnitUsingDirectives.end())
		{
			nodes.push_back(createUsingDirectiveAstFromRust(*usingDirective));
			if (!nodes.back())
				return nullptr;
			continue;
		}

		auto variableDeclaration = std::find_if(
			_result.sourceUnitVariableDeclarations.begin(),
			_result.sourceUnitVariableDeclarations.end(),
			nodeMatches
		);
			if (variableDeclaration != _result.sourceUnitVariableDeclarations.end())
			{
				if (!variableDeclarationIsFileLevelCompatible(*variableDeclaration))
					return nullptr;
				nodes.push_back(createVariableDeclarationAstFromRust(*variableDeclaration));
				if (!nodes.back())
					return nullptr;
				continue;
			}

		auto valueType = std::find_if(
			_result.sourceUnitUserDefinedValueTypes.begin(),
			_result.sourceUnitUserDefinedValueTypes.end(),
			nodeMatches
		);
		if (valueType != _result.sourceUnitUserDefinedValueTypes.end())
		{
			nodes.push_back(createUserDefinedValueTypeDefinitionAstFromRust(*valueType));
			if (!nodes.back())
				return nullptr;
			continue;
		}

			return nullptr;
		}

	std::optional<std::string> license;
	if (_result.hasLicense)
		license = _result.license;

	return std::make_shared<SourceUnit>(
		_result.sourceUnit.nodeID,
		_result.sourceUnit.location,
		std::move(license),
		std::move(nodes),
		_result.experimentalSolidity
	);
}

RustParserResult solidity::frontend::parseSourceUnitWithRust(
	CharStream const& _charStream,
	EVMVersion _evmVersion,
	std::int64_t _currentNodeID
)
{
	CharStream sourceCopy(_charStream.source(), _charStream.name(), _charStream.isImportedFromAST());
	Scanner scanner(sourceCopy);

	::rust::Vec<rust_ffi::WireLocatedToken> tokens;
	::rust::Vec<rust_ffi::WireString> commentLiterals;
	::rust::Vec<rust_ffi::WireSourceLocation> commentLocations;
	appendTokensAndComments(scanner, tokens, commentLiterals, commentLocations);

	rust_ffi::set_parser_input_with_evm_version(
		std::move(tokens),
		wireString(sourceCopy.source()),
		std::move(commentLiterals),
		std::move(commentLocations),
		_currentNodeID,
		std::string(VersionString),
		_evmVersion.name()
	);

	auto rustResult = rust_ffi::parse();
	auto sourceName = std::make_shared<std::string const>(_charStream.name());

	RustParserResult result;
	result.ok = rustResult.ok;
	result.errorCode = errorCodeFromWire(rustResult.error_code);
	result.errorMessage = std::string(rustResult.error_message);
	result.source = sourceCopy.source();
	result.sourceName = _charStream.name();
	result.evmVersion = _evmVersion;
	result.errors = cppVectorFromRust<RustParserDiagnostic>(
		rustResult.errors,
		[&](rust_ffi::WireParserError const& _error) { return diagnosticFromWire(_error, sourceName); }
	);
	result.warnings = cppVectorFromRust<RustParserDiagnostic>(
		rustResult.warnings,
		[&](rust_ffi::WireParserError const& _error) { return diagnosticFromWire(_error, sourceName); }
	);
	result.hasLicense = rustResult.has_license;
	result.license = cppString(rustResult.license);
	result.experimentalSolidity = rustResult.experimental_solidity;
	result.maxID = rustResult.max_id;
	if (!result.ok)
		return result;

	result.sourceUnit = astNodeFromWire(rustResult.source_unit, sourceName);
	result.sourceUnitNodes = cppVectorFromRust<RustParserAstNode>(
		rustResult.source_unit_nodes,
		[&](rust_ffi::WireAstNode const& _node) { return astNodeFromWire(_node, sourceName); }
	);
	result.sourceUnitPragmas = cppVectorFromRust<RustParserPragmaDirective>(
		rustResult.source_unit_pragmas,
		[&](rust_ffi::WirePragmaDirectiveResult const& _pragma)
		{
			return pragmaDirectiveFromWire(_pragma, sourceName);
		}
	);
	result.sourceUnitImports = cppVectorFromRust<RustParserImportDirective>(
		rustResult.source_unit_imports,
		[&](rust_ffi::WireImportDirectiveResult const& _import)
		{
			return importDirectiveFromWire(_import, sourceName);
		}
	);
	result.sourceUnitEnums = cppVectorFromRust<RustParserEnumDefinition>(
		rustResult.source_unit_enums,
		[&](rust_ffi::WireEnumDefinitionResult const& _enum)
		{
			return enumDefinitionFromWire(_enum, sourceName);
		}
	);
	result.sourceUnitStructs = cppVectorFromRust<RustParserStructDefinition>(
		rustResult.source_unit_structs,
		[&](rust_ffi::WireStructDefinitionResult const& _struct)
		{
			return structDefinitionFromWire(_struct, sourceName);
		}
	);
	result.sourceUnitEvents = cppVectorFromRust<RustParserEventDefinition>(
		rustResult.source_unit_events,
		[&](rust_ffi::WireEventDefinitionResult const& _event)
		{
			return eventDefinitionFromWire(_event, sourceName);
		}
	);
	result.sourceUnitErrors = cppVectorFromRust<RustParserErrorDefinition>(
		rustResult.source_unit_errors,
		[&](rust_ffi::WireErrorDefinitionResult const& _error)
		{
			return errorDefinitionFromWire(_error, sourceName);
		}
	);
	result.sourceUnitContracts = cppVectorFromRust<RustParserContractDefinition>(
		rustResult.source_unit_contracts,
		[&](rust_ffi::WireContractDefinitionResult const& _contract)
		{
			return contractDefinitionFromWire(_contract, sourceName);
		}
	);
	result.sourceUnitFunctions = cppVectorFromRust<RustParserFunctionDefinition>(
		rustResult.source_unit_functions,
		[&](rust_ffi::WireFunctionDefinitionResult const& _function)
		{
			return functionDefinitionFromWire(_function, sourceName);
		}
	);
	result.sourceUnitForAllQuantifiers = cppVectorFromRust<RustParserForAllQuantifier>(
		rustResult.source_unit_for_all_quantifiers,
		[&](rust_ffi::WireForAllQuantifierResult const& _quantifier)
		{
			return forAllQuantifierFromWire(_quantifier, sourceName);
		}
	);
	result.sourceUnitTypeDefinitions = cppVectorFromRust<RustParserTypeDefinition>(
		rustResult.source_unit_type_definitions,
		[&](rust_ffi::WireTypeDefinitionResult const& _typeDefinition)
		{
			return typeDefinitionFromWire(_typeDefinition, sourceName);
		}
	);
	result.sourceUnitTypeClassDefinitions = cppVectorFromRust<RustParserTypeClassDefinition>(
		rustResult.source_unit_type_class_definitions,
		[&](rust_ffi::WireTypeClassDefinitionResult const& _typeClassDefinition)
		{
			return typeClassDefinitionFromWire(_typeClassDefinition, sourceName);
		}
	);
	result.sourceUnitTypeClassInstantiations = cppVectorFromRust<RustParserTypeClassInstantiation>(
		rustResult.source_unit_type_class_instantiations,
		[&](rust_ffi::WireTypeClassInstantiationResult const& _typeClassInstantiation)
		{
			return typeClassInstantiationFromWire(_typeClassInstantiation, sourceName);
		}
	);
	result.sourceUnitUsingDirectives = cppVectorFromRust<RustParserUsingDirective>(
		rustResult.source_unit_using_directives,
		[&](rust_ffi::WireUsingDirectiveResult const& _using)
		{
			return usingDirectiveFromWire(_using, sourceName);
		}
	);
	result.sourceUnitVariableDeclarations = cppVectorFromRust<RustParserVariableDeclaration>(
		rustResult.source_unit_variable_declarations,
		[&](rust_ffi::WireVariableDeclarationResult const& _variable)
		{
			return variableDeclarationFromWire(_variable, sourceName);
		}
	);
	result.sourceUnitUserDefinedValueTypes = cppVectorFromRust<RustParserUserDefinedValueTypeDefinition>(
		rustResult.source_unit_user_defined_value_types,
		[&](rust_ffi::WireUserDefinedValueTypeDefinitionResult const& _typeDefinition)
		{
			return userDefinedValueTypeDefinitionFromWire(_typeDefinition, sourceName);
		}
	);
	return result;
}

RustParserResult solidity::frontend::parseSourceUnitWithRust(
	CharStream const& _charStream,
	EVMVersion _evmVersion,
	ErrorReporter& _errorReporter,
	std::int64_t _currentNodeID
)
{
	RustParserResult result = parseSourceUnitWithRust(_charStream, _evmVersion, _currentNodeID);
	reportRustParserDiagnostics(_errorReporter, result);
	return result;
}

void solidity::frontend::reportRustParserDiagnostics(
	ErrorReporter& _errorReporter,
	RustParserResult const& _result
)
{
	for (RustParserDiagnostic const& error: _result.errors)
		reportRustParserDiagnostic(_errorReporter, error, false);

	for (RustParserDiagnostic const& warning: _result.warnings)
		reportRustParserDiagnostic(_errorReporter, warning, true);
}

#endif
