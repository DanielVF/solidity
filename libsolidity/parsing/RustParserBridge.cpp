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
#include <chrono>
#include <cstdlib>
#include <functional>
#include <iostream>
#include <iterator>
#include <memory>
#include <optional>
#include <string_view>
#include <unordered_map>

using namespace solidity;
using namespace solidity::frontend;
using namespace solidity::langutil;

namespace
{

namespace rust_ffi = solidity::frontend::rust;

using RustParserPhaseClock = std::chrono::steady_clock;

struct RustParserPhaseTiming
{
	std::int64_t sourcePrepNs = 0;
	std::int64_t scannerInputNs = 0;
	std::int64_t rustParseNs = 0;
	std::int64_t metadataNs = 0;
	std::int64_t astReconstructionNs = 0;
	std::int64_t legacyDropNs = 0;
	std::int64_t totalNs = 0;
	size_t compactExpressionDetails = 0;
	size_t compactStatementDetails = 0;
	size_t compactTryCatchClauseDetails = 0;
};

bool rustParserPhaseTimingEnabled()
{
	static bool const enabled = std::getenv("SOLIDITY_RUST_PARSER_PHASE_TIMINGS") != nullptr;
	return enabled;
}

bool rustParserCompactDebugEnabled()
{
	static bool const enabled = std::getenv("SOLIDITY_RUST_PARSER_COMPACT_DEBUG") != nullptr;
	return enabled;
}

void debugCompactNodeFailure(char const* _context, rust_ffi::WireAstNode const& _node)
{
	if (!rustParserCompactDebugEnabled())
		return;
	std::cerr
		<< "RUST_PARSER_COMPACT_FAIL"
		<< ",context=" << _context
		<< ",kind=" << static_cast<int>(_node.kind)
		<< ",id=" << _node.node_id
		<< ",start=" << _node.location.start
		<< ",end=" << _node.location.end
		<< "\n";
}

void debugCompactNodeFailure(char const* _context, rust_ffi::WireCompactNode const& _node)
{
	if (!rustParserCompactDebugEnabled())
		return;
	std::cerr
		<< "RUST_PARSER_COMPACT_FAIL"
		<< ",context=" << _context
		<< ",kind=" << static_cast<int>(_node.kind)
		<< ",id=" << _node.id
		<< ",start=" << _node.span.start
		<< ",end=" << _node.span.end
		<< "\n";
}

std::int64_t elapsedNs(RustParserPhaseClock::time_point _start, RustParserPhaseClock::time_point _end)
{
	return std::chrono::duration_cast<std::chrono::nanoseconds>(_end - _start).count();
}

struct RustParserReconstructionContext
{
	RustParserReconstructionContext(
		std::string const& _source,
		std::string const& _sourceName,
		EVMVersion _evmVersion,
		bool _experimentalSolidity
	):
		source(_source),
		sourceName(_sourceName),
		evmVersion(_evmVersion),
		experimentalSolidity(_experimentalSolidity)
	{}

	std::string const& source;
	std::string const& sourceName;
	EVMVersion evmVersion;
	bool experimentalSolidity;
	rust_ffi::WireCompactParseOutput const* compactArena = nullptr;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactPragmaDirectiveDetail const*> compactPragmaDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactImportDirectiveDetail const*> compactImportDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactContractDefinitionDetail const*> compactContractDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactInheritanceSpecifierDetail const*> compactInheritanceSpecifierDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactUsingDirectiveDetail const*> compactUsingDirectiveDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactIdentifierPathDetail const*> compactIdentifierPathDetailsByID;
	std::vector<rust_ffi::WireCompactIdentifierPathDetail const*> compactIdentifierPathDetailsByDenseID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactModifierInvocationDetail const*> compactModifierInvocationDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactFunctionDefinitionDetail const*> compactFunctionDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactModifierDefinitionDetail const*> compactModifierDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactEnumValueDetail const*> compactEnumValueDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactEnumDefinitionDetail const*> compactEnumDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactStructDefinitionDetail const*> compactStructDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactEventDefinitionDetail const*> compactEventDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactErrorDefinitionDetail const*> compactErrorDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const*> compactUserDefinedValueTypeDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactForAllQuantifierDetail const*> compactForAllQuantifierDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactTypeDefinitionDetail const*> compactTypeDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactTypeClassNameDetail const*> compactTypeClassNameDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactTypeClassDefinitionDetail const*> compactTypeClassDefinitionDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactTypeClassInstantiationDetail const*> compactTypeClassInstantiationDetailsByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactMappingTypeName const*> compactMappingTypeNamesByID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactTypeNameDetail const*> compactTypeNameDetailsByID;
	std::vector<rust_ffi::WireCompactTypeNameDetail const*> compactTypeNameDetailsByDenseID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactVariableDeclarationDetail const*> compactVariableDeclarationDetailsByID;
	std::vector<rust_ffi::WireCompactVariableDeclarationDetail const*> compactVariableDeclarationDetailsByDenseID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactExpressionDetail const*> compactExpressionDetailsByID;
	std::vector<rust_ffi::WireCompactExpressionDetail const*> compactExpressionDetailsByDenseID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactStatementDetail const*> compactStatementDetailsByID;
	std::vector<rust_ffi::WireCompactStatementDetail const*> compactStatementDetailsByDenseID;
	std::unordered_map<std::int64_t, rust_ffi::WireCompactTryCatchClauseDetail const*> compactTryCatchClauseDetailsByID;
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
constexpr std::uint8_t rustCompactPayloadList = 2;
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

bool wireAstNodesMatch(rust_ffi::WireAstNode const& _left, rust_ffi::WireAstNode const& _right)
{
	return
		_left.present == _right.present &&
		_left.node_id == _right.node_id &&
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
	std::string text;
	if (_node.present && !_node.text.bytes.empty())
		text = cppString(_node.text);

	return RustParserAstNode{
		_node.present,
		_node.node_id,
		_node.kind,
		sourceLocation(_node.location, _sourceName),
		std::move(text),
	};
}

SourceLocation sourceLocation(
	rust_ffi::WireCompactSpan const& _span,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (_span.source_id < 0)
		return SourceLocation{_span.start, _span.end, nullptr};

	return SourceLocation{_span.start, _span.end, _sourceName};
}

std::string compactText(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactTextRef const& _text
)
{
	size_t const start = _text.start;
	size_t const length = _text.len;
	if (start > _arena.text.size() || length > _arena.text.size() - start)
		return {};

	return std::string(
		reinterpret_cast<char const*>(_arena.text.data() + start),
		length
	);
}

RustParserAstNode astNodeFromCompact(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNode const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return RustParserAstNode{
		true,
		_node.id,
		_node.kind,
		sourceLocation(_node.span, _sourceName),
		compactText(_arena, _node.text),
	};
}

rust_ffi::WireAstNode wireAstNodeFromCompact(rust_ffi::WireCompactNode const& _node)
{
	return rust_ffi::WireAstNode{
		true,
		_node.id,
		_node.kind,
		rust_ffi::WireSourceLocation{
			_node.span.start,
			_node.span.end,
			_node.span.source_id,
		},
		rust_ffi::WireString{},
	};
}

rust_ffi::WireCompactNode const* compactNodeAt(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNodeRef const& _node
)
{
	if (_node.index >= _arena.nodes.size())
		return nullptr;
	return &_arena.nodes[_node.index];
}

bool compactNodeRefsMatch(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNodeRef const& _left,
	rust_ffi::WireCompactNodeRef const& _right
)
{
	rust_ffi::WireCompactNode const* left = compactNodeAt(_arena, _left);
	rust_ffi::WireCompactNode const* right = compactNodeAt(_arena, _right);
	return left && right && left->id == right->id && left->kind == right->kind;
}

bool compactNodeMatchesWire(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNodeRef const& _compactNode,
	rust_ffi::WireAstNode const& _wireNode
)
{
	rust_ffi::WireCompactNode const* compactNode = compactNodeAt(_arena, _compactNode);
	return
		_wireNode.present &&
		compactNode &&
		compactNode->id == _wireNode.node_id &&
		compactNode->kind == _wireNode.kind;
}

rust_ffi::WireCompactExpressionDetail const* compactExpressionDetailByNode(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNodeRef const& _node
)
{
	for (rust_ffi::WireCompactExpressionDetail const& detail: _arena.expression_details)
		if (compactNodeRefsMatch(_arena, detail.expression, _node))
			return &detail;
	return nullptr;
}

rust_ffi::WireCompactExpressionDetail const* compactExpressionDetailByWireNode(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireAstNode const& _node
)
{
	for (rust_ffi::WireCompactExpressionDetail const& detail: _arena.expression_details)
		if (compactNodeMatchesWire(_arena, detail.expression, _node))
			return &detail;
	return nullptr;
}

rust_ffi::WireCompactStatementDetail const* compactStatementDetailByWireNode(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireAstNode const& _node
)
{
	for (rust_ffi::WireCompactStatementDetail const& detail: _arena.statement_details)
		if (compactNodeMatchesWire(_arena, detail.statement, _node))
			return &detail;
	return nullptr;
}

rust_ffi::WireCompactTryCatchClauseDetail const* compactTryCatchClauseDetailByWireNode(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireAstNode const& _node
)
{
	for (rust_ffi::WireCompactTryCatchClauseDetail const& detail: _arena.try_catch_clause_details)
		if (compactNodeMatchesWire(_arena, detail.try_catch_clause, _node))
			return &detail;
	return nullptr;
}

bool compactDetailTablesAreIndexed(rust_ffi::WireCompactParseOutput const& _arena)
{
	if (!_arena.expression_details.empty())
	{
		auto const& detail = _arena.expression_details.front();
		rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.expression);
		if (!node)
			return false;
		if (compactExpressionDetailByNode(_arena, detail.expression) == nullptr)
			return false;
		if (compactExpressionDetailByWireNode(_arena, wireAstNodeFromCompact(*node)) == nullptr)
			return false;
	}

	if (!_arena.statement_details.empty())
	{
		auto const& detail = _arena.statement_details.front();
		rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.statement);
		if (!node)
			return false;
		if (compactStatementDetailByWireNode(_arena, wireAstNodeFromCompact(*node)) == nullptr)
			return false;
	}

	if (!_arena.try_catch_clause_details.empty())
	{
		auto const& detail = _arena.try_catch_clause_details.front();
		rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.try_catch_clause);
		if (!node)
			return false;
		if (compactTryCatchClauseDetailByWireNode(_arena, wireAstNodeFromCompact(*node)) == nullptr)
			return false;
	}

	for (rust_ffi::WireCompactExpressionDetail const& detail: _arena.expression_details)
		if (!compactNodeAt(_arena, detail.expression))
			return false;

	for (rust_ffi::WireCompactStatementDetail const& detail: _arena.statement_details)
		if (!compactNodeAt(_arena, detail.statement))
			return false;

	for (rust_ffi::WireCompactTryCatchClauseDetail const& detail: _arena.try_catch_clause_details)
		if (!compactNodeAt(_arena, detail.try_catch_clause))
			return false;

	for (rust_ffi::WireCompactIdentifierPathDetail const& detail: _arena.identifier_path_details)
		if (!compactNodeAt(_arena, detail.identifier_path))
			return false;

	for (rust_ffi::WireCompactModifierInvocationDetail const& detail: _arena.modifier_invocation_details)
		if (!compactNodeAt(_arena, detail.modifier_invocation))
			return false;

	for (rust_ffi::WireCompactFunctionDefinitionDetail const& detail: _arena.function_definition_details)
		if (!compactNodeAt(_arena, detail.function_definition))
			return false;

	for (rust_ffi::WireCompactModifierDefinitionDetail const& detail: _arena.modifier_definition_details)
		if (!compactNodeAt(_arena, detail.modifier_definition))
			return false;

	for (rust_ffi::WireCompactContractDefinitionDetail const& detail: _arena.contract_definition_details)
		if (!compactNodeAt(_arena, detail.contract_definition))
			return false;

	for (rust_ffi::WireCompactInheritanceSpecifierDetail const& detail: _arena.inheritance_specifier_details)
		if (!compactNodeAt(_arena, detail.inheritance_specifier))
			return false;

	for (rust_ffi::WireCompactUsingDirectiveDetail const& detail: _arena.using_directive_details)
		if (!compactNodeAt(_arena, detail.using_directive))
			return false;

	for (rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const& detail:
		_arena.user_defined_value_type_definition_details)
		if (!compactNodeAt(_arena, detail.user_defined_value_type_definition))
			return false;

	for (rust_ffi::WireCompactForAllQuantifierDetail const& detail: _arena.for_all_quantifier_details)
		if (!compactNodeAt(_arena, detail.for_all_quantifier))
			return false;

	for (rust_ffi::WireCompactTypeDefinitionDetail const& detail: _arena.type_definition_details)
		if (!compactNodeAt(_arena, detail.type_definition))
			return false;

	for (rust_ffi::WireCompactTypeClassNameDetail const& detail: _arena.type_class_name_details)
		if (!compactNodeAt(_arena, detail.type_class_name))
			return false;

	for (rust_ffi::WireCompactTypeClassDefinitionDetail const& detail: _arena.type_class_definition_details)
		if (!compactNodeAt(_arena, detail.type_class_definition))
			return false;

	for (rust_ffi::WireCompactTypeClassInstantiationDetail const& detail: _arena.type_class_instantiation_details)
		if (!compactNodeAt(_arena, detail.type_class_instantiation))
			return false;

	return true;
}

bool compactChildRangeIsValid(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactChildRange const& _range
)
{
	size_t const start = _range.start;
	size_t const length = _range.len;
	return start <= _arena.children.size() && length <= _arena.children.size() - start;
}

bool compactRefRangeIsValid(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range
)
{
	size_t const start = _range.start;
	size_t const length = _range.len;
	return start <= _arena.ref_items.size() && length <= _arena.ref_items.size() - start;
}

bool compactNameLocationRangeIsValid(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range
)
{
	size_t const start = _range.start;
	size_t const length = _range.len;
	return start <= _arena.name_locations.size() && length <= _arena.name_locations.size() - start;
}

bool compactTokenLiteralRangeIsValid(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range
)
{
	size_t const start = _range.start;
	size_t const length = _range.len;
	return start <= _arena.token_literals.size() && length <= _arena.token_literals.size() - start;
}

bool compactImportAliasRangeIsValid(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range
)
{
	size_t const start = _range.start;
	size_t const length = _range.len;
	return start <= _arena.import_aliases.size() && length <= _arena.import_aliases.size() - start;
}

bool compactUsingOperatorRangeIsValid(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range
)
{
	size_t const start = _range.start;
	size_t const length = _range.len;
	return start <= _arena.using_operators.size() && length <= _arena.using_operators.size() - start;
}

RustParserAstNode astNodeFromCompactRef(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNodeRef const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, _node);
	if (!node)
		return {};
	return astNodeFromCompact(_arena, *node, _sourceName);
}

struct RustParserCompactNameLocations
{
	std::vector<std::string> names;
	std::vector<SourceLocation> locations;
};

std::optional<RustParserCompactNameLocations> nameLocationsFromCompactRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!compactNameLocationRangeIsValid(_arena, _range))
		return std::nullopt;

	RustParserCompactNameLocations result;
	result.names.reserve(_range.len);
	result.locations.reserve(_range.len);
	size_t const end = static_cast<size_t>(_range.start) + _range.len;
	for (size_t index = _range.start; index < end; ++index)
	{
		auto const& nameLocation = _arena.name_locations[index];
		result.names.push_back(compactText(_arena, nameLocation.text));
		result.locations.push_back(sourceLocation(nameLocation.span, _sourceName));
	}
	return result;
}

void indexCompactReconstructionDetails(
	RustParserReconstructionContext& _context,
	rust_ffi::WireCompactParseOutput const& _arena
)
{
	_context.compactArena = &_arena;
	_context.compactPragmaDetailsByID.reserve(_arena.pragma_directive_details.size());
	for (rust_ffi::WireCompactPragmaDirectiveDetail const& detail: _arena.pragma_directive_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.pragma_directive))
			_context.compactPragmaDetailsByID.emplace(node->id, &detail);
	_context.compactImportDetailsByID.reserve(_arena.import_directive_details.size());
	for (rust_ffi::WireCompactImportDirectiveDetail const& detail: _arena.import_directive_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.import_directive))
			_context.compactImportDetailsByID.emplace(node->id, &detail);
	_context.compactContractDefinitionDetailsByID.reserve(_arena.contract_definition_details.size());
	for (rust_ffi::WireCompactContractDefinitionDetail const& detail: _arena.contract_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.contract_definition))
			_context.compactContractDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactInheritanceSpecifierDetailsByID.reserve(_arena.inheritance_specifier_details.size());
	for (rust_ffi::WireCompactInheritanceSpecifierDetail const& detail: _arena.inheritance_specifier_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.inheritance_specifier))
			_context.compactInheritanceSpecifierDetailsByID.emplace(node->id, &detail);
	_context.compactUsingDirectiveDetailsByID.reserve(_arena.using_directive_details.size());
	for (rust_ffi::WireCompactUsingDirectiveDetail const& detail: _arena.using_directive_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.using_directive))
			_context.compactUsingDirectiveDetailsByID.emplace(node->id, &detail);
	if (_arena.max_id >= 0 && _arena.max_id < 10'000'000)
		_context.compactIdentifierPathDetailsByDenseID.assign(static_cast<size_t>(_arena.max_id) + 1, nullptr);
	else
		_context.compactIdentifierPathDetailsByID.reserve(_arena.identifier_path_details.size());
	for (rust_ffi::WireCompactIdentifierPathDetail const& detail: _arena.identifier_path_details)
	{
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.identifier_path))
		{
			if (
				!_context.compactIdentifierPathDetailsByDenseID.empty() &&
				node->id >= 0 &&
				static_cast<size_t>(node->id) < _context.compactIdentifierPathDetailsByDenseID.size()
			)
				_context.compactIdentifierPathDetailsByDenseID[static_cast<size_t>(node->id)] = &detail;
			else
				_context.compactIdentifierPathDetailsByID.emplace(node->id, &detail);
		}
	}
	_context.compactModifierInvocationDetailsByID.reserve(_arena.modifier_invocation_details.size());
	for (rust_ffi::WireCompactModifierInvocationDetail const& detail: _arena.modifier_invocation_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.modifier_invocation))
			_context.compactModifierInvocationDetailsByID.emplace(node->id, &detail);
	_context.compactFunctionDefinitionDetailsByID.reserve(_arena.function_definition_details.size());
	for (rust_ffi::WireCompactFunctionDefinitionDetail const& detail: _arena.function_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.function_definition))
			_context.compactFunctionDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactModifierDefinitionDetailsByID.reserve(_arena.modifier_definition_details.size());
	for (rust_ffi::WireCompactModifierDefinitionDetail const& detail: _arena.modifier_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.modifier_definition))
			_context.compactModifierDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactEnumValueDetailsByID.reserve(_arena.enum_value_details.size());
	for (rust_ffi::WireCompactEnumValueDetail const& detail: _arena.enum_value_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.enum_value))
			_context.compactEnumValueDetailsByID.emplace(node->id, &detail);
	_context.compactEnumDefinitionDetailsByID.reserve(_arena.enum_definition_details.size());
	for (rust_ffi::WireCompactEnumDefinitionDetail const& detail: _arena.enum_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.enum_definition))
			_context.compactEnumDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactStructDefinitionDetailsByID.reserve(_arena.struct_definition_details.size());
	for (rust_ffi::WireCompactStructDefinitionDetail const& detail: _arena.struct_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.struct_definition))
			_context.compactStructDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactEventDefinitionDetailsByID.reserve(_arena.event_definition_details.size());
	for (rust_ffi::WireCompactEventDefinitionDetail const& detail: _arena.event_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.event_definition))
			_context.compactEventDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactErrorDefinitionDetailsByID.reserve(_arena.error_definition_details.size());
	for (rust_ffi::WireCompactErrorDefinitionDetail const& detail: _arena.error_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.error_definition))
			_context.compactErrorDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactUserDefinedValueTypeDefinitionDetailsByID.reserve(
		_arena.user_defined_value_type_definition_details.size()
	);
	for (rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const& detail:
		_arena.user_defined_value_type_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.user_defined_value_type_definition))
			_context.compactUserDefinedValueTypeDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactForAllQuantifierDetailsByID.reserve(_arena.for_all_quantifier_details.size());
	for (rust_ffi::WireCompactForAllQuantifierDetail const& detail: _arena.for_all_quantifier_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.for_all_quantifier))
			_context.compactForAllQuantifierDetailsByID.emplace(node->id, &detail);
	_context.compactTypeDefinitionDetailsByID.reserve(_arena.type_definition_details.size());
	for (rust_ffi::WireCompactTypeDefinitionDetail const& detail: _arena.type_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.type_definition))
			_context.compactTypeDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactTypeClassNameDetailsByID.reserve(_arena.type_class_name_details.size());
	for (rust_ffi::WireCompactTypeClassNameDetail const& detail: _arena.type_class_name_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.type_class_name))
			_context.compactTypeClassNameDetailsByID.emplace(node->id, &detail);
	_context.compactTypeClassDefinitionDetailsByID.reserve(_arena.type_class_definition_details.size());
	for (rust_ffi::WireCompactTypeClassDefinitionDetail const& detail: _arena.type_class_definition_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.type_class_definition))
			_context.compactTypeClassDefinitionDetailsByID.emplace(node->id, &detail);
	_context.compactTypeClassInstantiationDetailsByID.reserve(_arena.type_class_instantiation_details.size());
	for (rust_ffi::WireCompactTypeClassInstantiationDetail const& detail: _arena.type_class_instantiation_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.type_class_instantiation))
			_context.compactTypeClassInstantiationDetailsByID.emplace(node->id, &detail);
	_context.compactMappingTypeNamesByID.reserve(_arena.mapping_type_names.size());
	for (rust_ffi::WireCompactMappingTypeName const& detail: _arena.mapping_type_names)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.mapping))
			_context.compactMappingTypeNamesByID.emplace(node->id, &detail);
	if (_arena.max_id >= 0 && _arena.max_id < 10'000'000)
		_context.compactTypeNameDetailsByDenseID.assign(static_cast<size_t>(_arena.max_id) + 1, nullptr);
	else
		_context.compactTypeNameDetailsByID.reserve(_arena.type_name_details.size());
	for (rust_ffi::WireCompactTypeNameDetail const& detail: _arena.type_name_details)
	{
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.type_name))
		{
			if (
				!_context.compactTypeNameDetailsByDenseID.empty() &&
				node->id >= 0 &&
				static_cast<size_t>(node->id) < _context.compactTypeNameDetailsByDenseID.size()
			)
				_context.compactTypeNameDetailsByDenseID[static_cast<size_t>(node->id)] = &detail;
			else
				_context.compactTypeNameDetailsByID.emplace(node->id, &detail);
		}
	}
	if (_arena.max_id >= 0 && _arena.max_id < 10'000'000)
		_context.compactVariableDeclarationDetailsByDenseID.assign(static_cast<size_t>(_arena.max_id) + 1, nullptr);
	else
		_context.compactVariableDeclarationDetailsByID.reserve(_arena.variable_declaration_details.size());
	for (rust_ffi::WireCompactVariableDeclarationDetail const& detail: _arena.variable_declaration_details)
	{
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.variable_declaration))
		{
			if (
				!_context.compactVariableDeclarationDetailsByDenseID.empty() &&
				node->id >= 0 &&
				static_cast<size_t>(node->id) < _context.compactVariableDeclarationDetailsByDenseID.size()
			)
				_context.compactVariableDeclarationDetailsByDenseID[static_cast<size_t>(node->id)] = &detail;
			else
				_context.compactVariableDeclarationDetailsByID.emplace(node->id, &detail);
		}
	}
	if (_arena.max_id >= 0 && _arena.max_id < 10'000'000)
		_context.compactExpressionDetailsByDenseID.assign(static_cast<size_t>(_arena.max_id) + 1, nullptr);
	else
		_context.compactExpressionDetailsByID.reserve(_arena.expression_details.size());
	for (rust_ffi::WireCompactExpressionDetail const& detail: _arena.expression_details)
	{
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.expression))
		{
			if (
				!_context.compactExpressionDetailsByDenseID.empty() &&
				node->id >= 0 &&
				static_cast<size_t>(node->id) < _context.compactExpressionDetailsByDenseID.size()
			)
				_context.compactExpressionDetailsByDenseID[static_cast<size_t>(node->id)] = &detail;
			else
				_context.compactExpressionDetailsByID.emplace(node->id, &detail);
		}
	}
	if (_arena.max_id >= 0 && _arena.max_id < 10'000'000)
		_context.compactStatementDetailsByDenseID.assign(static_cast<size_t>(_arena.max_id) + 1, nullptr);
	else
		_context.compactStatementDetailsByID.reserve(_arena.statement_details.size());
	for (rust_ffi::WireCompactStatementDetail const& detail: _arena.statement_details)
	{
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.statement))
		{
			if (
				!_context.compactStatementDetailsByDenseID.empty() &&
				node->id >= 0 &&
				static_cast<size_t>(node->id) < _context.compactStatementDetailsByDenseID.size()
			)
				_context.compactStatementDetailsByDenseID[static_cast<size_t>(node->id)] = &detail;
			else
				_context.compactStatementDetailsByID.emplace(node->id, &detail);
		}
	}
	_context.compactTryCatchClauseDetailsByID.reserve(_arena.try_catch_clause_details.size());
	for (rust_ffi::WireCompactTryCatchClauseDetail const& detail: _arena.try_catch_clause_details)
		if (rust_ffi::WireCompactNode const* node = compactNodeAt(_arena, detail.try_catch_clause))
			_context.compactTryCatchClauseDetailsByID.emplace(node->id, &detail);
}

rust_ffi::WireCompactPragmaDirectiveDetail const* currentCompactPragmaDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactPragmaDetailsByID.find(_node.node_id);
	if (detail == context->compactPragmaDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->pragma_directive, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactImportDirectiveDetail const* currentCompactImportDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactImportDetailsByID.find(_node.node_id);
	if (detail == context->compactImportDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->import_directive, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactContractDefinitionDetail const* currentCompactContractDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactContractDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactContractDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->contract_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactInheritanceSpecifierDetail const* currentCompactInheritanceSpecifierDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactInheritanceSpecifierDetailsByID.find(node->id);
	if (detail == context->compactInheritanceSpecifierDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->inheritance_specifier, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactInheritanceSpecifierDetail const* currentCompactInheritanceSpecifierDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactInheritanceSpecifierDetailsByID.find(_node.node_id);
	if (detail == context->compactInheritanceSpecifierDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->inheritance_specifier, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactUsingDirectiveDetail const* currentCompactUsingDirectiveDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactUsingDirectiveDetailsByID.find(_node.node_id);
	if (detail == context->compactUsingDirectiveDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->using_directive, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactUsingDirectiveDetail const* currentCompactUsingDirectiveDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactUsingDirectiveDetailsByID.find(node->id);
	if (detail == context->compactUsingDirectiveDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->using_directive, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactIdentifierPathDetail const* currentCompactIdentifierPathDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	rust_ffi::WireCompactIdentifierPathDetail const* detail = nullptr;
	if (
		node->id >= 0 &&
		static_cast<size_t>(node->id) < context->compactIdentifierPathDetailsByDenseID.size()
	)
		detail = context->compactIdentifierPathDetailsByDenseID[static_cast<size_t>(node->id)];
	if (!detail)
	{
		auto found = context->compactIdentifierPathDetailsByID.find(node->id);
		if (found == context->compactIdentifierPathDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeRefsMatch(*context->compactArena, detail->identifier_path, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactModifierInvocationDetail const* currentCompactModifierInvocationDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactModifierInvocationDetailsByID.find(node->id);
	if (detail == context->compactModifierInvocationDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->modifier_invocation, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactFunctionDefinitionDetail const* currentCompactFunctionDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactFunctionDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactFunctionDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->function_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactModifierDefinitionDetail const* currentCompactModifierDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactModifierDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactModifierDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->modifier_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactEnumValueDetail const* currentCompactEnumValueDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactEnumValueDetailsByID.find(node->id);
	if (detail == context->compactEnumValueDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->enum_value, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactEnumDefinitionDetail const* currentCompactEnumDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactEnumDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactEnumDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->enum_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactStructDefinitionDetail const* currentCompactStructDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactStructDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactStructDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->struct_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactEventDefinitionDetail const* currentCompactEventDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactEventDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactEventDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->event_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactErrorDefinitionDetail const* currentCompactErrorDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactErrorDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactErrorDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->error_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const*
currentCompactUserDefinedValueTypeDefinitionDetailByWireNode(rust_ffi::WireAstNode const& _node)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactUserDefinedValueTypeDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactUserDefinedValueTypeDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->user_defined_value_type_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const*
currentCompactUserDefinedValueTypeDefinitionDetailByNode(rust_ffi::WireCompactNodeRef const& _node)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactUserDefinedValueTypeDefinitionDetailsByID.find(node->id);
	if (detail == context->compactUserDefinedValueTypeDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->user_defined_value_type_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactForAllQuantifierDetail const* currentCompactForAllQuantifierDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactForAllQuantifierDetailsByID.find(_node.node_id);
	if (detail == context->compactForAllQuantifierDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->for_all_quantifier, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactTypeDefinitionDetail const* currentCompactTypeDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactTypeDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactTypeDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->type_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactTypeClassNameDetail const* currentCompactTypeClassNameDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactTypeClassNameDetailsByID.find(node->id);
	if (detail == context->compactTypeClassNameDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->type_class_name, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactTypeClassDefinitionDetail const* currentCompactTypeClassDefinitionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactTypeClassDefinitionDetailsByID.find(_node.node_id);
	if (detail == context->compactTypeClassDefinitionDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->type_class_definition, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactTypeClassInstantiationDetail const*
currentCompactTypeClassInstantiationDetailByWireNode(rust_ffi::WireAstNode const& _node)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	auto detail = context->compactTypeClassInstantiationDetailsByID.find(_node.node_id);
	if (detail == context->compactTypeClassInstantiationDetailsByID.end())
		return nullptr;
	if (!compactNodeMatchesWire(*context->compactArena, detail->second->type_class_instantiation, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactMappingTypeName const* currentCompactMappingTypeNameByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactMappingTypeNamesByID.find(node->id);
	if (detail == context->compactMappingTypeNamesByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->mapping, _node))
		return nullptr;
	return detail->second;
}

rust_ffi::WireCompactTypeNameDetail const* currentCompactTypeNameDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	rust_ffi::WireCompactTypeNameDetail const* detail = nullptr;
	if (
		node->id >= 0 &&
		static_cast<size_t>(node->id) < context->compactTypeNameDetailsByDenseID.size()
	)
		detail = context->compactTypeNameDetailsByDenseID[static_cast<size_t>(node->id)];
	if (!detail)
	{
		auto found = context->compactTypeNameDetailsByID.find(node->id);
		if (found == context->compactTypeNameDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeRefsMatch(*context->compactArena, detail->type_name, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactVariableDeclarationDetail const* currentCompactVariableDeclarationDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	rust_ffi::WireCompactVariableDeclarationDetail const* detail = nullptr;
	if (
		_node.node_id >= 0 &&
		static_cast<size_t>(_node.node_id) < context->compactVariableDeclarationDetailsByDenseID.size()
	)
		detail = context->compactVariableDeclarationDetailsByDenseID[static_cast<size_t>(_node.node_id)];
	if (!detail)
	{
		auto found = context->compactVariableDeclarationDetailsByID.find(_node.node_id);
		if (found == context->compactVariableDeclarationDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeMatchesWire(*context->compactArena, detail->variable_declaration, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactVariableDeclarationDetail const* currentCompactVariableDeclarationDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	rust_ffi::WireCompactVariableDeclarationDetail const* detail = nullptr;
	if (
		node->id >= 0 &&
		static_cast<size_t>(node->id) < context->compactVariableDeclarationDetailsByDenseID.size()
	)
		detail = context->compactVariableDeclarationDetailsByDenseID[static_cast<size_t>(node->id)];
	if (!detail)
	{
		auto found = context->compactVariableDeclarationDetailsByID.find(node->id);
		if (found == context->compactVariableDeclarationDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeRefsMatch(*context->compactArena, detail->variable_declaration, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactExpressionDetail const* currentCompactExpressionDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;

	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	rust_ffi::WireCompactExpressionDetail const* detail = nullptr;
	if (
		node->id >= 0 &&
		static_cast<size_t>(node->id) < context->compactExpressionDetailsByDenseID.size()
	)
		detail = context->compactExpressionDetailsByDenseID[static_cast<size_t>(node->id)];
	if (!detail)
	{
		auto found = context->compactExpressionDetailsByID.find(node->id);
		if (found == context->compactExpressionDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeRefsMatch(*context->compactArena, detail->expression, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactExpressionDetail const* currentCompactExpressionDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	rust_ffi::WireCompactExpressionDetail const* detail = nullptr;
	if (
		_node.node_id >= 0 &&
		static_cast<size_t>(_node.node_id) < context->compactExpressionDetailsByDenseID.size()
	)
		detail = context->compactExpressionDetailsByDenseID[static_cast<size_t>(_node.node_id)];
	if (!detail)
	{
		auto found = context->compactExpressionDetailsByID.find(_node.node_id);
		if (found == context->compactExpressionDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeMatchesWire(*context->compactArena, detail->expression, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactExpressionDetail const* currentCompactExpressionDetailByAstNode(
	RustParserAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	rust_ffi::WireCompactExpressionDetail const* detail = nullptr;
	if (
		_node.nodeID >= 0 &&
		static_cast<size_t>(_node.nodeID) < context->compactExpressionDetailsByDenseID.size()
	)
		detail = context->compactExpressionDetailsByDenseID[static_cast<size_t>(_node.nodeID)];
	if (!detail)
	{
		auto found = context->compactExpressionDetailsByID.find(_node.nodeID);
		if (found == context->compactExpressionDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, detail->expression);
	if (!node || node->id != _node.nodeID || node->kind != _node.kind)
		return nullptr;
	return detail;
}

rust_ffi::WireCompactStatementDetail const* currentCompactStatementDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	rust_ffi::WireCompactStatementDetail const* detail = nullptr;
	if (
		node->id >= 0 &&
		static_cast<size_t>(node->id) < context->compactStatementDetailsByDenseID.size()
	)
		detail = context->compactStatementDetailsByDenseID[static_cast<size_t>(node->id)];
	if (!detail)
	{
		auto found = context->compactStatementDetailsByID.find(node->id);
		if (found == context->compactStatementDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeRefsMatch(*context->compactArena, detail->statement, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactStatementDetail const* currentCompactStatementDetailByWireNode(
	rust_ffi::WireAstNode const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !_node.present)
		return nullptr;
	rust_ffi::WireCompactStatementDetail const* detail = nullptr;
	if (
		_node.node_id >= 0 &&
		static_cast<size_t>(_node.node_id) < context->compactStatementDetailsByDenseID.size()
	)
		detail = context->compactStatementDetailsByDenseID[static_cast<size_t>(_node.node_id)];
	if (!detail)
	{
		auto found = context->compactStatementDetailsByID.find(_node.node_id);
		if (found == context->compactStatementDetailsByID.end())
			return nullptr;
		detail = found->second;
	}
	if (!compactNodeMatchesWire(*context->compactArena, detail->statement, _node))
		return nullptr;
	return detail;
}

rust_ffi::WireCompactTryCatchClauseDetail const* currentCompactTryCatchClauseDetailByNode(
	rust_ffi::WireCompactNodeRef const& _node
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	auto detail = context->compactTryCatchClauseDetailsByID.find(node->id);
	if (detail == context->compactTryCatchClauseDetailsByID.end())
		return nullptr;
	if (!compactNodeRefsMatch(*context->compactArena, detail->second->try_catch_clause, _node))
		return nullptr;
	return detail->second;
}

std::optional<std::vector<rust_ffi::WireAstNode>> sourceUnitChildNodesFromCompact(
	rust_ffi::WireCompactParseOutput const& _arena
)
{
	if (!_arena.has_root)
		return std::nullopt;

	rust_ffi::WireCompactNode const* root = compactNodeAt(_arena, _arena.root);
	if (!root || root->kind != rustAstNodeKindSourceUnit || root->payload_kind != rustCompactPayloadList)
		return std::nullopt;
	if (!compactChildRangeIsValid(_arena, root->child_range))
		return std::nullopt;

	std::vector<rust_ffi::WireAstNode> nodes;
	nodes.reserve(root->child_range.len);
	size_t const end = static_cast<size_t>(root->child_range.start) + root->child_range.len;
	for (size_t index = root->child_range.start; index < end; ++index)
	{
		rust_ffi::WireCompactNode const* child = compactNodeAt(_arena, _arena.children[index]);
		if (!child)
			return std::nullopt;
		nodes.push_back(wireAstNodeFromCompact(*child));
	}

	return nodes;
}

RustParserDiagnostic diagnosticFromCompact(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactDiagnostic const& _diagnostic,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserDiagnostic diagnostic{
		_diagnostic.error_id,
		compactText(_arena, _diagnostic.message),
		sourceLocation(_diagnostic.span, _sourceName),
		{},
		_diagnostic.syntax,
		_diagnostic.fatal,
	};

	size_t const start = _diagnostic.secondary_locations.start;
	size_t const length = _diagnostic.secondary_locations.len;
	if (start > _arena.secondary_locations.size() || length > _arena.secondary_locations.size() - start)
		return diagnostic;

	diagnostic.secondaryLocations.reserve(length);
	for (size_t index = start; index < start + length; ++index)
	{
		auto const& secondaryLocation = _arena.secondary_locations[index];
		diagnostic.secondaryLocations.emplace_back(
			compactText(_arena, secondaryLocation.message),
			sourceLocation(secondaryLocation.span, _sourceName)
		);
	}

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

template <class T, class Items, class Convert>
std::vector<T> cppVectorFromRust(Items const& _items, Convert _convert)
{
	std::vector<T> output;
	output.reserve(_items.size());
	for (auto const& item: _items)
		output.push_back(_convert(item));
	return output;
}

ASTPointer<ASTString> astStringFromWire(rust_ffi::WireString const& _string)
{
	return astString(cppString(_string));
}

std::vector<ASTPointer<ASTString>> astStringVectorFromWire(
	::rust::Vec<rust_ffi::WireString> const& _strings
)
{
	std::vector<ASTPointer<ASTString>> output;
	output.reserve(_strings.size());
	for (rust_ffi::WireString const& string: _strings)
		output.push_back(astStringFromWire(string));
	return output;
}

template <class Items, class WireNodeFromDetail>
auto findMatchingWireDetailByNode(
	Items const& _items,
	rust_ffi::WireAstNode const& _node,
	WireNodeFromDetail _wireNodeFromDetail
)
{
	return std::find_if(
		_items.begin(),
		_items.end(),
		[&](auto const& _detail)
		{
			return wireAstNodesMatch(_wireNodeFromDetail(_detail), _node);
		}
	);
}

template <class Items, class WireNodeFromDetail>
auto findMatchingWireDetailByNodeWithCursor(
	Items const& _items,
	size_t& _cursor,
	rust_ffi::WireAstNode const& _node,
	WireNodeFromDetail _wireNodeFromDetail
)
{
	using Pointer = decltype(&*_items.begin());
	if (_cursor < _items.size())
	{
		auto const& next = _items[_cursor];
		if (wireAstNodesMatch(_wireNodeFromDetail(next), _node))
		{
			++_cursor;
			return &next;
		}
	}

	auto matched = findMatchingWireDetailByNode(_items, _node, _wireNodeFromDetail);
	if (matched != _items.end())
	{
		_cursor = static_cast<size_t>(std::distance(_items.begin(), matched)) + 1;
		return &*matched;
	}
	return static_cast<Pointer>(nullptr);
}

RustParserExpression expressionFromWire(
	rust_ffi::WireExpressionResult const& _expression,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserExpression result;
	result.node = astNodeFromWire(_expression.expression, _sourceName);

	auto convertExpressionVector = [&](auto const& _items)
	{
		return cppVectorFromRust<RustParserExpression>(
			_items,
			[&](rust_ffi::WireExpressionResult const& _childExpression)
			{
				return expressionFromWire(_childExpression, _sourceName);
			}
		);
	};
	auto convertNodeVector = [&](auto const& _items)
	{
		return cppVectorFromRust<RustParserAstNode>(
			_items,
			[&](rust_ffi::WireAstNode const& _node)
			{
				return astNodeFromWire(_node, _sourceName);
			}
		);
	};
	auto convertStringVector = [](auto const& _items)
	{
		return cppVectorFromRust<std::string>(
			_items,
			[](rust_ffi::WireString const& _string)
			{
				return cppString(_string);
			}
		);
	};
	auto convertLocationVector = [&](auto const& _items)
	{
		return cppVectorFromRust<SourceLocation>(
			_items,
			[&](rust_ffi::WireSourceLocation const& _location)
			{
				return sourceLocation(_location, _sourceName);
			}
		);
	};

	switch (result.node.kind)
	{
	case rustAstNodeKindElementaryTypeNameExpression:
		result.expressionType = astNodeFromWire(_expression.expression_type, _sourceName);
		break;
	case rustAstNodeKindAssignment:
	case rustAstNodeKindBinaryOperation:
		result.leftExpression = astNodeFromWire(_expression.left_expression, _sourceName);
		result.leftExpressionDetail = convertExpressionVector(_expression.left_expression_detail);
		result.rightExpression = astNodeFromWire(_expression.right_expression, _sourceName);
		result.rightExpressionDetail = convertExpressionVector(_expression.right_expression_detail);
		break;
	case rustAstNodeKindConditional:
		result.conditionExpression = astNodeFromWire(_expression.condition_expression, _sourceName);
		result.conditionExpressionDetail = convertExpressionVector(_expression.condition_expression_detail);
		result.trueExpression = astNodeFromWire(_expression.true_expression, _sourceName);
		result.trueExpressionDetail = convertExpressionVector(_expression.true_expression_detail);
		result.falseExpression = astNodeFromWire(_expression.false_expression, _sourceName);
		result.falseExpressionDetail = convertExpressionVector(_expression.false_expression_detail);
		break;
	case rustAstNodeKindUnaryOperation:
		result.subExpression = astNodeFromWire(_expression.sub_expression, _sourceName);
		result.subExpressionDetail = convertExpressionVector(_expression.sub_expression_detail);
		result.isPrefixOperation = _expression.is_prefix_operation;
		break;
	case rustAstNodeKindTupleExpression:
	case rustAstNodeKindInlineArrayExpression:
		result.components = convertNodeVector(_expression.components);
		result.componentDetails = convertExpressionVector(_expression.component_details);
		result.isInlineArray = _expression.is_inline_array;
		break;
	case rustAstNodeKindNewExpression:
		result.typeName = astNodeFromWire(_expression.type_name, _sourceName);
		if (_expression.type_name_details.size() == 1)
			result.typeNameDetail = std::make_shared<RustParserTypeName>(
				typeNameFromWire(_expression.type_name_details[0], _sourceName)
			);
		break;
	case rustAstNodeKindIndexAccess:
		result.baseExpression = astNodeFromWire(_expression.base_expression, _sourceName);
		result.baseExpressionDetail = convertExpressionVector(_expression.base_expression_detail);
		result.indexExpression = astNodeFromWire(_expression.index_expression, _sourceName);
		result.indexExpressionDetail = convertExpressionVector(_expression.index_expression_detail);
		break;
	case rustAstNodeKindIndexRangeAccess:
		result.baseExpression = astNodeFromWire(_expression.base_expression, _sourceName);
		result.baseExpressionDetail = convertExpressionVector(_expression.base_expression_detail);
		result.indexExpression = astNodeFromWire(_expression.index_expression, _sourceName);
		result.indexExpressionDetail = convertExpressionVector(_expression.index_expression_detail);
		result.endIndexExpression = astNodeFromWire(_expression.end_index_expression, _sourceName);
		result.endIndexExpressionDetail = convertExpressionVector(_expression.end_index_expression_detail);
		break;
	case rustAstNodeKindMemberAccess:
		result.baseExpression = astNodeFromWire(_expression.base_expression, _sourceName);
		result.baseExpressionDetail = convertExpressionVector(_expression.base_expression_detail);
		result.memberNameLocation = sourceLocation(_expression.member_name_location, _sourceName);
		break;
	case rustAstNodeKindFunctionCallOptions:
	case rustAstNodeKindFunctionCall:
		result.baseExpression = astNodeFromWire(_expression.base_expression, _sourceName);
		result.baseExpressionDetail = convertExpressionVector(_expression.base_expression_detail);
		result.arguments = convertNodeVector(_expression.arguments);
		result.argumentDetails = convertExpressionVector(_expression.argument_details);
		result.parameterNames = convertStringVector(_expression.parameter_names);
		result.parameterNameLocations = convertLocationVector(_expression.parameter_name_locations);
		break;
	case rustAstNodeKindLiteral:
		result.literalToken = _expression.literal_token;
		result.literalSubdenomination = _expression.literal_subdenomination;
		break;
	default:
		break;
	}

	return result;
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

std::optional<RustParserPragmaDirective> pragmaDirectiveFromCompact(
	rust_ffi::WireCompactPragmaDirectiveDetail const& _pragma,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !compactTokenLiteralRangeIsValid(*context->compactArena, _pragma.token_literals))
		return std::nullopt;

	RustParserPragmaDirective result{
		astNodeFromCompactRef(*context->compactArena, _pragma.pragma_directive, _sourceName),
		{},
		{},
	};
	result.tokens.reserve(_pragma.token_literals.len);
	result.literals.reserve(_pragma.token_literals.len);
	size_t const end = static_cast<size_t>(_pragma.token_literals.start) + _pragma.token_literals.len;
	for (size_t index = _pragma.token_literals.start; index < end; ++index)
	{
		auto const& tokenLiteral = context->compactArena->token_literals[index];
		result.tokens.push_back(static_cast<Token>(tokenLiteral.token));
		result.literals.push_back(compactText(*context->compactArena, tokenLiteral.literal));
	}
	return result;
}

std::optional<RustParserImportDirective> importDirectiveFromCompact(
	rust_ffi::WireCompactImportDirectiveDetail const& _import,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !compactImportAliasRangeIsValid(*context->compactArena, _import.symbol_aliases))
		return std::nullopt;

	RustParserImportDirective result{
		astNodeFromCompactRef(*context->compactArena, _import.import_directive, _sourceName),
		compactText(*context->compactArena, _import.path),
		compactText(*context->compactArena, _import.unit_alias),
		sourceLocation(_import.unit_alias_location, _sourceName),
		{},
	};
	result.symbolAliases.reserve(_import.symbol_aliases.len);
	size_t const end = static_cast<size_t>(_import.symbol_aliases.start) + _import.symbol_aliases.len;
	for (size_t index = _import.symbol_aliases.start; index < end; ++index)
	{
		auto const& alias = context->compactArena->import_aliases[index];
		result.symbolAliases.push_back(RustParserImportSymbolAlias{
			astNodeFromCompactRef(*context->compactArena, alias.symbol, _sourceName),
			alias.has_alias,
			compactText(*context->compactArena, alias.alias),
			sourceLocation(alias.span, _sourceName),
		});
	}
	return result;
}

std::optional<RustParserEnumValue> enumValueFromCompact(
	rust_ffi::WireCompactEnumValueDetail const& _value,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;

	return RustParserEnumValue{
		astNodeFromCompactRef(*context->compactArena, _value.enum_value, _sourceName),
		compactText(*context->compactArena, _value.name),
		sourceLocation(_value.name_location, _sourceName),
		astNodeFromCompactRef(*context->compactArena, _value.documentation, _sourceName),
	};
}

std::optional<RustParserEnumDefinition> enumDefinitionFromCompact(
	rust_ffi::WireCompactEnumDefinitionDetail const& _enum,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena || !compactRefRangeIsValid(*context->compactArena, _enum.members))
		return std::nullopt;

	RustParserEnumDefinition result{
		astNodeFromCompactRef(*context->compactArena, _enum.enum_definition, _sourceName),
		compactText(*context->compactArena, _enum.name),
		sourceLocation(_enum.name_location, _sourceName),
		{},
		astNodeFromCompactRef(*context->compactArena, _enum.documentation, _sourceName),
	};
	result.members.reserve(_enum.members.len);
	size_t const end = static_cast<size_t>(_enum.members.start) + _enum.members.len;
	for (size_t index = _enum.members.start; index < end; ++index)
	{
		rust_ffi::WireCompactEnumValueDetail const* value =
			currentCompactEnumValueDetailByNode(context->compactArena->ref_items[index]);
		if (!value)
			return std::nullopt;
		std::optional<RustParserEnumValue> convertedValue = enumValueFromCompact(*value, _sourceName);
		if (!convertedValue)
			return std::nullopt;
		result.members.push_back(std::move(*convertedValue));
	}
	return result;
}

std::optional<std::vector<RustParserAstNode>> astNodesFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!compactRefRangeIsValid(_arena, _range))
		return std::nullopt;

	std::vector<RustParserAstNode> nodes;
	nodes.reserve(_range.len);
	size_t const end = static_cast<size_t>(_range.start) + _range.len;
	for (size_t index = _range.start; index < end; ++index)
		nodes.push_back(astNodeFromCompactRef(_arena, _arena.ref_items[index], _sourceName));
	return nodes;
}

std::optional<std::vector<RustParserExpression>> expressionPlaceholdersFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
);

std::optional<RustParserExpression> expressionFromCompactRef(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactNodeRef const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	rust_ffi::WireCompactNode const* compactNode = compactNodeAt(_arena, _node);
	if (!compactNode)
		return std::nullopt;

	RustParserExpression expression;
	expression.node = astNodeFromCompact(_arena, *compactNode, _sourceName);
	rust_ffi::WireCompactExpressionDetail const* detail = currentCompactExpressionDetailByNode(_node);
	if (!detail)
		return expression;

	auto singleExpression = [&](rust_ffi::WireCompactNodeRef const& _child)
		-> std::optional<std::vector<RustParserExpression>>
	{
		std::vector<RustParserExpression> result;
		if (!compactNodeAt(_arena, _child))
			return result;
		std::optional<RustParserExpression> child = expressionFromCompactRef(_arena, _child, _sourceName);
		if (!child)
			return std::nullopt;
		result.push_back(std::move(*child));
		return result;
	};

	auto expressionRange = [&](rust_ffi::WireCompactRefRange const& _range)
		-> std::optional<std::vector<RustParserExpression>>
	{
		return expressionPlaceholdersFromCompactRefRange(_arena, _range, _sourceName);
	};

	switch (expression.node.kind)
	{
	case rustAstNodeKindElementaryTypeNameExpression:
		expression.expressionType = astNodeFromCompactRef(_arena, detail->expression_type, _sourceName);
		break;
	case rustAstNodeKindAssignment:
	case rustAstNodeKindBinaryOperation:
		expression.leftExpression = astNodeFromCompactRef(_arena, detail->left_expression, _sourceName);
		if (auto left = singleExpression(detail->left_expression))
			expression.leftExpressionDetail = std::move(*left);
		else
			return std::nullopt;
		expression.rightExpression = astNodeFromCompactRef(_arena, detail->right_expression, _sourceName);
		if (auto right = singleExpression(detail->right_expression))
			expression.rightExpressionDetail = std::move(*right);
		else
			return std::nullopt;
		break;
	case rustAstNodeKindConditional:
		expression.conditionExpression = astNodeFromCompactRef(_arena, detail->condition_expression, _sourceName);
		if (auto condition = singleExpression(detail->condition_expression))
			expression.conditionExpressionDetail = std::move(*condition);
		else
			return std::nullopt;
		expression.trueExpression = astNodeFromCompactRef(_arena, detail->true_expression, _sourceName);
		if (auto trueExpression = singleExpression(detail->true_expression))
			expression.trueExpressionDetail = std::move(*trueExpression);
		else
			return std::nullopt;
		expression.falseExpression = astNodeFromCompactRef(_arena, detail->false_expression, _sourceName);
		if (auto falseExpression = singleExpression(detail->false_expression))
			expression.falseExpressionDetail = std::move(*falseExpression);
		else
			return std::nullopt;
		break;
	case rustAstNodeKindUnaryOperation:
		expression.subExpression = astNodeFromCompactRef(_arena, detail->sub_expression, _sourceName);
		if (auto subExpression = singleExpression(detail->sub_expression))
			expression.subExpressionDetail = std::move(*subExpression);
		else
			return std::nullopt;
		expression.isPrefixOperation = detail->is_prefix_operation;
		break;
	case rustAstNodeKindTupleExpression:
	case rustAstNodeKindInlineArrayExpression:
	{
		std::optional<std::vector<RustParserAstNode>> components =
			astNodesFromCompactRefRange(_arena, detail->components, _sourceName);
		std::optional<std::vector<RustParserExpression>> componentDetails =
			expressionRange(detail->components);
		if (!components || !componentDetails)
			return std::nullopt;
		expression.components = std::move(*components);
		expression.componentDetails = std::move(*componentDetails);
		expression.isInlineArray = detail->is_inline_array;
		break;
	}
	case rustAstNodeKindIndexAccess:
		expression.baseExpression = astNodeFromCompactRef(_arena, detail->base_expression, _sourceName);
		if (auto base = singleExpression(detail->base_expression))
			expression.baseExpressionDetail = std::move(*base);
		else
			return std::nullopt;
		expression.indexExpression = astNodeFromCompactRef(_arena, detail->index_expression, _sourceName);
		if (auto index = singleExpression(detail->index_expression))
			expression.indexExpressionDetail = std::move(*index);
		else
			return std::nullopt;
		break;
	case rustAstNodeKindIndexRangeAccess:
		expression.baseExpression = astNodeFromCompactRef(_arena, detail->base_expression, _sourceName);
		if (auto base = singleExpression(detail->base_expression))
			expression.baseExpressionDetail = std::move(*base);
		else
			return std::nullopt;
		expression.indexExpression = astNodeFromCompactRef(_arena, detail->index_expression, _sourceName);
		if (auto index = singleExpression(detail->index_expression))
			expression.indexExpressionDetail = std::move(*index);
		else
			return std::nullopt;
		expression.endIndexExpression = astNodeFromCompactRef(_arena, detail->end_index_expression, _sourceName);
		if (auto endIndex = singleExpression(detail->end_index_expression))
			expression.endIndexExpressionDetail = std::move(*endIndex);
		else
			return std::nullopt;
		break;
	case rustAstNodeKindMemberAccess:
		expression.baseExpression = astNodeFromCompactRef(_arena, detail->base_expression, _sourceName);
		if (auto base = singleExpression(detail->base_expression))
			expression.baseExpressionDetail = std::move(*base);
		else
			return std::nullopt;
		expression.memberNameLocation = sourceLocation(detail->member_name_location, _sourceName);
		break;
	case rustAstNodeKindFunctionCallOptions:
	case rustAstNodeKindFunctionCall:
	{
		expression.baseExpression = astNodeFromCompactRef(_arena, detail->base_expression, _sourceName);
		if (auto base = singleExpression(detail->base_expression))
			expression.baseExpressionDetail = std::move(*base);
		else
			return std::nullopt;
		std::optional<std::vector<RustParserAstNode>> arguments =
			astNodesFromCompactRefRange(_arena, detail->arguments, _sourceName);
		std::optional<std::vector<RustParserExpression>> argumentDetails =
			expressionRange(detail->arguments);
		std::optional<RustParserCompactNameLocations> names =
			nameLocationsFromCompactRange(_arena, detail->argument_names, _sourceName);
		if (!arguments || !argumentDetails || !names)
			return std::nullopt;
		expression.arguments = std::move(*arguments);
		expression.argumentDetails = std::move(*argumentDetails);
		expression.parameterNames = std::move(names->names);
		expression.parameterNameLocations = std::move(names->locations);
		break;
	}
	case rustAstNodeKindLiteral:
		expression.literalToken = detail->literal_token;
		expression.literalSubdenomination = detail->literal_subdenomination;
		break;
	default:
		break;
	}

	return expression;
}

std::optional<std::vector<RustParserExpression>> expressionPlaceholdersFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!compactRefRangeIsValid(_arena, _range))
		return std::nullopt;

	std::vector<RustParserExpression> expressions;
	expressions.reserve(_range.len);
	size_t const end = static_cast<size_t>(_range.start) + _range.len;
	for (size_t index = _range.start; index < end; ++index)
	{
		rust_ffi::WireCompactNodeRef const& node = _arena.ref_items[index];
		if (!compactNodeAt(_arena, node))
		{
			expressions.emplace_back();
			continue;
		}
		std::optional<RustParserExpression> expression =
			expressionFromCompactRef(_arena, node, _sourceName);
		if (!expression)
			return std::nullopt;
		expressions.push_back(std::move(*expression));
	}
	return expressions;
}

std::optional<RustParserMappingTypeName> mappingTypeNameFromCompact(
	rust_ffi::WireCompactMappingTypeName const& _mapping,
	std::shared_ptr<std::string const> const& _sourceName
);

std::optional<std::vector<RustParserMappingTypeName>> mappingTypeNamesFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!compactRefRangeIsValid(_arena, _range))
		return std::nullopt;

	std::vector<RustParserMappingTypeName> mappings;
	mappings.reserve(_range.len);
	size_t const end = static_cast<size_t>(_range.start) + _range.len;
	for (size_t index = _range.start; index < end; ++index)
	{
		rust_ffi::WireCompactMappingTypeName const* detail =
			currentCompactMappingTypeNameByNode(_arena.ref_items[index]);
		if (!detail)
			return std::nullopt;
		std::optional<RustParserMappingTypeName> mapping = mappingTypeNameFromCompact(*detail, _sourceName);
		if (!mapping)
			return std::nullopt;
		mappings.push_back(std::move(*mapping));
	}
	return mappings;
}

std::optional<RustParserVariableDeclaration> variableDeclarationFromCompact(
	rust_ffi::WireCompactVariableDeclarationDetail const& _variable,
	std::shared_ptr<std::string const> const& _sourceName
);

std::optional<std::vector<RustParserVariableDeclaration>> variableDeclarationsFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
);

std::optional<RustParserMappingTypeName> mappingTypeNameFromCompact(
	rust_ffi::WireCompactMappingTypeName const& _mapping,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	std::optional<RustParserCompactNameLocations> keyPath =
		nameLocationsFromCompactRange(arena, _mapping.key_user_defined_path, _sourceName);
	std::optional<RustParserCompactNameLocations> valuePath =
		nameLocationsFromCompactRange(arena, _mapping.value_user_defined_path, _sourceName);
	std::optional<std::vector<RustParserAstNode>> valueArrayBaseTypes =
		astNodesFromCompactRefRange(arena, _mapping.value_array_base_types, _sourceName);
	std::optional<std::vector<RustParserAstNode>> valueArrayLengths =
		astNodesFromCompactRefRange(arena, _mapping.value_array_lengths, _sourceName);
	std::optional<std::vector<RustParserExpression>> valueArrayLengthDetails =
		expressionPlaceholdersFromCompactRefRange(arena, _mapping.value_array_lengths, _sourceName);
	std::optional<std::vector<RustParserAstNode>> valueFunctionParameterDeclarations =
		astNodesFromCompactRefRange(arena, _mapping.value_function_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserAstNode>> valueFunctionReturnParameterDeclarations =
		astNodesFromCompactRefRange(arena, _mapping.value_function_return_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> valueFunctionParameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _mapping.value_function_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> valueFunctionReturnParameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _mapping.value_function_return_parameter_declarations, _sourceName);
	if (
		!keyPath ||
		!valuePath ||
		!valueArrayBaseTypes ||
		!valueArrayLengths ||
		!valueArrayLengthDetails ||
		!valueFunctionParameterDeclarations ||
		!valueFunctionReturnParameterDeclarations ||
		!valueFunctionParameterDetails ||
		!valueFunctionReturnParameterDetails
	)
		return std::nullopt;

	return RustParserMappingTypeName{
		astNodeFromCompactRef(arena, _mapping.mapping, _sourceName),
		astNodeFromCompactRef(arena, _mapping.key_type, _sourceName),
		_mapping.key_elementary_token,
		_mapping.key_elementary_first_number,
		_mapping.key_elementary_second_number,
		astNodeFromCompactRef(arena, _mapping.key_user_defined_path_node, _sourceName),
		std::move(keyPath->names),
		std::move(keyPath->locations),
		compactText(arena, _mapping.key_name),
		sourceLocation(_mapping.key_name_location, _sourceName),
		astNodeFromCompactRef(arena, _mapping.value_type, _sourceName),
		_mapping.value_elementary_token,
		_mapping.value_elementary_first_number,
		_mapping.value_elementary_second_number,
		_mapping.value_has_state_mutability,
		_mapping.value_state_mutability,
		astNodeFromCompactRef(arena, _mapping.value_user_defined_path_node, _sourceName),
		std::move(valuePath->names),
		std::move(valuePath->locations),
		std::move(*valueArrayBaseTypes),
		std::move(*valueArrayLengths),
		std::move(*valueArrayLengthDetails),
		astNodeFromCompactRef(arena, _mapping.value_function_parameters, _sourceName),
		std::move(*valueFunctionParameterDeclarations),
		std::move(*valueFunctionParameterDetails),
		astNodeFromCompactRef(arena, _mapping.value_function_return_parameters, _sourceName),
		std::move(*valueFunctionReturnParameterDeclarations),
		std::move(*valueFunctionReturnParameterDetails),
		_mapping.value_function_visibility,
		_mapping.value_function_state_mutability,
		compactText(arena, _mapping.value_name),
		sourceLocation(_mapping.value_name_location, _sourceName),
	};
}

std::optional<RustParserTypeName> typeNameFromCompact(
	rust_ffi::WireCompactTypeNameDetail const& _typeName,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	std::optional<RustParserCompactNameLocations> userDefinedPath =
		nameLocationsFromCompactRange(arena, _typeName.user_defined_path, _sourceName);
	std::optional<std::vector<RustParserAstNode>> arrayBaseTypes =
		astNodesFromCompactRefRange(arena, _typeName.array_base_types, _sourceName);
	std::optional<std::vector<RustParserAstNode>> arrayLengths =
		astNodesFromCompactRefRange(arena, _typeName.array_lengths, _sourceName);
	std::optional<std::vector<RustParserExpression>> arrayLengthDetails =
		expressionPlaceholdersFromCompactRefRange(arena, _typeName.array_lengths, _sourceName);
	std::optional<std::vector<RustParserAstNode>> functionParameterDeclarations =
		astNodesFromCompactRefRange(arena, _typeName.function_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserAstNode>> functionReturnParameterDeclarations =
		astNodesFromCompactRefRange(arena, _typeName.function_return_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> functionParameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _typeName.function_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> functionReturnParameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _typeName.function_return_parameter_declarations, _sourceName);
	std::optional<RustParserCompactNameLocations> mappingKeyPath =
		nameLocationsFromCompactRange(arena, _typeName.mapping_key_user_defined_path, _sourceName);
	std::optional<RustParserCompactNameLocations> mappingValuePath =
		nameLocationsFromCompactRange(arena, _typeName.mapping_value_user_defined_path, _sourceName);
	std::optional<std::vector<RustParserAstNode>> mappingValueArrayBaseTypes =
		astNodesFromCompactRefRange(arena, _typeName.mapping_value_array_base_types, _sourceName);
	std::optional<std::vector<RustParserAstNode>> mappingValueArrayLengths =
		astNodesFromCompactRefRange(arena, _typeName.mapping_value_array_lengths, _sourceName);
	std::optional<std::vector<RustParserExpression>> mappingValueArrayLengthDetails =
		expressionPlaceholdersFromCompactRefRange(arena, _typeName.mapping_value_array_lengths, _sourceName);
	std::optional<std::vector<RustParserAstNode>> mappingValueFunctionParameterDeclarations =
		astNodesFromCompactRefRange(arena, _typeName.mapping_value_function_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserAstNode>> mappingValueFunctionReturnParameterDeclarations =
		astNodesFromCompactRefRange(arena, _typeName.mapping_value_function_return_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> mappingValueFunctionParameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _typeName.mapping_value_function_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> mappingValueFunctionReturnParameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _typeName.mapping_value_function_return_parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserMappingTypeName>> mappingDetails =
		mappingTypeNamesFromCompactRefRange(arena, _typeName.mapping_details, _sourceName);
	if (
		!userDefinedPath ||
		!arrayBaseTypes ||
		!arrayLengths ||
		!arrayLengthDetails ||
		!functionParameterDeclarations ||
		!functionReturnParameterDeclarations ||
		!functionParameterDetails ||
		!functionReturnParameterDetails ||
		!mappingKeyPath ||
		!mappingValuePath ||
		!mappingValueArrayBaseTypes ||
		!mappingValueArrayLengths ||
		!mappingValueArrayLengthDetails ||
		!mappingValueFunctionParameterDeclarations ||
		!mappingValueFunctionReturnParameterDeclarations ||
		!mappingValueFunctionParameterDetails ||
		!mappingValueFunctionReturnParameterDetails ||
		!mappingDetails
	)
		return std::nullopt;

	return RustParserTypeName{
		astNodeFromCompactRef(arena, _typeName.type_name, _sourceName),
		_typeName.elementary_token,
		_typeName.elementary_first_number,
		_typeName.elementary_second_number,
		_typeName.has_state_mutability,
		_typeName.state_mutability,
		astNodeFromCompactRef(arena, _typeName.user_defined_path_node, _sourceName),
		std::move(userDefinedPath->names),
		std::move(userDefinedPath->locations),
		std::move(*arrayBaseTypes),
		std::move(*arrayLengths),
		std::move(*arrayLengthDetails),
		astNodeFromCompactRef(arena, _typeName.function_parameters, _sourceName),
		std::move(*functionParameterDeclarations),
		std::move(*functionParameterDetails),
		astNodeFromCompactRef(arena, _typeName.function_return_parameters, _sourceName),
		std::move(*functionReturnParameterDeclarations),
		std::move(*functionReturnParameterDetails),
		_typeName.function_visibility,
		_typeName.function_state_mutability,
		astNodeFromCompactRef(arena, _typeName.mapping_key_type, _sourceName),
		_typeName.mapping_key_elementary_token,
		_typeName.mapping_key_elementary_first_number,
		_typeName.mapping_key_elementary_second_number,
		astNodeFromCompactRef(arena, _typeName.mapping_key_user_defined_path_node, _sourceName),
		std::move(mappingKeyPath->names),
		std::move(mappingKeyPath->locations),
		compactText(arena, _typeName.mapping_key_name),
		sourceLocation(_typeName.mapping_key_name_location, _sourceName),
		astNodeFromCompactRef(arena, _typeName.mapping_value_type, _sourceName),
		_typeName.mapping_value_elementary_token,
		_typeName.mapping_value_elementary_first_number,
		_typeName.mapping_value_elementary_second_number,
		_typeName.mapping_value_has_state_mutability,
		_typeName.mapping_value_state_mutability,
		astNodeFromCompactRef(arena, _typeName.mapping_value_user_defined_path_node, _sourceName),
		std::move(mappingValuePath->names),
		std::move(mappingValuePath->locations),
		std::move(*mappingValueArrayBaseTypes),
		std::move(*mappingValueArrayLengths),
		std::move(*mappingValueArrayLengthDetails),
		astNodeFromCompactRef(arena, _typeName.mapping_value_function_parameters, _sourceName),
		std::move(*mappingValueFunctionParameterDeclarations),
		std::move(*mappingValueFunctionParameterDetails),
		astNodeFromCompactRef(arena, _typeName.mapping_value_function_return_parameters, _sourceName),
		std::move(*mappingValueFunctionReturnParameterDeclarations),
		std::move(*mappingValueFunctionReturnParameterDetails),
		_typeName.mapping_value_function_visibility,
		_typeName.mapping_value_function_state_mutability,
		compactText(arena, _typeName.mapping_value_name),
		sourceLocation(_typeName.mapping_value_name_location, _sourceName),
		std::move(*mappingDetails),
	};
}

std::optional<RustParserUserDefinedValueTypeDefinition> userDefinedValueTypeDefinitionFromCompact(
	rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const& _typeDefinition,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	rust_ffi::WireCompactTypeNameDetail const* typeNameDetail =
		currentCompactTypeNameDetailByNode(_typeDefinition.type_name);
	if (!typeNameDetail)
		return std::nullopt;
	std::optional<RustParserTypeName> typeName = typeNameFromCompact(*typeNameDetail, _sourceName);
	if (!typeName)
		return std::nullopt;

	return RustParserUserDefinedValueTypeDefinition{
		astNodeFromCompactRef(arena, _typeDefinition.user_defined_value_type_definition, _sourceName),
		compactText(arena, _typeDefinition.name),
		sourceLocation(_typeDefinition.name_location, _sourceName),
		astNodeFromCompactRef(arena, _typeDefinition.type_name, _sourceName),
		_typeDefinition.type_name_elementary_token,
		_typeDefinition.type_name_elementary_first_number,
		_typeDefinition.type_name_elementary_second_number,
		_typeDefinition.type_name_has_state_mutability,
		_typeDefinition.type_name_state_mutability,
		std::move(*typeName),
	};
}

std::optional<RustParserIdentifierPath> identifierPathFromCompact(
	rust_ffi::WireCompactIdentifierPathDetail const& _path,
	std::shared_ptr<std::string const> const& _sourceName
);

std::optional<std::vector<RustParserIdentifierPath>> identifierPathsFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
);

std::optional<RustParserVariableDeclaration> variableDeclarationFromCompact(
	rust_ffi::WireCompactVariableDeclarationDetail const& _variable,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	if (!compactRefRangeIsValid(arena, _variable.override_paths))
		return std::nullopt;
	if (compactNodeAt(arena, _variable.type_expression))
		return std::nullopt;
	std::optional<std::vector<RustParserAstNode>> overridePaths =
		astNodesFromCompactRefRange(arena, _variable.override_paths, _sourceName);
	std::optional<std::vector<RustParserIdentifierPath>> overridePathDetails =
		identifierPathsFromCompactRefRange(arena, _variable.override_paths, _sourceName);
	if (!overridePaths || !overridePathDetails)
		return std::nullopt;

	RustParserVariableDeclaration result;
	result.node = astNodeFromCompactRef(arena, _variable.variable_declaration, _sourceName);
	result.typeName = astNodeFromCompactRef(arena, _variable.type_name, _sourceName);
	result.typeExpression = astNodeFromCompactRef(arena, _variable.type_expression, _sourceName);
	result.documentation = astNodeFromCompactRef(arena, _variable.documentation, _sourceName);
	result.overrides = astNodeFromCompactRef(arena, _variable.overrides, _sourceName);
	result.overridePaths = std::move(*overridePaths);
	result.overridePathDetails = std::move(*overridePathDetails);
	result.value = astNodeFromCompactRef(arena, _variable.value, _sourceName);
	result.name = compactText(arena, _variable.name);
	result.nameLocation = sourceLocation(_variable.name_location, _sourceName);
	result.visibility = _variable.visibility;
	result.mutability = _variable.mutability;
	result.variableLocation = _variable.variable_location;
	result.indexed = _variable.indexed;

	if (result.typeName.present)
	{
		rust_ffi::WireCompactTypeNameDetail const* typeNameDetail =
			currentCompactTypeNameDetailByNode(_variable.type_name);
		if (!typeNameDetail)
			return std::nullopt;
		std::optional<RustParserTypeName> typeName = typeNameFromCompact(*typeNameDetail, _sourceName);
		if (!typeName || !rustAstNodesMatch(result.typeName, typeName->node))
			return std::nullopt;

		result.typeNameElementaryToken = typeName->elementaryToken;
		result.typeNameElementaryFirstNumber = typeName->elementaryFirstNumber;
		result.typeNameElementarySecondNumber = typeName->elementarySecondNumber;
		result.typeNameHasStateMutability = typeName->hasStateMutability;
		result.typeNameStateMutability = typeName->stateMutability;
		result.typeNameUserDefinedPathNode = typeName->userDefinedPathNode;
		result.typeNameUserDefinedPath = std::move(typeName->userDefinedPath);
		result.typeNameUserDefinedPathLocations = std::move(typeName->userDefinedPathLocations);
		result.typeNameArrayBaseTypes = std::move(typeName->arrayBaseTypes);
		result.typeNameArrayLengths = std::move(typeName->arrayLengths);
		result.typeNameArrayLengthDetails = std::move(typeName->arrayLengthDetails);
		result.typeNameFunctionParameters = typeName->functionParameters;
		result.typeNameFunctionParameterDeclarations = std::move(typeName->functionParameterDeclarations);
		result.typeNameFunctionParameterDetails = std::move(typeName->functionParameterDetails);
		result.typeNameFunctionReturnParameters = typeName->functionReturnParameters;
		result.typeNameFunctionReturnParameterDeclarations = std::move(typeName->functionReturnParameterDeclarations);
		result.typeNameFunctionReturnParameterDetails = std::move(typeName->functionReturnParameterDetails);
		result.typeNameFunctionVisibility = typeName->functionVisibility;
		result.typeNameFunctionStateMutability = typeName->functionStateMutability;
		result.typeNameMappingKeyType = typeName->mappingKeyType;
		result.typeNameMappingKeyElementaryToken = typeName->mappingKeyElementaryToken;
		result.typeNameMappingKeyElementaryFirstNumber = typeName->mappingKeyElementaryFirstNumber;
		result.typeNameMappingKeyElementarySecondNumber = typeName->mappingKeyElementarySecondNumber;
		result.typeNameMappingKeyUserDefinedPathNode = typeName->mappingKeyUserDefinedPathNode;
		result.typeNameMappingKeyUserDefinedPath = std::move(typeName->mappingKeyUserDefinedPath);
		result.typeNameMappingKeyUserDefinedPathLocations = std::move(typeName->mappingKeyUserDefinedPathLocations);
		result.typeNameMappingKeyName = std::move(typeName->mappingKeyName);
		result.typeNameMappingKeyNameLocation = typeName->mappingKeyNameLocation;
		result.typeNameMappingValueType = typeName->mappingValueType;
		result.typeNameMappingValueElementaryToken = typeName->mappingValueElementaryToken;
		result.typeNameMappingValueElementaryFirstNumber = typeName->mappingValueElementaryFirstNumber;
		result.typeNameMappingValueElementarySecondNumber = typeName->mappingValueElementarySecondNumber;
		result.typeNameMappingValueHasStateMutability = typeName->mappingValueHasStateMutability;
		result.typeNameMappingValueStateMutability = typeName->mappingValueStateMutability;
		result.typeNameMappingValueUserDefinedPathNode = typeName->mappingValueUserDefinedPathNode;
		result.typeNameMappingValueUserDefinedPath = std::move(typeName->mappingValueUserDefinedPath);
		result.typeNameMappingValueUserDefinedPathLocations = std::move(typeName->mappingValueUserDefinedPathLocations);
		result.typeNameMappingValueArrayBaseTypes = std::move(typeName->mappingValueArrayBaseTypes);
		result.typeNameMappingValueArrayLengths = std::move(typeName->mappingValueArrayLengths);
		result.typeNameMappingValueArrayLengthDetails = std::move(typeName->mappingValueArrayLengthDetails);
		result.typeNameMappingValueFunctionParameters = typeName->mappingValueFunctionParameters;
		result.typeNameMappingValueFunctionParameterDeclarations = std::move(typeName->mappingValueFunctionParameterDeclarations);
		result.typeNameMappingValueFunctionParameterDetails = std::move(typeName->mappingValueFunctionParameterDetails);
		result.typeNameMappingValueFunctionReturnParameters = typeName->mappingValueFunctionReturnParameters;
		result.typeNameMappingValueFunctionReturnParameterDeclarations =
			std::move(typeName->mappingValueFunctionReturnParameterDeclarations);
		result.typeNameMappingValueFunctionReturnParameterDetails =
			std::move(typeName->mappingValueFunctionReturnParameterDetails);
		result.typeNameMappingValueFunctionVisibility = typeName->mappingValueFunctionVisibility;
		result.typeNameMappingValueFunctionStateMutability = typeName->mappingValueFunctionStateMutability;
		result.typeNameMappingValueName = std::move(typeName->mappingValueName);
		result.typeNameMappingValueNameLocation = typeName->mappingValueNameLocation;
		result.typeNameMappingDetails = std::move(typeName->mappingDetails);
	}

	return result;
}

std::optional<std::vector<RustParserVariableDeclaration>> variableDeclarationsFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!compactRefRangeIsValid(_arena, _range))
		return std::nullopt;

	std::vector<RustParserVariableDeclaration> variables;
	variables.reserve(_range.len);
	size_t const end = static_cast<size_t>(_range.start) + _range.len;
	for (size_t index = _range.start; index < end; ++index)
	{
		rust_ffi::WireCompactVariableDeclarationDetail const* detail =
			currentCompactVariableDeclarationDetailByNode(_arena.ref_items[index]);
		if (!detail)
			return std::nullopt;
		std::optional<RustParserVariableDeclaration> variable =
			variableDeclarationFromCompact(*detail, _sourceName);
		if (!variable)
			return std::nullopt;
		variables.push_back(std::move(*variable));
	}
	return variables;
}

std::optional<RustParserIdentifierPath> identifierPathFromCompact(
	rust_ffi::WireCompactIdentifierPathDetail const& _path,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	std::optional<RustParserCompactNameLocations> path =
		nameLocationsFromCompactRange(*context->compactArena, _path.path, _sourceName);
	if (!path)
		return std::nullopt;

	return RustParserIdentifierPath{
		astNodeFromCompactRef(*context->compactArena, _path.identifier_path, _sourceName),
		std::move(path->names),
		std::move(path->locations),
	};
}

std::optional<std::vector<RustParserIdentifierPath>> identifierPathsFromCompactRefRange(
	rust_ffi::WireCompactParseOutput const& _arena,
	rust_ffi::WireCompactRefRange const& _range,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!compactRefRangeIsValid(_arena, _range))
		return std::nullopt;

	std::vector<RustParserIdentifierPath> paths;
	paths.reserve(_range.len);
	size_t const end = static_cast<size_t>(_range.start) + _range.len;
	for (size_t index = _range.start; index < end; ++index)
	{
		rust_ffi::WireCompactIdentifierPathDetail const* detail =
			currentCompactIdentifierPathDetailByNode(_arena.ref_items[index]);
		if (!detail)
			return std::nullopt;
		std::optional<RustParserIdentifierPath> path = identifierPathFromCompact(*detail, _sourceName);
		if (!path)
			return std::nullopt;
		paths.push_back(std::move(*path));
	}
	return paths;
}

std::optional<RustParserUsingDirective> usingDirectiveFromCompact(
	rust_ffi::WireCompactUsingDirectiveDetail const& _using,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	if (
		!compactRefRangeIsValid(arena, _using.functions) ||
		!compactUsingOperatorRangeIsValid(arena, _using.operators) ||
		_using.functions.len != _using.operators.len
	)
		return std::nullopt;

	std::optional<std::vector<RustParserAstNode>> functions =
		astNodesFromCompactRefRange(arena, _using.functions, _sourceName);
	std::optional<std::vector<RustParserIdentifierPath>> functionDetails =
		identifierPathsFromCompactRefRange(arena, _using.functions, _sourceName);
	if (!functions || !functionDetails)
		return std::nullopt;

	RustParserUsingDirective result;
	result.node = astNodeFromCompactRef(arena, _using.using_directive, _sourceName);
	result.functions = std::move(*functions);
	result.functionDetails = std::move(*functionDetails);
	result.operators.reserve(_using.operators.len);
	size_t const end = static_cast<size_t>(_using.operators.start) + _using.operators.len;
	for (size_t index = _using.operators.start; index < end; ++index)
		result.operators.push_back(RustParserUsingOperator{
			arena.using_operators[index].present,
			arena.using_operators[index].token,
		});
	result.usesBraces = _using.uses_braces;
	result.typeName = astNodeFromCompactRef(arena, _using.type_name, _sourceName);
	if (result.typeName.present)
	{
		rust_ffi::WireCompactTypeNameDetail const* typeNameDetail =
			currentCompactTypeNameDetailByNode(_using.type_name);
		if (!typeNameDetail)
			return std::nullopt;
		std::optional<RustParserTypeName> typeName = typeNameFromCompact(*typeNameDetail, _sourceName);
		if (!typeName)
			return std::nullopt;
		result.typeNameDetail = std::move(*typeName);
	}
	result.global = _using.global;
	return result;
}

std::optional<RustParserStructDefinition> structDefinitionFromCompact(
	rust_ffi::WireCompactStructDefinitionDetail const& _struct,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	std::optional<std::vector<RustParserAstNode>> members =
		astNodesFromCompactRefRange(arena, _struct.members, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> memberDetails =
		variableDeclarationsFromCompactRefRange(arena, _struct.members, _sourceName);
	if (!members || !memberDetails)
		return std::nullopt;

	return RustParserStructDefinition{
		astNodeFromCompactRef(arena, _struct.struct_definition, _sourceName),
		compactText(arena, _struct.name),
		sourceLocation(_struct.name_location, _sourceName),
		std::move(*members),
		std::move(*memberDetails),
		astNodeFromCompactRef(arena, _struct.documentation, _sourceName),
	};
}

std::optional<RustParserEventDefinition> eventDefinitionFromCompact(
	rust_ffi::WireCompactEventDefinitionDetail const& _event,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	std::optional<std::vector<RustParserAstNode>> parameterDeclarations =
		astNodesFromCompactRefRange(arena, _event.parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> parameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _event.parameter_declarations, _sourceName);
	if (!parameterDeclarations || !parameterDetails)
		return std::nullopt;

	return RustParserEventDefinition{
		astNodeFromCompactRef(arena, _event.event_definition, _sourceName),
		compactText(arena, _event.name),
		sourceLocation(_event.name_location, _sourceName),
		astNodeFromCompactRef(arena, _event.documentation, _sourceName),
		astNodeFromCompactRef(arena, _event.parameters, _sourceName),
		std::move(*parameterDeclarations),
		std::move(*parameterDetails),
		_event.anonymous,
	};
}

std::optional<RustParserErrorDefinition> errorDefinitionFromCompact(
	rust_ffi::WireCompactErrorDefinitionDetail const& _error,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return std::nullopt;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	std::optional<std::vector<RustParserAstNode>> parameterDeclarations =
		astNodesFromCompactRefRange(arena, _error.parameter_declarations, _sourceName);
	std::optional<std::vector<RustParserVariableDeclaration>> parameterDetails =
		variableDeclarationsFromCompactRefRange(arena, _error.parameter_declarations, _sourceName);
	if (!parameterDeclarations || !parameterDetails)
		return std::nullopt;

	return RustParserErrorDefinition{
		astNodeFromCompactRef(arena, _error.error_definition, _sourceName),
		compactText(arena, _error.name),
		sourceLocation(_error.name_location, _sourceName),
		astNodeFromCompactRef(arena, _error.documentation, _sourceName),
		astNodeFromCompactRef(arena, _error.parameters, _sourceName),
		std::move(*parameterDeclarations),
		std::move(*parameterDetails),
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
	RustParserStatement result;
	result.node = astNodeFromWire(_statement.statement, _sourceName);

	auto convertNodeVector = [&](auto const& _items)
	{
		return cppVectorFromRust<RustParserAstNode>(
			_items,
			[&](rust_ffi::WireAstNode const& _node)
			{
				return astNodeFromWire(_node, _sourceName);
			}
		);
	};
	auto convertExpressionVector = [&](auto const& _items)
	{
		return cppVectorFromRust<RustParserExpression>(
			_items,
			[&](rust_ffi::WireExpressionResult const& _expression)
			{
				return expressionFromWire(_expression, _sourceName);
			}
		);
	};
	auto convertStringVector = [](auto const& _items)
	{
		return cppVectorFromRust<std::string>(
			_items,
			[](rust_ffi::WireString const& _string)
			{
				return cppString(_string);
			}
		);
	};
	auto convertLocationVector = [&](auto const& _items)
	{
		return cppVectorFromRust<SourceLocation>(
			_items,
			[&](rust_ffi::WireSourceLocation const& _location)
			{
				return sourceLocation(_location, _sourceName);
			}
		);
	};

	switch (result.node.kind)
	{
	case rustAstNodeKindBlock:
	case rustAstNodeKindUncheckedBlock:
		result.blockUnchecked = _statement.block_unchecked;
		result.blockStatements = convertNodeVector(_statement.block_statements);
		result.blockStatementDetails = statementsFromWire(_statement.block_statement_details, _sourceName);
		break;
	case rustAstNodeKindReturnStatement:
		result.expression = astNodeFromWire(_statement.expression, _sourceName);
		if (!_statement.expression_detail.empty())
			result.expressionDetail = expressionFromWire(_statement.expression_detail.front(), _sourceName);
		break;
	case rustAstNodeKindEmitStatement:
		result.eventCall = astNodeFromWire(_statement.event_call, _sourceName);
		result.eventCallCallee = astNodeFromWire(_statement.event_call_callee, _sourceName);
		if (!_statement.event_call_callee_detail.empty())
			result.eventCallCalleeDetail = expressionFromWire(_statement.event_call_callee_detail.front(), _sourceName);
		result.eventCallArguments = convertNodeVector(_statement.event_call_arguments);
		result.eventCallArgumentDetails = convertExpressionVector(_statement.event_call_argument_details);
		result.eventCallParameterNames = convertStringVector(_statement.event_call_parameter_names);
		result.eventCallParameterNameLocations = convertLocationVector(_statement.event_call_parameter_name_locations);
		break;
	case rustAstNodeKindRevertStatement:
		result.errorCall = astNodeFromWire(_statement.error_call, _sourceName);
		result.errorCallCallee = astNodeFromWire(_statement.error_call_callee, _sourceName);
		if (!_statement.error_call_callee_detail.empty())
			result.errorCallCalleeDetail = expressionFromWire(_statement.error_call_callee_detail.front(), _sourceName);
		result.errorCallArguments = convertNodeVector(_statement.error_call_arguments);
		result.errorCallArgumentDetails = convertExpressionVector(_statement.error_call_argument_details);
		result.errorCallParameterNames = convertStringVector(_statement.error_call_parameter_names);
		result.errorCallParameterNameLocations = convertLocationVector(_statement.error_call_parameter_name_locations);
		break;
	case rustAstNodeKindInlineAssembly:
		result.inlineAssemblyFlags = convertStringVector(_statement.inline_assembly_flags);
		result.inlineAssemblyBlockLocation = sourceLocation(_statement.inline_assembly_block_location, _sourceName);
		break;
	case rustAstNodeKindIfStatement:
		result.conditionExpression = astNodeFromWire(_statement.condition_expression, _sourceName);
		if (!_statement.condition_expression_detail.empty())
			result.conditionExpressionDetail = expressionFromWire(_statement.condition_expression_detail.front(), _sourceName);
		result.trueBody = astNodeFromWire(_statement.true_body, _sourceName);
		result.trueBodyDetail = statementsFromWire(_statement.true_body_detail, _sourceName);
		result.falseBody = astNodeFromWire(_statement.false_body, _sourceName);
		if (result.falseBody.present || !_statement.false_body_detail.empty())
			result.falseBodyDetail = statementsFromWire(_statement.false_body_detail, _sourceName);
		break;
	case rustAstNodeKindTryStatement:
		result.externalCall = astNodeFromWire(_statement.external_call, _sourceName);
		if (!_statement.external_call_detail.empty())
			result.externalCallDetail = expressionFromWire(_statement.external_call_detail.front(), _sourceName);
		result.clauses = convertNodeVector(_statement.clauses);
		result.clauseDetails = cppVectorFromRust<RustParserTryCatchClause>(
			_statement.clause_details,
			[&](rust_ffi::WireTryCatchClauseResult const& _clause)
			{
				return tryCatchClauseFromWire(_clause, _sourceName);
			}
		);
		result.clauseBlockStatementDetails = statementsFromWire(_statement.clause_block_statement_details, _sourceName);
		break;
	case rustAstNodeKindWhileStatement:
	case rustAstNodeKindDoWhileStatement:
		result.conditionExpression = astNodeFromWire(_statement.condition_expression, _sourceName);
		if (!_statement.condition_expression_detail.empty())
			result.conditionExpressionDetail = expressionFromWire(_statement.condition_expression_detail.front(), _sourceName);
		result.body = astNodeFromWire(_statement.body, _sourceName);
		result.bodyDetail = statementsFromWire(_statement.body_detail, _sourceName);
		result.isDoWhile = _statement.is_do_while;
		break;
	case rustAstNodeKindForStatement:
		result.initExpression = astNodeFromWire(_statement.init_expression, _sourceName);
		if (result.initExpression.present || !_statement.init_expression_detail.empty())
			result.initExpressionDetail = statementsFromWire(_statement.init_expression_detail, _sourceName);
		result.conditionExpression = astNodeFromWire(_statement.condition_expression, _sourceName);
		if (!_statement.condition_expression_detail.empty())
			result.conditionExpressionDetail = expressionFromWire(_statement.condition_expression_detail.front(), _sourceName);
		result.loopExpression = astNodeFromWire(_statement.loop_expression, _sourceName);
		if (result.loopExpression.present || !_statement.loop_expression_detail.empty())
			result.loopExpressionDetail = statementsFromWire(_statement.loop_expression_detail, _sourceName);
		result.body = astNodeFromWire(_statement.body, _sourceName);
		result.bodyDetail = statementsFromWire(_statement.body_detail, _sourceName);
		break;
	case rustAstNodeKindExpressionStatement:
		result.expression = astNodeFromWire(_statement.expression, _sourceName);
		if (!_statement.expression_detail.empty())
			result.expressionDetail = expressionFromWire(_statement.expression_detail.front(), _sourceName);
		break;
	case rustAstNodeKindVariableDeclarationStatement:
		result.variables = convertNodeVector(_statement.variables);
		result.variableDetails = cppVectorFromRust<RustParserVariableDeclaration>(
			_statement.variable_details,
			[&](rust_ffi::WireVariableDeclarationResult const& _variable)
			{
				return variableDeclarationFromWire(_variable, _sourceName);
			}
		);
		result.initialValue = astNodeFromWire(_statement.initial_value, _sourceName);
		if (!_statement.initial_value_detail.empty())
			result.initialValueDetail = expressionFromWire(_statement.initial_value_detail.front(), _sourceName);
		break;
	default:
		break;
	}

	return result;
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

ASTPointer<ParameterList> createParameterListAstFromRustWire(
	rust_ffi::WireAstNode const& _parameterList,
	::rust::Vec<rust_ffi::WireAstNode> const& _parameterDeclarations,
	::rust::Vec<rust_ffi::WireVariableDeclarationResult> const& _parameterDetails,
	std::shared_ptr<std::string const> const& _sourceName,
	RustParserParameterContext _context = RustParserParameterContext::Default
)
{
	return createParameterListAstFromRust(
		astNodeFromWire(_parameterList, _sourceName),
		cppVectorFromRust<RustParserAstNode>(
			_parameterDeclarations,
			[&](rust_ffi::WireAstNode const& _parameter)
			{
				return astNodeFromWire(_parameter, _sourceName);
			}
		),
		cppVectorFromRust<RustParserVariableDeclaration>(
			_parameterDetails,
			[&](rust_ffi::WireVariableDeclarationResult const& _parameter)
			{
				return variableDeclarationFromWire(_parameter, _sourceName);
			}
		),
		_context
	);
}

ASTPointer<ParameterList> createParameterListAstFromRustCompact(
	rust_ffi::WireCompactNodeRef const& _parameterList,
	rust_ffi::WireCompactRefRange const& _parameterDeclarations,
	std::shared_ptr<std::string const> const& _sourceName,
	RustParserParameterContext _context = RustParserParameterContext::Default
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	if (!compactRefRangeIsValid(arena, _parameterDeclarations))
		return nullptr;

	RustParserAstNode parameterList = astNodeFromCompactRef(arena, _parameterList, _sourceName);
	if (!parameterList.present || parameterList.kind != rustAstNodeKindParameterList)
		return nullptr;

	std::vector<ASTPointer<VariableDeclaration>> parameters;
	parameters.reserve(_parameterDeclarations.len);
	size_t const end = static_cast<size_t>(_parameterDeclarations.start) + _parameterDeclarations.len;
	for (size_t index = _parameterDeclarations.start; index < end; ++index)
	{
		rust_ffi::WireCompactVariableDeclarationDetail const* detail =
			currentCompactVariableDeclarationDetailByNode(arena.ref_items[index]);
		if (!detail)
			return nullptr;
		std::optional<RustParserVariableDeclaration> variable =
			variableDeclarationFromCompact(*detail, _sourceName);
		if (!variable)
			return nullptr;
		if (!variableDeclarationIsParameterContextCompatible(*variable, _context))
			return nullptr;

		parameters.push_back(createVariableDeclarationAstFromRust(*variable));
		if (!parameters.back())
			return nullptr;
	}

	return std::make_shared<ParameterList>(
		parameterList.nodeID,
		parameterList.location,
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

bool functionCallParameterNamesAreValid(
	size_t _arguments,
	::rust::Vec<rust_ffi::WireString> const& _parameterNames,
	::rust::Vec<rust_ffi::WireSourceLocation> const& _parameterNameLocations
)
{
	return
		_parameterNames.size() == _parameterNameLocations.size() &&
			(_parameterNames.empty() || _parameterNames.size() == _arguments);
}

ASTPointer<Expression> createExpressionAstFromRustWire(
	rust_ffi::WireAstNode const& _expressionNode,
	rust_ffi::WireExpressionResult const& _expression,
	std::shared_ptr<std::string const> const& _sourceName
);

ASTPointer<Expression> createSingleChildExpressionAstFromRustWire(
	rust_ffi::WireAstNode const& _expressionNode,
	::rust::Vec<rust_ffi::WireExpressionResult> const& _expressionDetails,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_expressionNode.present || _expressionDetails.size() != 1)
		return nullptr;
	return createExpressionAstFromRustWire(_expressionNode, _expressionDetails.front(), _sourceName);
}

bool expressionIsIdentifierPathFromRustWire(
	rust_ffi::WireAstNode const& _expressionNode,
	rust_ffi::WireExpressionResult const& _expression
)
{
	if (!_expressionNode.present || !_expression.expression.present)
		return false;
	if (!wireAstNodesMatch(_expressionNode, _expression.expression))
		return false;
	if (_expression.expression.kind == rustAstNodeKindIdentifier)
		return !_expression.expression.text.bytes.empty();
	if (_expression.expression.kind != rustAstNodeKindMemberAccess || _expression.expression.text.bytes.empty())
		return false;
	if (_expression.base_expression_detail.size() != 1)
		return false;
	return expressionIsIdentifierPathFromRustWire(
		_expression.base_expression,
		_expression.base_expression_detail.front()
	);
}

ASTPointer<ElementaryTypeName> createElementaryTypeNameFromExpressionTypeAstWire(
	rust_ffi::WireAstNode const& _expressionType,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return createElementaryTypeNameFromExpressionTypeAst(astNodeFromWire(_expressionType, _sourceName));
}

std::vector<SourceLocation> sourceLocationsFromWire(
	::rust::Vec<rust_ffi::WireSourceLocation> const& _locations,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return cppVectorFromRust<SourceLocation>(
		_locations,
		[&](rust_ffi::WireSourceLocation const& _location)
		{
			return sourceLocation(_location, _sourceName);
		}
	);
}

ASTPointer<Expression> createExpressionAstFromRustCompactNode(
	rust_ffi::WireCompactNodeRef const& _node,
	std::shared_ptr<std::string const> const& _sourceName
);

ASTPointer<Expression> createExpressionAstFromRustCompact(
	rust_ffi::WireAstNode const& _expressionNode,
	rust_ffi::WireCompactExpressionDetail const& _expression,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;

	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	RustParserAstNode expression = astNodeFromCompactRef(arena, _expression.expression, _sourceName);
	if (!expression.present || !rustAstNodesMatch(expression, astNodeFromWire(_expressionNode, _sourceName)))
		return nullptr;

	auto createOptionalExpression = [&](rust_ffi::WireCompactNodeRef const& _node) -> ASTPointer<Expression>
	{
		if (!compactNodeAt(arena, _node))
			return nullptr;
		return createExpressionAstFromRustCompactNode(_node, _sourceName);
	};
	auto expressionArguments = [&](rust_ffi::WireCompactRefRange const& _range)
		-> std::optional<std::vector<ASTPointer<Expression>>>
	{
		if (!compactRefRangeIsValid(arena, _range))
			return std::nullopt;

		std::vector<ASTPointer<Expression>> expressions;
		expressions.reserve(_range.len);
		size_t const end = static_cast<size_t>(_range.start) + _range.len;
		for (size_t index = _range.start; index < end; ++index)
		{
			ASTPointer<Expression> argument = createExpressionAstFromRustCompactNode(
				arena.ref_items[index],
				_sourceName
			);
			if (!argument)
				return std::nullopt;
			expressions.push_back(std::move(argument));
		}
		return expressions;
	};

	switch (expression.kind)
	{
	case rustAstNodeKindIdentifier:
		return createIdentifierExpressionAstFromRust(expression);
	case rustAstNodeKindElementaryTypeNameExpression:
	{
		ASTPointer<ElementaryTypeName> typeName = createElementaryTypeNameFromExpressionTypeAst(
			astNodeFromCompactRef(arena, _expression.expression_type, _sourceName)
		);
		if (!typeName)
			return nullptr;
		return std::make_shared<ElementaryTypeNameExpression>(
			expression.nodeID,
			expression.location,
			std::move(typeName)
		);
	}
	case rustAstNodeKindAssignment:
	{
		std::optional<Token> assignmentOperator = scanSingleTokenFromRust(expression.text);
		if (!assignmentOperator || !TokenTraits::isAssignmentOp(*assignmentOperator))
			return nullptr;

		ASTPointer<Expression> leftExpression = createExpressionAstFromRustCompactNode(
			_expression.left_expression,
			_sourceName
		);
		ASTPointer<Expression> rightExpression = createExpressionAstFromRustCompactNode(
			_expression.right_expression,
			_sourceName
		);
		if (!leftExpression || !rightExpression)
			return nullptr;

		return std::make_shared<Assignment>(
			expression.nodeID,
			expression.location,
			std::move(leftExpression),
			*assignmentOperator,
			std::move(rightExpression)
		);
	}
	case rustAstNodeKindConditional:
	{
		ASTPointer<Expression> conditionExpression = createExpressionAstFromRustCompactNode(
			_expression.condition_expression,
			_sourceName
		);
		ASTPointer<Expression> trueExpression = createExpressionAstFromRustCompactNode(
			_expression.true_expression,
			_sourceName
		);
		ASTPointer<Expression> falseExpression = createExpressionAstFromRustCompactNode(
			_expression.false_expression,
			_sourceName
		);
		if (!conditionExpression || !trueExpression || !falseExpression)
			return nullptr;

		return std::make_shared<Conditional>(
			expression.nodeID,
			expression.location,
			std::move(conditionExpression),
			std::move(trueExpression),
			std::move(falseExpression)
		);
	}
	case rustAstNodeKindBinaryOperation:
	{
		std::optional<Token> operation = scanSingleTokenFromRust(expression.text);
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

		ASTPointer<Expression> leftExpression = createExpressionAstFromRustCompactNode(
			_expression.left_expression,
			_sourceName
		);
		ASTPointer<Expression> rightExpression = createExpressionAstFromRustCompactNode(
			_expression.right_expression,
			_sourceName
		);
		if (!leftExpression || !rightExpression)
			return nullptr;

		return std::make_shared<BinaryOperation>(
			expression.nodeID,
			expression.location,
			std::move(leftExpression),
			*operation,
			std::move(rightExpression)
		);
	}
	case rustAstNodeKindUnaryOperation:
	{
		std::optional<Token> operation = scanSingleTokenFromRust(expression.text);
		if (!operation || !TokenTraits::isUnaryOp(*operation))
			return nullptr;

		ASTPointer<Expression> subExpression = createExpressionAstFromRustCompactNode(
			_expression.sub_expression,
			_sourceName
		);
		if (!subExpression)
			return nullptr;

		return std::make_shared<UnaryOperation>(
			expression.nodeID,
			expression.location,
			*operation,
			std::move(subExpression),
			_expression.is_prefix_operation
		);
	}
	case rustAstNodeKindTupleExpression:
	case rustAstNodeKindInlineArrayExpression:
	{
		if ((expression.kind == rustAstNodeKindInlineArrayExpression) != _expression.is_inline_array)
			return nullptr;
		if (!compactRefRangeIsValid(arena, _expression.components))
			return nullptr;

		std::vector<ASTPointer<Expression>> components;
		components.reserve(_expression.components.len);
		size_t const end = static_cast<size_t>(_expression.components.start) + _expression.components.len;
		for (size_t index = _expression.components.start; index < end; ++index)
		{
			rust_ffi::WireCompactNodeRef const& componentNode = arena.ref_items[index];
			if (!compactNodeAt(arena, componentNode))
			{
				if (_expression.is_inline_array)
					return nullptr;
				components.push_back(nullptr);
				continue;
			}

			components.push_back(createExpressionAstFromRustCompactNode(componentNode, _sourceName));
			if (!components.back())
				return nullptr;
		}

		return std::make_shared<TupleExpression>(
			expression.nodeID,
			expression.location,
			std::move(components),
			_expression.is_inline_array
		);
	}
	case rustAstNodeKindNewExpression:
	{
		if (!compactNodeAt(arena, _expression.type_name))
			return nullptr;
		rust_ffi::WireCompactTypeNameDetail const* typeNameDetail =
			currentCompactTypeNameDetailByNode(_expression.type_name);
		if (!typeNameDetail)
			return nullptr;
		std::optional<RustParserTypeName> convertedTypeName = typeNameFromCompact(*typeNameDetail, _sourceName);
		if (!convertedTypeName)
			return nullptr;
		RustParserAstNode typeNameNode = astNodeFromCompactRef(arena, _expression.type_name, _sourceName);
		if (!typeNameNode.present || !rustAstNodesMatch(typeNameNode, convertedTypeName->node))
			return nullptr;

		ASTPointer<TypeName> typeName = createTypeNameAstFromRust(*convertedTypeName);
		if (!typeName)
			return nullptr;

		return std::make_shared<NewExpression>(
			expression.nodeID,
			expression.location,
			std::move(typeName)
		);
	}
	case rustAstNodeKindIndexAccess:
	{
		ASTPointer<Expression> baseExpression = createExpressionAstFromRustCompactNode(
			_expression.base_expression,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		ASTPointer<Expression> indexExpression = createOptionalExpression(_expression.index_expression);
		if (compactNodeAt(arena, _expression.index_expression) && !indexExpression)
			return nullptr;

		return std::make_shared<IndexAccess>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			std::move(indexExpression)
		);
	}
	case rustAstNodeKindIndexRangeAccess:
	{
		ASTPointer<Expression> baseExpression = createExpressionAstFromRustCompactNode(
			_expression.base_expression,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		ASTPointer<Expression> startExpression = createOptionalExpression(_expression.index_expression);
		if (compactNodeAt(arena, _expression.index_expression) && !startExpression)
			return nullptr;

		ASTPointer<Expression> endExpression = createOptionalExpression(_expression.end_index_expression);
		if (compactNodeAt(arena, _expression.end_index_expression) && !endExpression)
			return nullptr;

		return std::make_shared<IndexRangeAccess>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			std::move(startExpression),
			std::move(endExpression)
		);
	}
	case rustAstNodeKindMemberAccess:
	{
		if (expression.text.empty())
			return nullptr;

		ASTPointer<Expression> baseExpression = createExpressionAstFromRustCompactNode(
			_expression.base_expression,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		return std::make_shared<MemberAccess>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			astString(expression.text),
			sourceLocation(_expression.member_name_location, _sourceName)
		);
	}
	case rustAstNodeKindFunctionCallOptions:
	{
		if (_expression.arguments.len == 0)
			return nullptr;
		std::optional<std::vector<ASTPointer<Expression>>> options =
			expressionArguments(_expression.arguments);
		if (!options)
			return nullptr;

		std::optional<RustParserCompactNameLocations> names =
			nameLocationsFromCompactRange(arena, _expression.argument_names, _sourceName);
		if (!names || !functionCallParameterNamesAreValid(options->size(), names->names, names->locations))
			return nullptr;
		if (names->names.size() != options->size())
			return nullptr;

		ASTPointer<Expression> baseExpression = createExpressionAstFromRustCompactNode(
			_expression.base_expression,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		std::vector<ASTPointer<ASTString>> optionNames;
		optionNames.reserve(names->names.size());
		for (std::string const& name: names->names)
			optionNames.push_back(astString(name));

		return std::make_shared<FunctionCallOptions>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			std::move(*options),
			std::move(optionNames)
		);
	}
	case rustAstNodeKindFunctionCall:
	{
		std::optional<std::vector<ASTPointer<Expression>>> arguments =
			expressionArguments(_expression.arguments);
		if (!arguments)
			return nullptr;

		std::optional<RustParserCompactNameLocations> names =
			nameLocationsFromCompactRange(arena, _expression.argument_names, _sourceName);
		if (!names || !functionCallParameterNamesAreValid(arguments->size(), names->names, names->locations))
			return nullptr;

		ASTPointer<Expression> callee = createExpressionAstFromRustCompactNode(
			_expression.base_expression,
			_sourceName
		);
		if (!callee)
			return nullptr;

		std::vector<ASTPointer<ASTString>> parameterNames;
		parameterNames.reserve(names->names.size());
		for (std::string const& parameterName: names->names)
			parameterNames.push_back(astString(parameterName));

		return std::make_shared<FunctionCall>(
			expression.nodeID,
			expression.location,
			std::move(callee),
			std::move(*arguments),
			std::move(parameterNames),
			names->locations
		);
	}
	case rustAstNodeKindLiteral:
	{
		Token literalToken = static_cast<Token>(_expression.literal_token);
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
		if (_expression.literal_subdenomination != static_cast<std::uint32_t>(Token::Illegal))
		{
			Token subdenominationToken = static_cast<Token>(_expression.literal_subdenomination);
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
			expression.nodeID,
			expression.location,
			literalToken,
			astString(expression.text),
			subdenomination
		);
	}
	default:
		return nullptr;
	}
}

ASTPointer<Expression> createExpressionAstFromRustCompactNode(
	rust_ffi::WireCompactNodeRef const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;

	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return nullptr;
	rust_ffi::WireCompactExpressionDetail const* detail = currentCompactExpressionDetailByNode(_node);
	if (!detail)
	{
		debugCompactNodeFailure("expression detail", *node);
		return nullptr;
	}
	ASTPointer<Expression> expression = createExpressionAstFromRustCompact(
		wireAstNodeFromCompact(*node),
		*detail,
		_sourceName
	);
	if (!expression)
		debugCompactNodeFailure("expression", *node);
	return expression;
}

bool expressionIsIdentifierPathFromRustCompactNode(rust_ffi::WireCompactNodeRef const& _node)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return false;

	rust_ffi::WireCompactNode const* node = compactNodeAt(*context->compactArena, _node);
	if (!node)
		return false;
	if (node->kind == rustAstNodeKindIdentifier)
		return !compactText(*context->compactArena, node->text).empty();
	if (node->kind != rustAstNodeKindMemberAccess || compactText(*context->compactArena, node->text).empty())
		return false;

	rust_ffi::WireCompactExpressionDetail const* detail = currentCompactExpressionDetailByNode(_node);
	if (!detail)
		return false;
	return expressionIsIdentifierPathFromRustCompactNode(detail->base_expression);
}

ASTPointer<Expression> createExpressionAstFromRustWire(
	rust_ffi::WireAstNode const& _expressionNode,
	rust_ffi::WireExpressionResult const& _expression,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (rust_ffi::WireCompactExpressionDetail const* compactExpression =
		currentCompactExpressionDetailByWireNode(_expression.expression))
		if (ASTPointer<Expression> expression = createExpressionAstFromRustCompact(
			_expressionNode,
			*compactExpression,
			_sourceName
		))
			return expression;

	if (!_expressionNode.present || !_expression.expression.present)
		return nullptr;
	if (!wireAstNodesMatch(_expressionNode, _expression.expression))
		return nullptr;

	RustParserAstNode expression = astNodeFromWire(_expression.expression, _sourceName);
	switch (expression.kind)
	{
	case rustAstNodeKindIdentifier:
		return createIdentifierExpressionAstFromRust(expression);
	case rustAstNodeKindElementaryTypeNameExpression:
	{
		ASTPointer<ElementaryTypeName> typeName = createElementaryTypeNameFromExpressionTypeAstWire(
			_expression.expression_type,
			_sourceName
		);
		if (!typeName)
			return nullptr;
		return std::make_shared<ElementaryTypeNameExpression>(
			expression.nodeID,
			expression.location,
			std::move(typeName)
		);
	}
	case rustAstNodeKindAssignment:
	{
		std::optional<Token> assignmentOperator = scanSingleTokenFromRust(expression.text);
		if (!assignmentOperator || !TokenTraits::isAssignmentOp(*assignmentOperator))
			return nullptr;

		ASTPointer<Expression> leftExpression = createSingleChildExpressionAstFromRustWire(
			_expression.left_expression,
			_expression.left_expression_detail,
			_sourceName
		);
		if (!leftExpression)
			return nullptr;

		ASTPointer<Expression> rightExpression = createSingleChildExpressionAstFromRustWire(
			_expression.right_expression,
			_expression.right_expression_detail,
			_sourceName
		);
		if (!rightExpression)
			return nullptr;

		return std::make_shared<Assignment>(
			expression.nodeID,
			expression.location,
			std::move(leftExpression),
			*assignmentOperator,
			std::move(rightExpression)
		);
	}
	case rustAstNodeKindConditional:
	{
		ASTPointer<Expression> conditionExpression = createSingleChildExpressionAstFromRustWire(
			_expression.condition_expression,
			_expression.condition_expression_detail,
			_sourceName
		);
		if (!conditionExpression)
			return nullptr;

		ASTPointer<Expression> trueExpression = createSingleChildExpressionAstFromRustWire(
			_expression.true_expression,
			_expression.true_expression_detail,
			_sourceName
		);
		if (!trueExpression)
			return nullptr;

		ASTPointer<Expression> falseExpression = createSingleChildExpressionAstFromRustWire(
			_expression.false_expression,
			_expression.false_expression_detail,
			_sourceName
		);
		if (!falseExpression)
			return nullptr;

		return std::make_shared<Conditional>(
			expression.nodeID,
			expression.location,
			std::move(conditionExpression),
			std::move(trueExpression),
			std::move(falseExpression)
		);
	}
	case rustAstNodeKindBinaryOperation:
	{
		std::optional<Token> operation = scanSingleTokenFromRust(expression.text);
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

		ASTPointer<Expression> leftExpression = createSingleChildExpressionAstFromRustWire(
			_expression.left_expression,
			_expression.left_expression_detail,
			_sourceName
		);
		if (!leftExpression)
			return nullptr;

		ASTPointer<Expression> rightExpression = createSingleChildExpressionAstFromRustWire(
			_expression.right_expression,
			_expression.right_expression_detail,
			_sourceName
		);
		if (!rightExpression)
			return nullptr;

		return std::make_shared<BinaryOperation>(
			expression.nodeID,
			expression.location,
			std::move(leftExpression),
			*operation,
			std::move(rightExpression)
		);
	}
	case rustAstNodeKindUnaryOperation:
	{
		std::optional<Token> operation = scanSingleTokenFromRust(expression.text);
		if (!operation || !TokenTraits::isUnaryOp(*operation))
			return nullptr;

		ASTPointer<Expression> subExpression = createSingleChildExpressionAstFromRustWire(
			_expression.sub_expression,
			_expression.sub_expression_detail,
			_sourceName
		);
		if (!subExpression)
			return nullptr;

		return std::make_shared<UnaryOperation>(
			expression.nodeID,
			expression.location,
			*operation,
			std::move(subExpression),
			_expression.is_prefix_operation
		);
	}
	case rustAstNodeKindTupleExpression:
	case rustAstNodeKindInlineArrayExpression:
	{
		if ((expression.kind == rustAstNodeKindInlineArrayExpression) != _expression.is_inline_array)
			return nullptr;
		if (_expression.components.size() != _expression.component_details.size())
			return nullptr;

		std::vector<ASTPointer<Expression>> components;
		components.reserve(_expression.components.size());
		for (size_t i = 0; i < _expression.components.size(); ++i)
		{
			if (!_expression.components[i].present)
			{
				if (_expression.is_inline_array)
					return nullptr;
				if (_expression.component_details[i].expression.present)
					return nullptr;
				components.push_back(nullptr);
				continue;
			}

			components.push_back(createExpressionAstFromRustWire(
				_expression.components[i],
				_expression.component_details[i],
				_sourceName
			));
			if (!components.back())
				return nullptr;
		}

		return std::make_shared<TupleExpression>(
			expression.nodeID,
			expression.location,
			std::move(components),
			_expression.is_inline_array
		);
	}
	case rustAstNodeKindNewExpression:
	{
		if (_expression.type_name_details.size() != 1)
			return nullptr;
		RustParserAstNode typeNameNode = astNodeFromWire(_expression.type_name, _sourceName);
		RustParserTypeName typeNameDetail = typeNameFromWire(_expression.type_name_details[0], _sourceName);
		if (!typeNameNode.present || !rustAstNodesMatch(typeNameNode, typeNameDetail.node))
			return nullptr;

		ASTPointer<TypeName> typeName = createTypeNameAstFromRust(typeNameDetail);
		if (!typeName)
			return nullptr;

		return std::make_shared<NewExpression>(
			expression.nodeID,
			expression.location,
			std::move(typeName)
		);
	}
	case rustAstNodeKindIndexAccess:
	{
		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRustWire(
			_expression.base_expression,
			_expression.base_expression_detail,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		ASTPointer<Expression> indexExpression;
		if (_expression.index_expression.present)
		{
			indexExpression = createSingleChildExpressionAstFromRustWire(
				_expression.index_expression,
				_expression.index_expression_detail,
				_sourceName
			);
			if (!indexExpression)
				return nullptr;
		}
		else if (!_expression.index_expression_detail.empty())
			return nullptr;

		return std::make_shared<IndexAccess>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			std::move(indexExpression)
		);
	}
	case rustAstNodeKindIndexRangeAccess:
	{
		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRustWire(
			_expression.base_expression,
			_expression.base_expression_detail,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		ASTPointer<Expression> startExpression;
		if (_expression.index_expression.present)
		{
			startExpression = createSingleChildExpressionAstFromRustWire(
				_expression.index_expression,
				_expression.index_expression_detail,
				_sourceName
			);
			if (!startExpression)
				return nullptr;
		}
		else if (!_expression.index_expression_detail.empty())
			return nullptr;

		ASTPointer<Expression> endExpression;
		if (_expression.end_index_expression.present)
		{
			endExpression = createSingleChildExpressionAstFromRustWire(
				_expression.end_index_expression,
				_expression.end_index_expression_detail,
				_sourceName
			);
			if (!endExpression)
				return nullptr;
		}
		else if (!_expression.end_index_expression_detail.empty())
			return nullptr;

		return std::make_shared<IndexRangeAccess>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			std::move(startExpression),
			std::move(endExpression)
		);
	}
	case rustAstNodeKindMemberAccess:
	{
		if (expression.text.empty())
			return nullptr;

		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRustWire(
			_expression.base_expression,
			_expression.base_expression_detail,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		return std::make_shared<MemberAccess>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			astString(expression.text),
			sourceLocation(_expression.member_name_location, _sourceName)
		);
	}
	case rustAstNodeKindFunctionCallOptions:
	{
		if (_expression.arguments.empty())
			return nullptr;
		if (_expression.arguments.size() != _expression.argument_details.size())
			return nullptr;
		if (!functionCallParameterNamesAreValid(
			_expression.arguments.size(),
			_expression.parameter_names,
			_expression.parameter_name_locations
		))
			return nullptr;
		if (_expression.parameter_names.size() != _expression.arguments.size())
			return nullptr;

		ASTPointer<Expression> baseExpression = createSingleChildExpressionAstFromRustWire(
			_expression.base_expression,
			_expression.base_expression_detail,
			_sourceName
		);
		if (!baseExpression)
			return nullptr;

		std::vector<ASTPointer<Expression>> options;
		options.reserve(_expression.arguments.size());
		for (size_t i = 0; i < _expression.arguments.size(); ++i)
		{
			options.push_back(createExpressionAstFromRustWire(
				_expression.arguments[i],
				_expression.argument_details[i],
				_sourceName
			));
			if (!options.back())
				return nullptr;
		}

		return std::make_shared<FunctionCallOptions>(
			expression.nodeID,
			expression.location,
			std::move(baseExpression),
			std::move(options),
			astStringVectorFromWire(_expression.parameter_names)
		);
	}
	case rustAstNodeKindFunctionCall:
	{
		if (_expression.arguments.size() != _expression.argument_details.size())
			return nullptr;
		if (!functionCallParameterNamesAreValid(
			_expression.arguments.size(),
			_expression.parameter_names,
			_expression.parameter_name_locations
		))
			return nullptr;

		ASTPointer<Expression> callee = createSingleChildExpressionAstFromRustWire(
			_expression.base_expression,
			_expression.base_expression_detail,
			_sourceName
		);
		if (!callee)
			return nullptr;

		std::vector<ASTPointer<Expression>> arguments;
		arguments.reserve(_expression.arguments.size());
		for (size_t i = 0; i < _expression.arguments.size(); ++i)
		{
			arguments.push_back(createExpressionAstFromRustWire(
				_expression.arguments[i],
				_expression.argument_details[i],
				_sourceName
			));
			if (!arguments.back())
				return nullptr;
		}

		return std::make_shared<FunctionCall>(
			expression.nodeID,
			expression.location,
			std::move(callee),
			std::move(arguments),
			astStringVectorFromWire(_expression.parameter_names),
			sourceLocationsFromWire(_expression.parameter_name_locations, _sourceName)
		);
	}
	case rustAstNodeKindLiteral:
	{
		Token literalToken = static_cast<Token>(_expression.literal_token);
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
		if (_expression.literal_subdenomination != static_cast<std::uint32_t>(Token::Illegal))
		{
			Token subdenominationToken = static_cast<Token>(_expression.literal_subdenomination);
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
			expression.nodeID,
			expression.location,
			literalToken,
			astString(expression.text),
			subdenomination
		);
	}
	default:
		return nullptr;
	}
}

ASTPointer<FunctionCall> createFunctionCallAstFromRustWire(
	rust_ffi::WireAstNode const& _functionCall,
	rust_ffi::WireAstNode const& _calleeNode,
	rust_ffi::WireExpressionResult const& _callee,
	::rust::Vec<rust_ffi::WireAstNode> const& _arguments,
	::rust::Vec<rust_ffi::WireExpressionResult> const& _argumentDetails,
	::rust::Vec<rust_ffi::WireString> const& _parameterNames,
	::rust::Vec<rust_ffi::WireSourceLocation> const& _parameterNameLocations,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_functionCall.present || _functionCall.kind != rustAstNodeKindFunctionCall)
		return nullptr;
	if (_arguments.size() != _argumentDetails.size())
		return nullptr;
	if (!functionCallParameterNamesAreValid(_arguments.size(), _parameterNames, _parameterNameLocations))
		return nullptr;

	RustParserAstNode functionCall = astNodeFromWire(_functionCall, _sourceName);
	ASTPointer<Expression> callee = createExpressionAstFromRustWire(_calleeNode, _callee, _sourceName);
	if (!callee)
		return nullptr;

	std::vector<ASTPointer<Expression>> arguments;
	arguments.reserve(_arguments.size());
	for (size_t i = 0; i < _arguments.size(); ++i)
	{
		arguments.push_back(createExpressionAstFromRustWire(_arguments[i], _argumentDetails[i], _sourceName));
		if (!arguments.back())
			return nullptr;
	}

	return std::make_shared<FunctionCall>(
		functionCall.nodeID,
		functionCall.location,
		std::move(callee),
		std::move(arguments),
		astStringVectorFromWire(_parameterNames),
		sourceLocationsFromWire(_parameterNameLocations, _sourceName)
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
		if (rust_ffi::WireCompactExpressionDetail const* compactExpression =
			currentCompactExpressionDetailByAstNode(_variable.value))
		{
			RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
			rust_ffi::WireCompactNode const* compactNode =
				context && context->compactArena ? compactNodeAt(*context->compactArena, compactExpression->expression) : nullptr;
			if (compactNode && _variable.value.location.sourceName)
				value = createExpressionAstFromRustCompact(
					wireAstNodeFromCompact(*compactNode),
					*compactExpression,
					_variable.value.location.sourceName
				);
		}
		else
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
	scanner->setPosition(static_cast<size_t>(_blockLocation.start));

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

ASTPointer<ASTString> documentationStringFromRustWire(rust_ffi::WireAstNode const& _node)
{
	if (_node.text.bytes.empty())
		return nullptr;
	return astStringFromWire(_node.text);
}

ASTPointer<Statement> createStatementAstFromRustCompactNode(
	rust_ffi::WireCompactNodeRef const& _node,
	std::shared_ptr<std::string const> const& _sourceName
);

ASTPointer<Block> createBlockAstFromRustCompact(
	rust_ffi::WireCompactStatementDetail const& _statement,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	RustParserAstNode block = astNodeFromCompactRef(arena, _statement.statement, _sourceName);
	if (!block.present)
		return nullptr;
	if (block.kind == rustAstNodeKindBlock && _statement.block_unchecked)
		return nullptr;
	if (block.kind == rustAstNodeKindUncheckedBlock && !_statement.block_unchecked)
		return nullptr;
	if (block.kind != rustAstNodeKindBlock && block.kind != rustAstNodeKindUncheckedBlock)
		return nullptr;
	if (!compactRefRangeIsValid(arena, _statement.block_statements))
		return nullptr;

	std::vector<ASTPointer<Statement>> statements;
	statements.reserve(_statement.block_statements.len);
	size_t const end = static_cast<size_t>(_statement.block_statements.start) + _statement.block_statements.len;
	for (size_t index = _statement.block_statements.start; index < end; ++index)
	{
		rust_ffi::WireCompactNodeRef const& statementRef = arena.ref_items[index];
		statements.push_back(createStatementAstFromRustCompactNode(statementRef, _sourceName));
		if (!statements.back())
		{
			if (rust_ffi::WireCompactNode const* statementNode = compactNodeAt(arena, statementRef))
				debugCompactNodeFailure("block statement", *statementNode);
			return nullptr;
		}
	}

	return std::make_shared<Block>(
		block.nodeID,
		block.location,
		documentationStringFromRust(block),
		_statement.block_unchecked,
		std::move(statements)
	);
}

ASTPointer<TryCatchClause> createTryCatchClauseAstFromRustCompact(
	rust_ffi::WireCompactTryCatchClauseDetail const& _clause,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	RustParserAstNode clause = astNodeFromCompactRef(arena, _clause.try_catch_clause, _sourceName);
	if (!clause.present || clause.kind != rustAstNodeKindTryCatchClause)
		return nullptr;

	ASTPointer<ParameterList> parameters;
	if (compactNodeAt(arena, _clause.error_parameters))
	{
		parameters = createParameterListAstFromRustCompact(
			_clause.error_parameters,
			_clause.error_parameter_declarations,
			_sourceName,
			RustParserParameterContext::LocationAllowed
		);
		if (!parameters)
			return nullptr;
	}
	else if (_clause.error_parameter_declarations.len != 0)
		return nullptr;

	std::string errorName = compactText(arena, _clause.error_name);
	if (!errorName.empty() && !compactNodeAt(arena, _clause.error_parameters))
		return nullptr;

	rust_ffi::WireCompactStatementDetail const* blockDetail =
		currentCompactStatementDetailByNode(_clause.block);
	if (!blockDetail || blockDetail->block_unchecked != _clause.block_unchecked)
		return nullptr;
	ASTPointer<Block> block = createBlockAstFromRustCompact(*blockDetail, _sourceName);
	if (!block)
		return nullptr;

	return std::make_shared<TryCatchClause>(
		clause.nodeID,
		clause.location,
		astString(errorName),
		std::move(parameters),
		std::move(block)
	);
}

ASTPointer<FunctionCall> createStatementFunctionCallAstFromRustCompact(
	rust_ffi::WireCompactNodeRef const& _call,
	rust_ffi::WireCompactNodeRef const& _callee,
	rust_ffi::WireCompactRefRange const& _arguments,
	rust_ffi::WireCompactRefRange const& _parameterNames,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode call = astNodeFromCompactRef(arena, _call, _sourceName);
	if (!call.present || call.kind != rustAstNodeKindFunctionCall)
		return nullptr;
	if (!expressionIsIdentifierPathFromRustCompactNode(_callee))
		return nullptr;

	ASTPointer<Expression> callee = createExpressionAstFromRustCompactNode(_callee, _sourceName);
	if (!callee)
		return nullptr;
	if (!compactRefRangeIsValid(arena, _arguments))
		return nullptr;

	std::vector<ASTPointer<Expression>> arguments;
	arguments.reserve(_arguments.len);
	size_t const end = static_cast<size_t>(_arguments.start) + _arguments.len;
	for (size_t index = _arguments.start; index < end; ++index)
	{
		arguments.push_back(createExpressionAstFromRustCompactNode(arena.ref_items[index], _sourceName));
		if (!arguments.back())
			return nullptr;
	}

	std::optional<RustParserCompactNameLocations> names =
		nameLocationsFromCompactRange(arena, _parameterNames, _sourceName);
	if (!names || !functionCallParameterNamesAreValid(arguments.size(), names->names, names->locations))
		return nullptr;

	std::vector<ASTPointer<ASTString>> parameterNames;
	parameterNames.reserve(names->names.size());
	for (std::string const& name: names->names)
		parameterNames.push_back(astString(name));

	return std::make_shared<FunctionCall>(
		call.nodeID,
		call.location,
		std::move(callee),
		std::move(arguments),
		std::move(parameterNames),
		names->locations
	);
}

ASTPointer<Statement> createStatementAstFromRustCompact(
	rust_ffi::WireCompactStatementDetail const& _statement,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;
	RustParserAstNode statement = astNodeFromCompactRef(arena, _statement.statement, _sourceName);
	if (!statement.present)
		return nullptr;

	ASTPointer<ASTString> documentation = documentationStringFromRust(statement);
	auto optionalExpression = [&](rust_ffi::WireCompactNodeRef const& _node) -> ASTPointer<Expression>
	{
		if (!compactNodeAt(arena, _node))
			return nullptr;
		return createExpressionAstFromRustCompactNode(_node, _sourceName);
	};
	auto requiredExpression = [&](rust_ffi::WireCompactNodeRef const& _node) -> ASTPointer<Expression>
	{
		if (!compactNodeAt(arena, _node))
			return nullptr;
		return createExpressionAstFromRustCompactNode(_node, _sourceName);
	};
	auto optionalStatement = [&](rust_ffi::WireCompactNodeRef const& _node) -> ASTPointer<Statement>
	{
		if (!compactNodeAt(arena, _node))
			return nullptr;
		return createStatementAstFromRustCompactNode(_node, _sourceName);
	};

	switch (statement.kind)
	{
	case rustAstNodeKindBlock:
	case rustAstNodeKindUncheckedBlock:
		return createBlockAstFromRustCompact(_statement, _sourceName);
	case rustAstNodeKindPlaceholderStatement:
		return std::make_shared<PlaceholderStatement>(statement.nodeID, statement.location, std::move(documentation));
	case rustAstNodeKindContinueStatement:
		return std::make_shared<Continue>(statement.nodeID, statement.location, std::move(documentation));
	case rustAstNodeKindBreakStatement:
		return std::make_shared<Break>(statement.nodeID, statement.location, std::move(documentation));
	case rustAstNodeKindThrowStatement:
		return std::make_shared<Throw>(statement.nodeID, statement.location, std::move(documentation));
	case rustAstNodeKindEmitStatement:
	{
		ASTPointer<FunctionCall> eventCall = createStatementFunctionCallAstFromRustCompact(
			_statement.event_call,
			_statement.event_call_callee,
			_statement.event_call_arguments,
			_statement.event_call_parameter_names,
			_sourceName
		);
		if (!eventCall)
			return nullptr;
		return std::make_shared<EmitStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(eventCall)
		);
	}
	case rustAstNodeKindRevertStatement:
	{
		ASTPointer<FunctionCall> errorCall = createStatementFunctionCallAstFromRustCompact(
			_statement.error_call,
			_statement.error_call_callee,
			_statement.error_call_arguments,
			_statement.error_call_parameter_names,
			_sourceName
		);
		if (!errorCall)
			return nullptr;
		return std::make_shared<RevertStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(errorCall)
		);
	}
	case rustAstNodeKindReturnStatement:
	{
		ASTPointer<Expression> expression = optionalExpression(_statement.expression);
		if (compactNodeAt(arena, _statement.expression) && !expression)
			return nullptr;
		return std::make_shared<Return>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(expression)
		);
	}
	case rustAstNodeKindInlineAssembly:
	{
		std::optional<RustParserCompactNameLocations> flags =
			nameLocationsFromCompactRange(arena, _statement.inline_assembly_flags, _sourceName);
		if (!flags)
			return nullptr;
		RustParserStatement compactStatement;
		compactStatement.node = statement;
		compactStatement.inlineAssemblyFlags = std::move(flags->names);
		compactStatement.inlineAssemblyBlockLocation = sourceLocation(
			_statement.inline_assembly_block_location,
			_sourceName
		);
		return createInlineAssemblyAstFromRust(compactStatement, std::move(documentation));
	}
	case rustAstNodeKindIfStatement:
	{
		ASTPointer<Expression> condition = requiredExpression(_statement.condition_expression);
		ASTPointer<Statement> trueBody = optionalStatement(_statement.true_body);
		if (!condition || !trueBody)
			return nullptr;
		ASTPointer<Statement> falseBody = optionalStatement(_statement.false_body);
		if (compactNodeAt(arena, _statement.false_body) && !falseBody)
			return nullptr;
		return std::make_shared<IfStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(condition),
			std::move(trueBody),
			std::move(falseBody)
		);
	}
	case rustAstNodeKindTryStatement:
	{
		ASTPointer<Expression> externalCall = requiredExpression(_statement.external_call);
		if (!externalCall)
			return nullptr;
		if (!compactRefRangeIsValid(arena, _statement.clauses) || _statement.clauses.len == 0)
			return nullptr;

		auto clauseDetail = [&](rust_ffi::WireCompactNodeRef const& _clause)
			-> rust_ffi::WireCompactTryCatchClauseDetail const*
		{
			return currentCompactTryCatchClauseDetailByNode(_clause);
		};

		rust_ffi::WireCompactTryCatchClauseDetail const* firstClause =
			clauseDetail(arena.ref_items[_statement.clauses.start]);
		if (!firstClause || !compactText(arena, firstClause->error_name).empty())
			return nullptr;

		std::vector<ASTPointer<TryCatchClause>> clauses;
		clauses.reserve(_statement.clauses.len);
		size_t const end = static_cast<size_t>(_statement.clauses.start) + _statement.clauses.len;
		for (size_t index = _statement.clauses.start; index < end; ++index)
		{
			rust_ffi::WireCompactTryCatchClauseDetail const* detail = clauseDetail(arena.ref_items[index]);
			if (!detail)
				return nullptr;
			clauses.push_back(createTryCatchClauseAstFromRustCompact(*detail, _sourceName));
			if (!clauses.back())
				return nullptr;
		}

		return std::make_shared<TryStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(externalCall),
			std::move(clauses)
		);
	}
	case rustAstNodeKindWhileStatement:
	case rustAstNodeKindDoWhileStatement:
	{
		if ((statement.kind == rustAstNodeKindDoWhileStatement) != _statement.is_do_while)
			return nullptr;
		ASTPointer<Expression> condition = requiredExpression(_statement.condition_expression);
		ASTPointer<Statement> body = optionalStatement(_statement.body);
		if (!condition || !body)
			return nullptr;
		return std::make_shared<WhileStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(condition),
			std::move(body),
			_statement.is_do_while
		);
	}
	case rustAstNodeKindForStatement:
	{
		ASTPointer<Statement> initExpression = optionalStatement(_statement.init_expression);
		if (compactNodeAt(arena, _statement.init_expression) && !initExpression)
			return nullptr;

		ASTPointer<Expression> conditionExpression = optionalExpression(_statement.condition_expression);
		if (compactNodeAt(arena, _statement.condition_expression) && !conditionExpression)
			return nullptr;

		ASTPointer<ExpressionStatement> loopExpression;
		if (compactNodeAt(arena, _statement.loop_expression))
		{
			rust_ffi::WireCompactNode const* loopNode = compactNodeAt(arena, _statement.loop_expression);
			if (!loopNode || loopNode->kind != rustAstNodeKindExpressionStatement)
				return nullptr;
			ASTPointer<Statement> loopStatement = createStatementAstFromRustCompactNode(
				_statement.loop_expression,
				_sourceName
			);
			loopExpression = std::dynamic_pointer_cast<ExpressionStatement>(loopStatement);
			if (!loopExpression)
				return nullptr;
		}

		ASTPointer<Statement> body = optionalStatement(_statement.body);
		if (!body)
			return nullptr;
		return std::make_shared<ForStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(initExpression),
			std::move(conditionExpression),
			std::move(loopExpression),
			std::move(body)
		);
	}
	case rustAstNodeKindExpressionStatement:
	{
		ASTPointer<Expression> expression = requiredExpression(_statement.expression);
		if (!expression)
			return nullptr;
		return std::make_shared<ExpressionStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(expression)
		);
	}
	case rustAstNodeKindVariableDeclarationStatement:
	{
		if (!compactRefRangeIsValid(arena, _statement.variables))
			return nullptr;
		size_t presentVariables = 0;
		std::vector<ASTPointer<VariableDeclaration>> variables;
		variables.reserve(_statement.variables.len);
		size_t const end = static_cast<size_t>(_statement.variables.start) + _statement.variables.len;
		for (size_t index = _statement.variables.start; index < end; ++index)
		{
			rust_ffi::WireCompactNodeRef const& variableRef = arena.ref_items[index];
			if (!compactNodeAt(arena, variableRef))
			{
				variables.push_back(nullptr);
				continue;
			}
			++presentVariables;
			rust_ffi::WireCompactVariableDeclarationDetail const* variableDetail =
				currentCompactVariableDeclarationDetailByNode(variableRef);
			if (!variableDetail)
				return nullptr;
			std::optional<RustParserVariableDeclaration> variable =
				variableDeclarationFromCompact(*variableDetail, _sourceName);
			if (!variable || !variableDeclarationIsLocalStatementCompatible(*variable))
				return nullptr;
			variables.push_back(createVariableDeclarationAstFromRust(*variable));
			if (!variables.back())
				return nullptr;
		}
		if (_statement.variables.len == 0 || presentVariables == 0)
			return nullptr;
		if (!compactNodeAt(arena, _statement.initial_value) && _statement.variables.len != 1)
			return nullptr;

		ASTPointer<Expression> initialValue = optionalExpression(_statement.initial_value);
		if (compactNodeAt(arena, _statement.initial_value) && !initialValue)
			return nullptr;
		return std::make_shared<VariableDeclarationStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(variables),
			std::move(initialValue)
		);
	}
	default:
		return nullptr;
	}
}

ASTPointer<Statement> createStatementAstFromRustCompactNode(
	rust_ffi::WireCompactNodeRef const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	rust_ffi::WireCompactStatementDetail const* detail = currentCompactStatementDetailByNode(_node);
	rust_ffi::WireCompactNode const* node = context && context->compactArena ? compactNodeAt(*context->compactArena, _node) : nullptr;
	if (!detail)
	{
		if (node)
			debugCompactNodeFailure("statement detail", *node);
		return nullptr;
	}
	ASTPointer<Statement> statement = createStatementAstFromRustCompact(*detail, _sourceName);
	if (!statement && node)
		debugCompactNodeFailure("statement", *node);
	return statement;
}

ASTPointer<Statement> createStatementAstFromRustWire(
	rust_ffi::WireStatementResult const& _statement,
	std::shared_ptr<std::string const> const& _sourceName
);

rust_ffi::WireStatementResult const* findStatementWireDetail(
	::rust::Vec<rust_ffi::WireStatementResult> const& _statementDetails,
	size_t _index,
	rust_ffi::WireAstNode const& _statement
)
{
	if (_index < _statementDetails.size() && wireAstNodesMatch(_statementDetails[_index].statement, _statement))
		return &_statementDetails[_index];

	auto statementDetail = findMatchingWireDetailByNode(
		_statementDetails,
		_statement,
		[](auto const& _detail) -> rust_ffi::WireAstNode const& { return _detail.statement; }
	);
	if (statementDetail == _statementDetails.end())
		return nullptr;
	return &*statementDetail;
}

ASTPointer<Block> createBlockAstFromRustWire(
	rust_ffi::WireAstNode const& _block,
	bool _unchecked,
	::rust::Vec<rust_ffi::WireAstNode> const& _statements,
	::rust::Vec<rust_ffi::WireStatementResult> const& _statementDetails,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_block.present)
		return nullptr;

	if (rust_ffi::WireCompactStatementDetail const* compactDetail =
		currentCompactStatementDetailByWireNode(_block))
		if (compactDetail->block_unchecked == _unchecked)
			if (ASTPointer<Block> compactBlock = createBlockAstFromRustCompact(*compactDetail, _sourceName))
				return compactBlock;

	RustParserAstNode block = astNodeFromWire(_block, _sourceName);
	if (block.kind == rustAstNodeKindBlock && _unchecked)
		return nullptr;
	if (block.kind == rustAstNodeKindUncheckedBlock && !_unchecked)
		return nullptr;
	if (block.kind != rustAstNodeKindBlock && block.kind != rustAstNodeKindUncheckedBlock)
		return nullptr;
	if (_statements.size() != _statementDetails.size())
		return nullptr;

	std::vector<ASTPointer<Statement>> statements;
	statements.reserve(_statements.size());
	for (size_t i = 0; i < _statements.size(); ++i)
	{
		rust_ffi::WireStatementResult const* statementDetail = findStatementWireDetail(
			_statementDetails,
			i,
			_statements[i]
		);
		if (!statementDetail)
			return nullptr;

		statements.push_back(createStatementAstFromRustWire(*statementDetail, _sourceName));
		if (!statements.back())
			return nullptr;
	}

	return std::make_shared<Block>(
		block.nodeID,
		block.location,
		documentationStringFromRustWire(_block),
		_unchecked,
		std::move(statements)
	);
}

ASTPointer<Statement> createSingleStatementAstFromRustWire(
	rust_ffi::WireAstNode const& _node,
	::rust::Vec<rust_ffi::WireStatementResult> const& _statementDetails,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_node.present)
		return nullptr;

	if (rust_ffi::WireCompactStatementDetail const* compactDetail =
		currentCompactStatementDetailByWireNode(_node))
		if (ASTPointer<Statement> compactStatement = createStatementAstFromRustCompact(*compactDetail, _sourceName))
			return compactStatement;

	if (_statementDetails.size() != 1)
		return nullptr;
	rust_ffi::WireStatementResult const& statement = _statementDetails.front();
	if (!wireAstNodesMatch(statement.statement, _node))
		return nullptr;
	return createStatementAstFromRustWire(statement, _sourceName);
}

std::vector<RustParserAstNode> astNodesFromWire(
	::rust::Vec<rust_ffi::WireAstNode> const& _nodes,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return cppVectorFromRust<RustParserAstNode>(
		_nodes,
		[&](rust_ffi::WireAstNode const& _node)
		{
			return astNodeFromWire(_node, _sourceName);
		}
	);
}

ASTPointer<InlineAssembly> createInlineAssemblyAstFromRustWire(
	rust_ffi::WireStatementResult const& _statement,
	ASTPointer<ASTString> _documentation,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_statement.statement.present || _statement.statement.kind != rustAstNodeKindInlineAssembly)
		return nullptr;

	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context)
		return nullptr;

	RustParserAstNode statement = astNodeFromWire(_statement.statement, _sourceName);
	yul::Dialect const& dialect = yul::EVMDialect::strictAssemblyForEVM(context->evmVersion);
	std::shared_ptr<yul::AST> operations = parseInlineAssemblyOperationsFromRust(
		sourceLocation(_statement.inline_assembly_block_location, _sourceName)
	);
	if (!operations)
		return nullptr;

	ASTPointer<std::vector<ASTPointer<ASTString>>> flags;
	if (!_statement.inline_assembly_flags.empty())
	{
		flags = std::make_shared<std::vector<ASTPointer<ASTString>>>();
		flags->reserve(_statement.inline_assembly_flags.size());
		for (rust_ffi::WireString const& flag: _statement.inline_assembly_flags)
			flags->push_back(astStringFromWire(flag));
	}

	return std::make_shared<InlineAssembly>(
		statement.nodeID,
		statement.location,
		std::move(_documentation),
		dialect,
		std::move(flags),
		std::move(operations)
	);
}

ASTPointer<TryCatchClause> createTryCatchClauseAstFromRustWire(
	rust_ffi::WireTryCatchClauseResult const& _clause,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_clause.try_catch_clause.present || _clause.try_catch_clause.kind != rustAstNodeKindTryCatchClause)
		return nullptr;

	RustParserAstNode clause = astNodeFromWire(_clause.try_catch_clause, _sourceName);
	ASTPointer<ParameterList> parameters;
	if (_clause.error_parameters.present)
	{
		parameters = createParameterListAstFromRustWire(
			_clause.error_parameters,
			_clause.error_parameter_declarations,
			_clause.error_parameter_details,
			_sourceName,
			RustParserParameterContext::LocationAllowed
		);
		if (!parameters)
			return nullptr;
	}
	else if (!_clause.error_parameter_declarations.empty() || !_clause.error_parameter_details.empty())
		return nullptr;
	if (!_clause.error_name.bytes.empty() && !_clause.error_parameters.present)
		return nullptr;

	ASTPointer<Block> block = createBlockAstFromRustWire(
		_clause.block,
		_clause.block_unchecked,
		_clause.block_statements,
		_clause.block_statement_details,
		_sourceName
	);
	if (!block)
		return nullptr;

	return std::make_shared<TryCatchClause>(
		clause.nodeID,
		clause.location,
		astStringFromWire(_clause.error_name),
		std::move(parameters),
		std::move(block)
	);
}

ASTPointer<Statement> createStatementAstFromRustWire(
	rust_ffi::WireStatementResult const& _statement,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_statement.statement.present)
		return nullptr;

	if (rust_ffi::WireCompactStatementDetail const* compactDetail =
		currentCompactStatementDetailByWireNode(_statement.statement))
		if (ASTPointer<Statement> compactStatement = createStatementAstFromRustCompact(*compactDetail, _sourceName))
			return compactStatement;

	RustParserAstNode statement = astNodeFromWire(_statement.statement, _sourceName);
	ASTPointer<ASTString> documentation = documentationStringFromRustWire(_statement.statement);
	switch (statement.kind)
	{
	case rustAstNodeKindBlock:
	case rustAstNodeKindUncheckedBlock:
		return createBlockAstFromRustWire(
			_statement.statement,
			_statement.block_unchecked,
			_statement.block_statements,
			_statement.block_statement_details,
			_sourceName
		);
	case rustAstNodeKindPlaceholderStatement:
		return std::make_shared<PlaceholderStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation)
		);
	case rustAstNodeKindContinueStatement:
		return std::make_shared<Continue>(
			statement.nodeID,
			statement.location,
			std::move(documentation)
		);
	case rustAstNodeKindBreakStatement:
		return std::make_shared<Break>(
			statement.nodeID,
			statement.location,
			std::move(documentation)
		);
	case rustAstNodeKindReturnStatement:
	{
		ASTPointer<Expression> expression;
		if (_statement.expression.present)
		{
			expression = createSingleChildExpressionAstFromRustWire(
				_statement.expression,
				_statement.expression_detail,
				_sourceName
			);
			if (!expression)
				return nullptr;
		}
		else if (!_statement.expression_detail.empty())
			return nullptr;
		return std::make_shared<Return>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(expression)
		);
	}
	case rustAstNodeKindThrowStatement:
		return std::make_shared<Throw>(
			statement.nodeID,
			statement.location,
			std::move(documentation)
		);
	case rustAstNodeKindEmitStatement:
	{
		if (_statement.event_call_callee_detail.size() != 1)
			return nullptr;
		rust_ffi::WireExpressionResult const& eventCallCalleeDetail = _statement.event_call_callee_detail.front();
		if (!expressionIsIdentifierPathFromRustWire(_statement.event_call_callee, eventCallCalleeDetail))
			return nullptr;

		ASTPointer<FunctionCall> eventCall = createFunctionCallAstFromRustWire(
			_statement.event_call,
			_statement.event_call_callee,
			eventCallCalleeDetail,
			_statement.event_call_arguments,
			_statement.event_call_argument_details,
			_statement.event_call_parameter_names,
			_statement.event_call_parameter_name_locations,
			_sourceName
		);
		if (!eventCall)
			return nullptr;

		return std::make_shared<EmitStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(eventCall)
		);
	}
	case rustAstNodeKindRevertStatement:
	{
		if (_statement.error_call_callee_detail.size() != 1)
			return nullptr;
		rust_ffi::WireExpressionResult const& errorCallCalleeDetail = _statement.error_call_callee_detail.front();
		if (!expressionIsIdentifierPathFromRustWire(_statement.error_call_callee, errorCallCalleeDetail))
			return nullptr;

		ASTPointer<FunctionCall> errorCall = createFunctionCallAstFromRustWire(
			_statement.error_call,
			_statement.error_call_callee,
			errorCallCalleeDetail,
			_statement.error_call_arguments,
			_statement.error_call_argument_details,
			_statement.error_call_parameter_names,
			_statement.error_call_parameter_name_locations,
			_sourceName
		);
		if (!errorCall)
			return nullptr;

		return std::make_shared<RevertStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(errorCall)
		);
	}
	case rustAstNodeKindInlineAssembly:
		return createInlineAssemblyAstFromRustWire(
			_statement,
			std::move(documentation),
			_sourceName
		);
	case rustAstNodeKindIfStatement:
	{
		ASTPointer<Expression> condition = createSingleChildExpressionAstFromRustWire(
			_statement.condition_expression,
			_statement.condition_expression_detail,
			_sourceName
		);
		if (!condition)
			return nullptr;

		ASTPointer<Statement> trueBody = createSingleStatementAstFromRustWire(
			_statement.true_body,
			_statement.true_body_detail,
			_sourceName
		);
		if (!trueBody)
			return nullptr;

		ASTPointer<Statement> falseBody;
		if (_statement.false_body.present)
		{
			falseBody = createSingleStatementAstFromRustWire(
				_statement.false_body,
				_statement.false_body_detail,
				_sourceName
			);
			if (!falseBody)
				return nullptr;
		}
		else if (!_statement.false_body_detail.empty())
			return nullptr;

		return std::make_shared<IfStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(condition),
			std::move(trueBody),
			std::move(falseBody)
		);
	}
	case rustAstNodeKindTryStatement:
	{
		ASTPointer<Expression> externalCall = createSingleChildExpressionAstFromRustWire(
			_statement.external_call,
			_statement.external_call_detail,
			_sourceName
		);
		if (!externalCall)
			return nullptr;
		if (_statement.clauses.size() != _statement.clause_details.size())
			return nullptr;
		if (_statement.clauses.size() < 2)
			return nullptr;

		auto findClauseDetail = [&](rust_ffi::WireAstNode const& _clause)
		{
			return findMatchingWireDetailByNode(
				_statement.clause_details,
				_clause,
				[](auto const& _clauseDetail) -> rust_ffi::WireAstNode const&
				{
					return _clauseDetail.try_catch_clause;
				}
			);
		};

		auto firstClauseDetail = findClauseDetail(_statement.clauses.front());
		if (firstClauseDetail == _statement.clause_details.end() || !firstClauseDetail->error_name.bytes.empty())
			return nullptr;

		std::vector<ASTPointer<TryCatchClause>> clauses;
		clauses.reserve(_statement.clauses.size());
		for (rust_ffi::WireAstNode const& clause: _statement.clauses)
		{
			auto clauseDetail = findClauseDetail(clause);
			if (clauseDetail == _statement.clause_details.end())
				return nullptr;

			clauses.push_back(createTryCatchClauseAstFromRustWire(*clauseDetail, _sourceName));
			if (!clauses.back())
				return nullptr;
		}

		return std::make_shared<TryStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(externalCall),
			std::move(clauses)
		);
	}
	case rustAstNodeKindWhileStatement:
	case rustAstNodeKindDoWhileStatement:
	{
		if ((statement.kind == rustAstNodeKindDoWhileStatement) != _statement.is_do_while)
			return nullptr;

		ASTPointer<Expression> condition = createSingleChildExpressionAstFromRustWire(
			_statement.condition_expression,
			_statement.condition_expression_detail,
			_sourceName
		);
		if (!condition)
			return nullptr;

		ASTPointer<Statement> body = createSingleStatementAstFromRustWire(
			_statement.body,
			_statement.body_detail,
			_sourceName
		);
		if (!body)
			return nullptr;

		return std::make_shared<WhileStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(condition),
			std::move(body),
			_statement.is_do_while
		);
	}
	case rustAstNodeKindForStatement:
	{
		ASTPointer<Statement> initExpression;
		if (_statement.init_expression.present)
		{
			initExpression = createSingleStatementAstFromRustWire(
				_statement.init_expression,
				_statement.init_expression_detail,
				_sourceName
			);
			if (!initExpression)
				return nullptr;
		}
		else if (!_statement.init_expression_detail.empty())
			return nullptr;

		ASTPointer<Expression> conditionExpression;
		if (_statement.condition_expression.present)
		{
			conditionExpression = createSingleChildExpressionAstFromRustWire(
				_statement.condition_expression,
				_statement.condition_expression_detail,
				_sourceName
			);
			if (!conditionExpression)
				return nullptr;
		}
		else if (!_statement.condition_expression_detail.empty())
			return nullptr;

		ASTPointer<ExpressionStatement> loopExpression;
		if (_statement.loop_expression.present)
		{
			if (_statement.loop_expression.kind != rustAstNodeKindExpressionStatement)
				return nullptr;

			ASTPointer<Statement> loopStatement = createSingleStatementAstFromRustWire(
				_statement.loop_expression,
				_statement.loop_expression_detail,
				_sourceName
			);
			loopExpression = std::dynamic_pointer_cast<ExpressionStatement>(loopStatement);
			if (!loopExpression)
				return nullptr;
		}
		else if (!_statement.loop_expression_detail.empty())
			return nullptr;

		ASTPointer<Statement> body = createSingleStatementAstFromRustWire(
			_statement.body,
			_statement.body_detail,
			_sourceName
		);
		if (!body)
			return nullptr;

		return std::make_shared<ForStatement>(
			statement.nodeID,
			statement.location,
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
		ASTPointer<Expression> expression = createSingleChildExpressionAstFromRustWire(
			_statement.expression,
			_statement.expression_detail,
			_sourceName
		);
		if (!expression)
			return nullptr;
		return std::make_shared<ExpressionStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(expression)
		);
	}
	case rustAstNodeKindVariableDeclarationStatement:
	{
		size_t presentVariables = 0;
		for (rust_ffi::WireAstNode const& variable: _statement.variables)
			if (variable.present)
				++presentVariables;
		if (_statement.variables.empty() || presentVariables == 0)
			return nullptr;
		if (!_statement.initial_value.present && _statement.variables.size() != 1)
			return nullptr;
		if (presentVariables != _statement.variable_details.size())
			return nullptr;

		std::vector<ASTPointer<VariableDeclaration>> variables;
		variables.reserve(_statement.variables.size());
		for (rust_ffi::WireAstNode const& variable: _statement.variables)
		{
			if (!variable.present)
			{
				variables.push_back(nullptr);
				continue;
			}

			auto variableDetail = findMatchingWireDetailByNode(
				_statement.variable_details,
				variable,
				[](auto const& _variable) -> rust_ffi::WireAstNode const& { return _variable.variable_declaration; }
			);
			if (variableDetail == _statement.variable_details.end())
				return nullptr;

			std::optional<RustParserVariableDeclaration> compactVariable;
			if (rust_ffi::WireCompactVariableDeclarationDetail const* compactDetail =
				currentCompactVariableDeclarationDetailByWireNode(variable))
				compactVariable = variableDeclarationFromCompact(*compactDetail, _sourceName);
			RustParserVariableDeclaration convertedVariable =
				compactVariable ? std::move(*compactVariable) : variableDeclarationFromWire(*variableDetail, _sourceName);
			if (!variableDeclarationIsLocalStatementCompatible(convertedVariable))
				return nullptr;

			variables.push_back(createVariableDeclarationAstFromRust(convertedVariable));
			if (!variables.back())
				return nullptr;
		}

		ASTPointer<Expression> initialValue;
		if (_statement.initial_value.present)
		{
			initialValue = createSingleChildExpressionAstFromRustWire(
				_statement.initial_value,
				_statement.initial_value_detail,
				_sourceName
			);
			if (!initialValue)
				return nullptr;
		}
		else if (!_statement.initial_value_detail.empty())
			return nullptr;

		return std::make_shared<VariableDeclarationStatement>(
			statement.nodeID,
			statement.location,
			std::move(documentation),
			std::move(variables),
			std::move(initialValue)
		);
	}
	default:
		return nullptr;
	}
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

ASTPointer<OverrideSpecifier> createOverrideSpecifierAstFromRustWire(
	rust_ffi::WireAstNode const& _overrides,
	::rust::Vec<rust_ffi::WireAstNode> const& _overridePaths,
	::rust::Vec<rust_ffi::WireIdentifierPathResult> const& _overridePathDetails,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	return createOverrideSpecifierAstFromRust(
		astNodeFromWire(_overrides, _sourceName),
		astNodesFromWire(_overridePaths, _sourceName),
		cppVectorFromRust<RustParserIdentifierPath>(
			_overridePathDetails,
			[&](rust_ffi::WireIdentifierPathResult const& _overridePath)
			{
				return identifierPathFromWire(_overridePath, _sourceName);
			}
		)
	);
}

ASTPointer<ModifierInvocation> createModifierInvocationAstFromRustWire(
	rust_ffi::WireModifierInvocationResult const& _modifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_modifier.modifier_invocation.present || _modifier.modifier_invocation.kind != rustAstNodeKindModifierInvocation)
		return nullptr;
	if (!_modifier.modifier_name.present)
		return nullptr;

	RustParserAstNode modifierInvocation = astNodeFromWire(_modifier.modifier_invocation, _sourceName);
	RustParserIdentifierPath modifierNameDetail = identifierPathFromWire(_modifier.modifier_name_detail, _sourceName);
	if (!rustAstNodesMatch(astNodeFromWire(_modifier.modifier_name, _sourceName), modifierNameDetail.node))
		return nullptr;

	ASTPointer<IdentifierPath> modifierName = createIdentifierPathAstFromRust(
		modifierNameDetail.node,
		modifierNameDetail.path,
		modifierNameDetail.pathLocations
	);
	if (!modifierName)
		return nullptr;

	std::unique_ptr<std::vector<ASTPointer<Expression>>> arguments;
	if (_modifier.has_arguments)
	{
		if (_modifier.arguments.size() != _modifier.argument_details.size())
			return nullptr;
		arguments = std::make_unique<std::vector<ASTPointer<Expression>>>();
		arguments->reserve(_modifier.arguments.size());
		for (size_t i = 0; i < _modifier.arguments.size(); ++i)
		{
			arguments->push_back(createExpressionAstFromRustWire(
				_modifier.arguments[i],
				_modifier.argument_details[i],
				_sourceName
			));
			if (!arguments->back())
				return nullptr;
		}
	}
	else if (!_modifier.arguments.empty() || !_modifier.argument_details.empty())
		return nullptr;

	return std::make_shared<ModifierInvocation>(
		modifierInvocation.nodeID,
		modifierInvocation.location,
		std::move(modifierName),
		std::move(arguments)
	);
}

ASTPointer<OverrideSpecifier> createOverrideSpecifierAstFromRustCompact(
	rust_ffi::WireCompactNodeRef const& _overrides,
	rust_ffi::WireCompactRefRange const& _overridePaths,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;

	std::optional<std::vector<RustParserAstNode>> overridePaths =
		astNodesFromCompactRefRange(*context->compactArena, _overridePaths, _sourceName);
	std::optional<std::vector<RustParserIdentifierPath>> overridePathDetails =
		identifierPathsFromCompactRefRange(*context->compactArena, _overridePaths, _sourceName);
	if (!overridePaths || !overridePathDetails)
		return nullptr;

	return createOverrideSpecifierAstFromRust(
		astNodeFromCompactRef(*context->compactArena, _overrides, _sourceName),
		std::move(*overridePaths),
		std::move(*overridePathDetails)
	);
}

ASTPointer<ModifierInvocation> createModifierInvocationAstFromRustCompact(
	rust_ffi::WireCompactModifierInvocationDetail const& _modifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode modifierInvocation = astNodeFromCompactRef(arena, _modifier.modifier_invocation, _sourceName);
	if (!modifierInvocation.present || modifierInvocation.kind != rustAstNodeKindModifierInvocation)
		return nullptr;
	RustParserAstNode modifierNameNode = astNodeFromCompactRef(arena, _modifier.modifier_name, _sourceName);
	if (!modifierNameNode.present)
		return nullptr;

	std::optional<RustParserCompactNameLocations> modifierNamePath =
		nameLocationsFromCompactRange(arena, _modifier.modifier_name_path, _sourceName);
	if (!modifierNamePath)
		return nullptr;
	ASTPointer<IdentifierPath> modifierName = createIdentifierPathAstFromRust(
		modifierNameNode,
		modifierNamePath->names,
		modifierNamePath->locations
	);
	if (!modifierName)
		return nullptr;

	std::unique_ptr<std::vector<ASTPointer<Expression>>> arguments;
	if (_modifier.has_arguments)
	{
		if (!compactRefRangeIsValid(arena, _modifier.arguments))
			return nullptr;
		arguments = std::make_unique<std::vector<ASTPointer<Expression>>>();
		arguments->reserve(_modifier.arguments.len);
		size_t const end = static_cast<size_t>(_modifier.arguments.start) + _modifier.arguments.len;
		for (size_t index = _modifier.arguments.start; index < end; ++index)
		{
			arguments->push_back(createExpressionAstFromRustCompactNode(arena.ref_items[index], _sourceName));
			if (!arguments->back())
				return nullptr;
		}
	}
	else if (_modifier.arguments.len != 0)
		return nullptr;

	return std::make_shared<ModifierInvocation>(
		modifierInvocation.nodeID,
		modifierInvocation.location,
		std::move(modifierName),
		std::move(arguments)
	);
}

ASTPointer<FunctionDefinition> createFunctionDefinitionAstFromRustCompact(
	rust_ffi::WireCompactFunctionDefinitionDetail const& _function,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode function = astNodeFromCompactRef(arena, _function.function_definition, _sourceName);
	if (!function.present || function.kind != rustAstNodeKindFunctionDefinition)
		return nullptr;
	RustParserAstNode documentation = astNodeFromCompactRef(arena, _function.documentation, _sourceName);
	if (!structuredDocumentationIsSupported(documentation))
		return nullptr;

	ASTPointer<OverrideSpecifier> overrides;
	if (compactNodeAt(arena, _function.overrides))
	{
		overrides = createOverrideSpecifierAstFromRustCompact(
			_function.overrides,
			_function.override_paths,
			_sourceName
		);
		if (!overrides)
			return nullptr;
	}
	else if (_function.override_paths.len != 0)
		return nullptr;

	std::optional<Visibility> visibility = visibilityFromRust(_function.visibility);
	if (!visibility.has_value())
		return nullptr;

	std::optional<StateMutability> stateMutability = stateMutabilityFromRust(_function.state_mutability);
	if (!stateMutability.has_value())
		return nullptr;

	Token kind = static_cast<Token>(_function.kind);
	if (kind != Token::Function && kind != Token::Constructor && kind != Token::Fallback && kind != Token::Receive)
		return nullptr;
	std::string name = compactText(arena, _function.name);
	if (_function.is_free_function && kind != Token::Function)
		return nullptr;
	if ((kind == Token::Function) != !name.empty())
		return nullptr;
	if (context->experimentalSolidity)
	{
		if (compactNodeAt(arena, _function.return_parameters) || _function.return_parameter_declarations.len != 0)
			return nullptr;
	}
	else if (!compactNodeAt(arena, _function.return_parameters))
		return nullptr;

	ASTPointer<ParameterList> parameters = createParameterListAstFromRustCompact(
		_function.parameters,
		_function.parameter_declarations,
		_sourceName,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<ParameterList> returnParameters;
	if (compactNodeAt(arena, _function.return_parameters))
	{
		returnParameters = createParameterListAstFromRustCompact(
			_function.return_parameters,
			_function.return_parameter_declarations,
			_sourceName,
			RustParserParameterContext::LocationAllowed
		);
		if (!returnParameters)
			return nullptr;
	}
	else if (_function.return_parameter_declarations.len != 0)
		return nullptr;

	ASTPointer<Expression> experimentalReturnExpression;
	if (compactNodeAt(arena, _function.experimental_return_expression))
	{
		experimentalReturnExpression = createExpressionAstFromRustCompactNode(
			_function.experimental_return_expression,
			_sourceName
		);
		if (!experimentalReturnExpression)
			return nullptr;
	}

	ASTPointer<Block> body;
	if (compactNodeAt(arena, _function.block))
	{
		rust_ffi::WireCompactStatementDetail const* blockDetail =
			currentCompactStatementDetailByNode(_function.block);
		if (!blockDetail || blockDetail->block_unchecked != _function.block_unchecked)
		{
			if (rust_ffi::WireCompactNode const* blockNode = compactNodeAt(arena, _function.block))
				debugCompactNodeFailure("function block detail", *blockNode);
			return nullptr;
		}
		body = createBlockAstFromRustCompact(*blockDetail, _sourceName);
		if (!body)
		{
			if (rust_ffi::WireCompactNode const* blockNode = compactNodeAt(arena, _function.block))
				debugCompactNodeFailure("function block", *blockNode);
			return nullptr;
		}
	}
	else if (_function.block_statements.len != 0)
		return nullptr;

	if (!compactRefRangeIsValid(arena, _function.modifiers))
		return nullptr;
	std::vector<ASTPointer<ModifierInvocation>> modifiers;
	modifiers.reserve(_function.modifiers.len);
	size_t const modifiersEnd = static_cast<size_t>(_function.modifiers.start) + _function.modifiers.len;
	for (size_t index = _function.modifiers.start; index < modifiersEnd; ++index)
	{
		rust_ffi::WireCompactModifierInvocationDetail const* detail =
			currentCompactModifierInvocationDetailByNode(arena.ref_items[index]);
		if (!detail)
		{
			if (rust_ffi::WireCompactNode const* modifierNode = compactNodeAt(arena, arena.ref_items[index]))
				debugCompactNodeFailure("function modifier detail", *modifierNode);
			return nullptr;
		}
		modifiers.push_back(createModifierInvocationAstFromRustCompact(*detail, _sourceName));
		if (!modifiers.back())
		{
			if (rust_ffi::WireCompactNode const* modifierNode = compactNodeAt(arena, arena.ref_items[index]))
				debugCompactNodeFailure("function modifier", *modifierNode);
			return nullptr;
		}
	}

	return std::make_shared<FunctionDefinition>(
		function.nodeID,
		function.location,
		astString(name),
		sourceLocation(_function.name_location, _sourceName),
		*visibility,
		*stateMutability,
		_function.is_free_function,
		kind,
		_function.is_virtual,
		overrides,
		createStructuredDocumentationAstFromRust(documentation),
		parameters,
		std::move(modifiers),
		returnParameters,
		body,
		experimentalReturnExpression
	);
}

ASTPointer<TypeClassName> createTypeClassNameAstFromRustCompact(
	rust_ffi::WireCompactTypeClassNameDetail const& _typeClassName,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode typeClassName = astNodeFromCompactRef(arena, _typeClassName.type_class_name, _sourceName);
	if (!typeClassName.present || typeClassName.kind != rustAstNodeKindTypeClassName)
		return nullptr;

	std::variant<Token, ASTPointer<IdentifierPath>> name;
	if (_typeClassName.is_builtin)
	{
		Token token = static_cast<Token>(_typeClassName.builtin_token);
		if (!TokenTraits::isBuiltinTypeClassName(token))
			return nullptr;
		if (compactNodeAt(arena, _typeClassName.identifier_path))
			return nullptr;
		name = token;
	}
	else
	{
		rust_ffi::WireCompactIdentifierPathDetail const* identifierPathDetail =
			currentCompactIdentifierPathDetailByNode(_typeClassName.identifier_path);
		if (!identifierPathDetail)
			return nullptr;
		std::optional<RustParserIdentifierPath> identifierPath =
			identifierPathFromCompact(*identifierPathDetail, _sourceName);
		if (!identifierPath)
			return nullptr;
		ASTPointer<IdentifierPath> path = createIdentifierPathAstFromRust(
			identifierPath->node,
			identifierPath->path,
			identifierPath->pathLocations
		);
		if (!path)
			return nullptr;
		name = std::move(path);
	}

	return std::make_shared<TypeClassName>(
		typeClassName.nodeID,
		typeClassName.location,
		std::move(name)
	);
}

ASTPointer<ForAllQuantifier> createForAllQuantifierAstFromRustCompact(
	rust_ffi::WireCompactForAllQuantifierDetail const& _quantifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode quantifier = astNodeFromCompactRef(arena, _quantifier.for_all_quantifier, _sourceName);
	if (!quantifier.present || quantifier.kind != rustAstNodeKindForAllQuantifier)
		return nullptr;

	ASTPointer<ParameterList> typeVariableDeclarations = createParameterListAstFromRustCompact(
		_quantifier.type_variable_declarations,
		_quantifier.type_variable_declaration_parameters,
		_sourceName
	);
	if (!typeVariableDeclarations)
		return nullptr;

	rust_ffi::WireCompactNode const* quantifiedFunctionNode =
		compactNodeAt(arena, _quantifier.quantified_function);
	if (!quantifiedFunctionNode)
		return nullptr;
	rust_ffi::WireCompactFunctionDefinitionDetail const* quantifiedFunctionDetail =
		currentCompactFunctionDefinitionDetailByWireNode(wireAstNodeFromCompact(*quantifiedFunctionNode));
	if (!quantifiedFunctionDetail || !quantifiedFunctionDetail->is_free_function)
		return nullptr;

	ASTPointer<FunctionDefinition> quantifiedFunction =
		createFunctionDefinitionAstFromRustCompact(*quantifiedFunctionDetail, _sourceName);
	if (!quantifiedFunction)
		return nullptr;

	return std::make_shared<ForAllQuantifier>(
		quantifier.nodeID,
		quantifier.location,
		std::move(typeVariableDeclarations),
		std::move(quantifiedFunction)
	);
}

ASTPointer<TypeDefinition> createTypeDefinitionAstFromRustCompact(
	rust_ffi::WireCompactTypeDefinitionDetail const& _typeDefinition,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode typeDefinition = astNodeFromCompactRef(arena, _typeDefinition.type_definition, _sourceName);
	if (!typeDefinition.present || typeDefinition.kind != rustAstNodeKindTypeDefinition)
		return nullptr;

	ASTPointer<ParameterList> arguments;
	if (compactNodeAt(arena, _typeDefinition.arguments))
	{
		arguments = createParameterListAstFromRustCompact(
			_typeDefinition.arguments,
			_typeDefinition.argument_parameters,
			_sourceName
		);
		if (!arguments)
			return nullptr;
	}
	else if (_typeDefinition.argument_parameters.len != 0)
		return nullptr;

	ASTPointer<Expression> expression;
	RustParserAstNode expressionNode = astNodeFromCompactRef(arena, _typeDefinition.expression, _sourceName);
	if (expressionNode.present)
	{
		if (expressionNode.kind == rustAstNodeKindBuiltin)
		{
			if (!_typeDefinition.has_builtin_name_parameter)
				return nullptr;
			expression = std::make_shared<Builtin>(
				expressionNode.nodeID,
				expressionNode.location,
				astString(compactText(arena, _typeDefinition.builtin_name_parameter)),
				sourceLocation(_typeDefinition.builtin_name_parameter_location, _sourceName)
			);
		}
		else
		{
			if (_typeDefinition.has_builtin_name_parameter)
				return nullptr;
			expression = createExpressionAstFromRustCompactNode(_typeDefinition.expression, _sourceName);
			if (!expression)
				return nullptr;
		}
	}
	else if (_typeDefinition.has_builtin_name_parameter)
		return nullptr;

	return std::make_shared<TypeDefinition>(
		typeDefinition.nodeID,
		typeDefinition.location,
		astString(compactText(arena, _typeDefinition.name)),
		sourceLocation(_typeDefinition.name_location, _sourceName),
		std::move(arguments),
		std::move(expression)
	);
}

ASTPointer<TypeClassDefinition> createTypeClassDefinitionAstFromRustCompact(
	rust_ffi::WireCompactTypeClassDefinitionDetail const& _typeClassDefinition,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode typeClassDefinition =
		astNodeFromCompactRef(arena, _typeClassDefinition.type_class_definition, _sourceName);
	if (!typeClassDefinition.present || typeClassDefinition.kind != rustAstNodeKindTypeClassDefinition)
		return nullptr;

	RustParserAstNode typeVariable = astNodeFromCompactRef(arena, _typeClassDefinition.type_variable, _sourceName);
	if (!typeVariable.present || typeVariable.kind != rustAstNodeKindVariableDeclaration)
		return nullptr;
	RustParserAstNode documentation = astNodeFromCompactRef(arena, _typeClassDefinition.documentation, _sourceName);
	if (!structuredDocumentationIsSupported(documentation) || !compactRefRangeIsValid(arena, _typeClassDefinition.sub_nodes))
		return nullptr;

	ASTPointer<VariableDeclaration> typeVariableDeclaration = std::make_shared<VariableDeclaration>(
		typeVariable.nodeID,
		typeVariable.location,
		nullptr,
		astString(compactText(arena, _typeClassDefinition.type_variable_name)),
		sourceLocation(_typeClassDefinition.type_variable_name_location, _sourceName),
		nullptr,
		Visibility::Default,
		nullptr
	);

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_typeClassDefinition.sub_nodes.len);
	size_t const subNodesEnd = static_cast<size_t>(_typeClassDefinition.sub_nodes.start) + _typeClassDefinition.sub_nodes.len;
	for (size_t index = _typeClassDefinition.sub_nodes.start; index < subNodesEnd; ++index)
	{
		rust_ffi::WireCompactNode const* functionNode = compactNodeAt(arena, arena.ref_items[index]);
		if (!functionNode)
			return nullptr;
		rust_ffi::WireCompactFunctionDefinitionDetail const* functionDetail =
			currentCompactFunctionDefinitionDetailByWireNode(wireAstNodeFromCompact(*functionNode));
		if (!functionDetail || functionDetail->is_free_function || compactNodeAt(arena, functionDetail->block))
			return nullptr;
		if (functionDetail->block_statements.len != 0)
			return nullptr;
		subNodes.push_back(createFunctionDefinitionAstFromRustCompact(*functionDetail, _sourceName));
		if (!subNodes.back())
			return nullptr;
	}

	return std::make_shared<TypeClassDefinition>(
		typeClassDefinition.nodeID,
		typeClassDefinition.location,
		std::move(typeVariableDeclaration),
		astString(compactText(arena, _typeClassDefinition.name)),
		sourceLocation(_typeClassDefinition.name_location, _sourceName),
		createStructuredDocumentationAstFromRust(documentation),
		std::move(subNodes)
	);
}

ASTPointer<TypeClassInstantiation> createTypeClassInstantiationAstFromRustCompact(
	rust_ffi::WireCompactTypeClassInstantiationDetail const& _typeClassInstantiation,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode typeClassInstantiation =
		astNodeFromCompactRef(arena, _typeClassInstantiation.type_class_instantiation, _sourceName);
	if (!typeClassInstantiation.present || typeClassInstantiation.kind != rustAstNodeKindTypeClassInstantiation)
		return nullptr;
	if (!compactRefRangeIsValid(arena, _typeClassInstantiation.sub_nodes))
		return nullptr;

	rust_ffi::WireCompactTypeNameDetail const* typeConstructorDetail =
		currentCompactTypeNameDetailByNode(_typeClassInstantiation.type_constructor);
	if (!typeConstructorDetail)
		return nullptr;
	std::optional<RustParserTypeName> typeConstructorParser = typeNameFromCompact(*typeConstructorDetail, _sourceName);
	if (!typeConstructorParser)
		return nullptr;
	ASTPointer<TypeName> typeConstructor = createTypeNameAstFromRust(*typeConstructorParser);
	if (!typeConstructor)
		return nullptr;

	ASTPointer<ParameterList> argumentSorts;
	if (compactNodeAt(arena, _typeClassInstantiation.argument_sorts))
	{
		argumentSorts = createParameterListAstFromRustCompact(
			_typeClassInstantiation.argument_sorts,
			_typeClassInstantiation.argument_sort_parameters,
			_sourceName
		);
		if (!argumentSorts)
			return nullptr;
	}
	else if (_typeClassInstantiation.argument_sort_parameters.len != 0)
		return nullptr;

	rust_ffi::WireCompactTypeClassNameDetail const* typeClassNameDetail =
		currentCompactTypeClassNameDetailByNode(_typeClassInstantiation.type_class_name);
	if (!typeClassNameDetail)
		return nullptr;
	ASTPointer<TypeClassName> typeClassName =
		createTypeClassNameAstFromRustCompact(*typeClassNameDetail, _sourceName);
	if (!typeClassName)
		return nullptr;

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_typeClassInstantiation.sub_nodes.len);
	size_t const subNodesEnd = static_cast<size_t>(_typeClassInstantiation.sub_nodes.start) + _typeClassInstantiation.sub_nodes.len;
	for (size_t index = _typeClassInstantiation.sub_nodes.start; index < subNodesEnd; ++index)
	{
		rust_ffi::WireCompactNode const* functionNode = compactNodeAt(arena, arena.ref_items[index]);
		if (!functionNode)
			return nullptr;
		rust_ffi::WireCompactFunctionDefinitionDetail const* functionDetail =
			currentCompactFunctionDefinitionDetailByWireNode(wireAstNodeFromCompact(*functionNode));
		if (!functionDetail || functionDetail->is_free_function)
			return nullptr;
		subNodes.push_back(createFunctionDefinitionAstFromRustCompact(*functionDetail, _sourceName));
		if (!subNodes.back())
			return nullptr;
	}

	return std::make_shared<TypeClassInstantiation>(
		typeClassInstantiation.nodeID,
		typeClassInstantiation.location,
		std::move(typeConstructor),
		std::move(argumentSorts),
		std::move(typeClassName),
		std::move(subNodes)
	);
}

ASTPointer<FunctionDefinition> createFunctionDefinitionAstFromRustWire(
	rust_ffi::WireFunctionDefinitionResult const& _function,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_function.function_definition.present || _function.function_definition.kind != rustAstNodeKindFunctionDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(astNodeFromWire(_function.documentation, _sourceName)))
		return nullptr;
	if (_function.modifiers.size() != _function.modifier_details.size())
		return nullptr;

	RustParserAstNode function = astNodeFromWire(_function.function_definition, _sourceName);
	ASTPointer<OverrideSpecifier> overrides;
	if (_function.overrides.present)
	{
		overrides = createOverrideSpecifierAstFromRustWire(
			_function.overrides,
			_function.override_paths,
			_function.override_path_details,
			_sourceName
		);
		if (!overrides)
			return nullptr;
	}
	else if (!_function.override_paths.empty() || !_function.override_path_details.empty())
		return nullptr;

	std::optional<Visibility> visibility = visibilityFromRust(_function.visibility);
	if (!visibility.has_value())
		return nullptr;

	std::optional<StateMutability> stateMutability = stateMutabilityFromRust(_function.state_mutability);
	if (!stateMutability.has_value())
		return nullptr;

	Token kind = static_cast<Token>(_function.kind);
	if (kind != Token::Function && kind != Token::Constructor && kind != Token::Fallback && kind != Token::Receive)
		return nullptr;
	if (_function.is_free_function && kind != Token::Function)
		return nullptr;
	if ((kind == Token::Function) != !_function.name.bytes.empty())
		return nullptr;
	if (RustParserReconstructionContext const* context = currentRustParserReconstructionContext)
	{
		if (context->experimentalSolidity)
		{
			if (
				_function.return_parameters.present ||
				!_function.return_parameter_declarations.empty() ||
				!_function.return_parameter_details.empty()
			)
				return nullptr;
		}
		else if (!_function.return_parameters.present)
			return nullptr;
	}

	ASTPointer<ParameterList> parameters = createParameterListAstFromRustWire(
		_function.parameters,
		_function.parameter_declarations,
		_function.parameter_details,
		_sourceName,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<ParameterList> returnParameters;
	if (_function.return_parameters.present)
	{
		returnParameters = createParameterListAstFromRustWire(
			_function.return_parameters,
			_function.return_parameter_declarations,
			_function.return_parameter_details,
			_sourceName,
			RustParserParameterContext::LocationAllowed
		);
		if (!returnParameters)
			return nullptr;
	}
	else if (!_function.return_parameter_declarations.empty() || !_function.return_parameter_details.empty())
		return nullptr;

	ASTPointer<Expression> experimentalReturnExpression;
	if (_function.experimental_return_expression.present)
	{
		experimentalReturnExpression = createExpressionAstFromRustWire(
			_function.experimental_return_expression,
			_function.experimental_return_expression_detail,
			_sourceName
		);
		if (!experimentalReturnExpression)
			return nullptr;
	}
	else if (_function.experimental_return_expression_detail.expression.present)
		return nullptr;

	ASTPointer<Block> body;
	if (_function.block.present)
	{
		body = createBlockAstFromRustWire(
			_function.block,
			_function.block_unchecked,
			_function.block_statements,
			_function.block_statement_details,
			_sourceName
		);
		if (!body)
			return nullptr;
	}
	else if (!_function.block_statements.empty() || !_function.block_statement_details.empty())
		return nullptr;

	std::vector<ASTPointer<ModifierInvocation>> modifiers;
	modifiers.reserve(_function.modifiers.size());
	for (rust_ffi::WireAstNode const& modifier: _function.modifiers)
	{
		auto modifierDetail = findMatchingWireDetailByNode(
			_function.modifier_details,
			modifier,
			[](auto const& _modifier) -> rust_ffi::WireAstNode const& { return _modifier.modifier_invocation; }
		);
		if (modifierDetail == _function.modifier_details.end())
			return nullptr;

		modifiers.push_back(createModifierInvocationAstFromRustWire(*modifierDetail, _sourceName));
		if (!modifiers.back())
			return nullptr;
	}

	return std::make_shared<FunctionDefinition>(
		function.nodeID,
		function.location,
		astStringFromWire(_function.name),
		sourceLocation(_function.name_location, _sourceName),
		*visibility,
		*stateMutability,
		_function.is_free_function,
		kind,
		_function.is_virtual,
		overrides,
		createStructuredDocumentationAstFromRust(astNodeFromWire(_function.documentation, _sourceName)),
		parameters,
		std::move(modifiers),
		returnParameters,
		body,
		experimentalReturnExpression
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

ASTPointer<ModifierDefinition> createModifierDefinitionAstFromRustCompact(
	rust_ffi::WireCompactModifierDefinitionDetail const& _modifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode modifier = astNodeFromCompactRef(arena, _modifier.modifier_definition, _sourceName);
	if (!modifier.present || modifier.kind != rustAstNodeKindModifierDefinition)
		return nullptr;
	RustParserAstNode documentation = astNodeFromCompactRef(arena, _modifier.documentation, _sourceName);
	if (!structuredDocumentationIsSupported(documentation))
		return nullptr;

	ASTPointer<ParameterList> parameters = createParameterListAstFromRustCompact(
		_modifier.parameters,
		_modifier.parameter_declarations,
		_sourceName,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<OverrideSpecifier> overrides;
	if (compactNodeAt(arena, _modifier.overrides))
	{
		overrides = createOverrideSpecifierAstFromRustCompact(
			_modifier.overrides,
			_modifier.override_paths,
			_sourceName
		);
		if (!overrides)
			return nullptr;
	}
	else if (_modifier.override_paths.len != 0)
		return nullptr;

	ASTPointer<Block> body;
	if (compactNodeAt(arena, _modifier.block))
	{
		rust_ffi::WireCompactStatementDetail const* blockDetail =
			currentCompactStatementDetailByNode(_modifier.block);
		if (!blockDetail || blockDetail->block_unchecked != _modifier.block_unchecked)
			return nullptr;
		body = createBlockAstFromRustCompact(*blockDetail, _sourceName);
		if (!body)
			return nullptr;
	}
	else if (_modifier.block_statements.len != 0)
		return nullptr;

	return std::make_shared<ModifierDefinition>(
		modifier.nodeID,
		modifier.location,
		astString(compactText(arena, _modifier.name)),
		sourceLocation(_modifier.name_location, _sourceName),
		createStructuredDocumentationAstFromRust(documentation),
		parameters,
		_modifier.is_virtual,
		overrides,
		body
	);
}

ASTPointer<ModifierDefinition> createModifierDefinitionAstFromRustWire(
	rust_ffi::WireModifierDefinitionResult const& _modifier,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_modifier.modifier_definition.present || _modifier.modifier_definition.kind != rustAstNodeKindModifierDefinition)
		return nullptr;
	if (!structuredDocumentationIsSupported(astNodeFromWire(_modifier.documentation, _sourceName)))
		return nullptr;

	RustParserAstNode modifier = astNodeFromWire(_modifier.modifier_definition, _sourceName);
	ASTPointer<ParameterList> parameters = createParameterListAstFromRustWire(
		_modifier.parameters,
		_modifier.parameter_declarations,
		_modifier.parameter_details,
		_sourceName,
		RustParserParameterContext::LocationAllowed
	);
	if (!parameters)
		return nullptr;

	ASTPointer<OverrideSpecifier> overrides;
	if (_modifier.overrides.present)
	{
		overrides = createOverrideSpecifierAstFromRustWire(
			_modifier.overrides,
			_modifier.override_paths,
			_modifier.override_path_details,
			_sourceName
		);
		if (!overrides)
			return nullptr;
	}
	else if (!_modifier.override_paths.empty() || !_modifier.override_path_details.empty())
		return nullptr;

	ASTPointer<Block> body;
	if (_modifier.block.present)
	{
		body = createBlockAstFromRustWire(
			_modifier.block,
			_modifier.block_unchecked,
			_modifier.block_statements,
			_modifier.block_statement_details,
			_sourceName
		);
		if (!body)
			return nullptr;
	}
	else if (!_modifier.block_statements.empty() || !_modifier.block_statement_details.empty())
		return nullptr;

	return std::make_shared<ModifierDefinition>(
		modifier.nodeID,
		modifier.location,
		astStringFromWire(_modifier.name),
		sourceLocation(_modifier.name_location, _sourceName),
		createStructuredDocumentationAstFromRust(astNodeFromWire(_modifier.documentation, _sourceName)),
		parameters,
		_modifier.is_virtual,
		overrides,
		body
	);
}

ASTPointer<InheritanceSpecifier> createInheritanceSpecifierAstFromRustCompact(
	rust_ffi::WireCompactInheritanceSpecifierDetail const& _inheritance,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode inheritance = astNodeFromCompactRef(arena, _inheritance.inheritance_specifier, _sourceName);
	if (!inheritance.present || inheritance.kind != rustAstNodeKindInheritanceSpecifier)
		return nullptr;

	std::optional<RustParserCompactNameLocations> baseNamePath =
		nameLocationsFromCompactRange(arena, _inheritance.base_name_path, _sourceName);
	if (!baseNamePath)
		return nullptr;
	ASTPointer<IdentifierPath> baseName = createIdentifierPathAstFromRust(
		astNodeFromCompactRef(arena, _inheritance.base_name, _sourceName),
		std::move(baseNamePath->names),
		std::move(baseNamePath->locations)
	);
	if (!baseName)
		return nullptr;

	std::unique_ptr<std::vector<ASTPointer<Expression>>> arguments;
	if (_inheritance.has_arguments)
	{
		if (!compactRefRangeIsValid(arena, _inheritance.arguments))
			return nullptr;
		arguments = std::make_unique<std::vector<ASTPointer<Expression>>>();
		arguments->reserve(_inheritance.arguments.len);
		size_t const end = static_cast<size_t>(_inheritance.arguments.start) + _inheritance.arguments.len;
		for (size_t index = _inheritance.arguments.start; index < end; ++index)
		{
			rust_ffi::WireCompactExpressionDetail const* argumentDetail =
				currentCompactExpressionDetailByNode(arena.ref_items[index]);
			if (!argumentDetail)
				return nullptr;
			arguments->push_back(createExpressionAstFromRustCompactNode(arena.ref_items[index], _sourceName));
			if (!arguments->back())
				return nullptr;
		}
	}
	else if (_inheritance.arguments.len != 0)
		return nullptr;

	return std::make_shared<InheritanceSpecifier>(
		inheritance.nodeID,
		inheritance.location,
		std::move(baseName),
		std::move(arguments)
	);
}

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

ASTPointer<InheritanceSpecifier> createInheritanceSpecifierAstFromRustWire(
	rust_ffi::WireInheritanceSpecifierResult const& _inheritance,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_inheritance.inheritance_specifier.present || _inheritance.inheritance_specifier.kind != rustAstNodeKindInheritanceSpecifier)
		return nullptr;

	RustParserAstNode inheritance = astNodeFromWire(_inheritance.inheritance_specifier, _sourceName);
	ASTPointer<IdentifierPath> baseName = createIdentifierPathAstFromRust(
		astNodeFromWire(_inheritance.base_name, _sourceName),
		cppVectorFromRust<std::string>(
			_inheritance.base_name_path,
			[](rust_ffi::WireString const& _pathComponent)
			{
				return cppString(_pathComponent);
			}
		),
		sourceLocationsFromWire(_inheritance.base_name_path_locations, _sourceName)
	);
	if (!baseName)
		return nullptr;

	std::unique_ptr<std::vector<ASTPointer<Expression>>> arguments;
	if (_inheritance.has_arguments)
	{
		if (_inheritance.arguments.size() != _inheritance.argument_details.size())
			return nullptr;
		arguments = std::make_unique<std::vector<ASTPointer<Expression>>>();
		arguments->reserve(_inheritance.arguments.size());
		for (size_t i = 0; i < _inheritance.arguments.size(); ++i)
		{
			arguments->push_back(createExpressionAstFromRustWire(
				_inheritance.arguments[i],
				_inheritance.argument_details[i],
				_sourceName
			));
			if (!arguments->back())
				return nullptr;
		}
	}
	else if (!_inheritance.arguments.empty() || !_inheritance.argument_details.empty())
		return nullptr;

	return std::make_shared<InheritanceSpecifier>(
		inheritance.nodeID,
		inheritance.location,
		std::move(baseName),
		std::move(arguments)
	);
}

ASTPointer<StorageLayoutSpecifier> createStorageLayoutSpecifierAstFromRustCompact(
	rust_ffi::WireCompactNodeRef const& _storageLayoutSpecifierNode,
	rust_ffi::WireCompactNodeRef const& _baseSlotExpressionNode,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode storageLayoutSpecifier =
		astNodeFromCompactRef(arena, _storageLayoutSpecifierNode, _sourceName);
	if (!storageLayoutSpecifier.present || storageLayoutSpecifier.kind != rustAstNodeKindStorageLayoutSpecifier)
		return nullptr;

	rust_ffi::WireCompactExpressionDetail const* baseSlotExpressionDetail =
		currentCompactExpressionDetailByNode(_baseSlotExpressionNode);
	if (!baseSlotExpressionDetail)
		return nullptr;
	ASTPointer<Expression> baseSlotExpression =
		createExpressionAstFromRustCompactNode(_baseSlotExpressionNode, _sourceName);
	if (!baseSlotExpression)
		return nullptr;

	return std::make_shared<StorageLayoutSpecifier>(
		storageLayoutSpecifier.nodeID,
		storageLayoutSpecifier.location,
		std::move(baseSlotExpression)
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

ASTPointer<StorageLayoutSpecifier> createStorageLayoutSpecifierAstFromRustWire(
	rust_ffi::WireAstNode const& _storageLayoutSpecifier,
	rust_ffi::WireAstNode const& _baseSlotExpressionNode,
	rust_ffi::WireExpressionResult const& _baseSlotExpressionDetail,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_storageLayoutSpecifier.present || _storageLayoutSpecifier.kind != rustAstNodeKindStorageLayoutSpecifier)
		return nullptr;

	RustParserAstNode storageLayoutSpecifier = astNodeFromWire(_storageLayoutSpecifier, _sourceName);
	ASTPointer<Expression> baseSlotExpression = createExpressionAstFromRustWire(
		_baseSlotExpressionNode,
		_baseSlotExpressionDetail,
		_sourceName
	);
	if (!baseSlotExpression)
		return nullptr;

	return std::make_shared<StorageLayoutSpecifier>(
		storageLayoutSpecifier.nodeID,
		storageLayoutSpecifier.location,
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

namespace
{

ASTPointer<ContractDefinition> createContractDefinitionAstFromRustCompact(
	rust_ffi::WireCompactContractDefinitionDetail const& _contract,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	RustParserReconstructionContext const* context = currentRustParserReconstructionContext;
	if (!context || !context->compactArena)
		return nullptr;
	rust_ffi::WireCompactParseOutput const& arena = *context->compactArena;

	RustParserAstNode contract = astNodeFromCompactRef(arena, _contract.contract_definition, _sourceName);
	if (!contract.present || contract.kind != rustAstNodeKindContractDefinition)
		return nullptr;
	RustParserAstNode documentation = astNodeFromCompactRef(arena, _contract.documentation, _sourceName);
	if (!structuredDocumentationIsSupported(documentation))
		return nullptr;
	if (!compactRefRangeIsValid(arena, _contract.base_contracts) || !compactRefRangeIsValid(arena, _contract.sub_nodes))
		return nullptr;

	std::optional<ContractKind> contractKind = contractKindFromRust(_contract.contract_kind);
	if (!contractKind.has_value())
		return nullptr;

	ASTPointer<StorageLayoutSpecifier> storageLayoutSpecifier;
	if (compactNodeAt(arena, _contract.storage_layout_specifier))
	{
		if (*contractKind != ContractKind::Contract)
			return nullptr;

		storageLayoutSpecifier = createStorageLayoutSpecifierAstFromRustCompact(
			_contract.storage_layout_specifier,
			_contract.storage_layout_base_slot_expression,
			_sourceName
		);
		if (!storageLayoutSpecifier)
			return nullptr;
	}
	else if (compactNodeAt(arena, _contract.storage_layout_base_slot_expression))
		return nullptr;

	auto createSupportedSubNode = [&](rust_ffi::WireCompactNodeRef const& _nodeRef) -> ASTPointer<ASTNode>
	{
		rust_ffi::WireCompactNode const* compactNode = compactNodeAt(arena, _nodeRef);
		if (!compactNode)
			return nullptr;
		rust_ffi::WireAstNode const wireNode = wireAstNodeFromCompact(*compactNode);

		switch (compactNode->kind)
		{
		case rustAstNodeKindStructDefinition:
			if (rust_ffi::WireCompactStructDefinitionDetail const* compactDetail =
				currentCompactStructDefinitionDetailByWireNode(wireNode))
				if (std::optional<RustParserStructDefinition> compactStruct =
					structDefinitionFromCompact(*compactDetail, _sourceName))
					return createStructDefinitionAstFromRust(*compactStruct);
			return nullptr;
		case rustAstNodeKindEnumDefinition:
			if (rust_ffi::WireCompactEnumDefinitionDetail const* compactDetail =
				currentCompactEnumDefinitionDetailByWireNode(wireNode))
				if (std::optional<RustParserEnumDefinition> compactEnum =
					enumDefinitionFromCompact(*compactDetail, _sourceName))
					return createEnumDefinitionAstFromRust(*compactEnum);
			return nullptr;
		case rustAstNodeKindEventDefinition:
			if (rust_ffi::WireCompactEventDefinitionDetail const* compactDetail =
				currentCompactEventDefinitionDetailByWireNode(wireNode))
				if (std::optional<RustParserEventDefinition> compactEvent =
					eventDefinitionFromCompact(*compactDetail, _sourceName))
					return createEventDefinitionAstFromRust(*compactEvent);
			return nullptr;
		case rustAstNodeKindErrorDefinition:
			if (rust_ffi::WireCompactErrorDefinitionDetail const* compactDetail =
				currentCompactErrorDefinitionDetailByWireNode(wireNode))
				if (std::optional<RustParserErrorDefinition> compactError =
					errorDefinitionFromCompact(*compactDetail, _sourceName))
					return createErrorDefinitionAstFromRust(*compactError);
			return nullptr;
		case rustAstNodeKindUserDefinedValueTypeDefinition:
			if (rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const* compactDetail =
				currentCompactUserDefinedValueTypeDefinitionDetailByNode(_nodeRef))
				if (std::optional<RustParserUserDefinedValueTypeDefinition> compactValueType =
					userDefinedValueTypeDefinitionFromCompact(*compactDetail, _sourceName))
					return createUserDefinedValueTypeDefinitionAstFromRust(*compactValueType);
			return nullptr;
		case rustAstNodeKindFunctionDefinition:
			if (rust_ffi::WireCompactFunctionDefinitionDetail const* compactDetail =
				currentCompactFunctionDefinitionDetailByWireNode(wireNode))
			{
				if (compactDetail->is_free_function)
					return nullptr;
				return createFunctionDefinitionAstFromRustCompact(*compactDetail, _sourceName);
			}
			return nullptr;
		case rustAstNodeKindModifierDefinition:
			if (rust_ffi::WireCompactModifierDefinitionDetail const* compactDetail =
				currentCompactModifierDefinitionDetailByWireNode(wireNode))
				return createModifierDefinitionAstFromRustCompact(*compactDetail, _sourceName);
			return nullptr;
		case rustAstNodeKindUsingForDirective:
			if (rust_ffi::WireCompactUsingDirectiveDetail const* compactDetail =
				currentCompactUsingDirectiveDetailByNode(_nodeRef))
			{
				std::optional<RustParserUsingDirective> usingDirective =
					usingDirectiveFromCompact(*compactDetail, _sourceName);
				if (usingDirective)
					return createUsingDirectiveAstFromRust(*usingDirective);
			}
			return nullptr;
		case rustAstNodeKindVariableDeclaration:
			if (rust_ffi::WireCompactVariableDeclarationDetail const* compactDetail =
				currentCompactVariableDeclarationDetailByNode(_nodeRef))
				if (std::optional<RustParserVariableDeclaration> compactVariable =
					variableDeclarationFromCompact(*compactDetail, _sourceName))
				{
					if (!variableDeclarationIsStateVariableCompatible(*compactVariable))
						return nullptr;
					return createVariableDeclarationAstFromRust(*compactVariable);
				}
			return nullptr;
		default:
			return nullptr;
		}
	};

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_contract.sub_nodes.len);
	size_t const subNodesEnd = static_cast<size_t>(_contract.sub_nodes.start) + _contract.sub_nodes.len;
	for (size_t index = _contract.sub_nodes.start; index < subNodesEnd; ++index)
	{
		rust_ffi::WireCompactNodeRef const& subNodeRef = arena.ref_items[index];
		subNodes.push_back(createSupportedSubNode(subNodeRef));
		if (!subNodes.back())
		{
			if (rust_ffi::WireCompactNode const* subNode = compactNodeAt(arena, subNodeRef))
				debugCompactNodeFailure("contract subnode", *subNode);
			return nullptr;
		}
	}

	std::vector<ASTPointer<InheritanceSpecifier>> baseContracts;
	baseContracts.reserve(_contract.base_contracts.len);
	size_t const baseContractsEnd = static_cast<size_t>(_contract.base_contracts.start) + _contract.base_contracts.len;
	for (size_t index = _contract.base_contracts.start; index < baseContractsEnd; ++index)
	{
		rust_ffi::WireCompactInheritanceSpecifierDetail const* baseContractDetail =
			currentCompactInheritanceSpecifierDetailByNode(arena.ref_items[index]);
		if (!baseContractDetail)
		{
			if (rust_ffi::WireCompactNode const* baseNode = compactNodeAt(arena, arena.ref_items[index]))
				debugCompactNodeFailure("contract base detail", *baseNode);
			return nullptr;
		}
		baseContracts.push_back(createInheritanceSpecifierAstFromRustCompact(*baseContractDetail, _sourceName));
		if (!baseContracts.back())
		{
			if (rust_ffi::WireCompactNode const* baseNode = compactNodeAt(arena, arena.ref_items[index]))
				debugCompactNodeFailure("contract base", *baseNode);
			return nullptr;
		}
	}

	return std::make_shared<ContractDefinition>(
		contract.nodeID,
		contract.location,
		astString(compactText(arena, _contract.name)),
		sourceLocation(_contract.name_location, _sourceName),
		createStructuredDocumentationAstFromRust(documentation),
		std::move(baseContracts),
		std::move(subNodes),
		*contractKind,
		_contract.is_abstract,
		std::move(storageLayoutSpecifier)
	);
}

[[maybe_unused]] ASTPointer<ContractDefinition> createContractDefinitionAstFromRustWire(
	rust_ffi::WireContractDefinitionResult const& _contract,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_contract.contract_definition.present || _contract.contract_definition.kind != rustAstNodeKindContractDefinition)
		return nullptr;
	RustParserAstNode documentation = astNodeFromWire(_contract.documentation, _sourceName);
	if (!structuredDocumentationIsSupported(documentation))
		return nullptr;
	bool const baseContractsHaveCompactDetails = std::all_of(
		_contract.base_contracts.begin(),
		_contract.base_contracts.end(),
		[](rust_ffi::WireAstNode const& _baseContract)
		{
			return currentCompactInheritanceSpecifierDetailByWireNode(_baseContract) != nullptr;
		}
	);
	if (_contract.base_contracts.size() != _contract.base_contract_details.size() && !baseContractsHaveCompactDetails)
		return nullptr;

	std::optional<ContractKind> contractKind = contractKindFromRust(_contract.contract_kind);
	if (!contractKind.has_value())
		return nullptr;

	ASTPointer<StorageLayoutSpecifier> storageLayoutSpecifier;
	if (_contract.storage_layout_specifier.present)
	{
		if (*contractKind != ContractKind::Contract)
			return nullptr;

		storageLayoutSpecifier = createStorageLayoutSpecifierAstFromRustWire(
			_contract.storage_layout_specifier,
			_contract.storage_layout_base_slot_expression,
			_contract.storage_layout_base_slot_expression_detail,
			_sourceName
		);
		if (!storageLayoutSpecifier)
			return nullptr;
	}
	else if (
		_contract.storage_layout_base_slot_expression.present ||
		_contract.storage_layout_base_slot_expression_detail.expression.present
	)
		return nullptr;

	size_t subNodeStructCursor = 0;
	size_t subNodeEnumCursor = 0;
	size_t subNodeValueTypeCursor = 0;
	size_t subNodeEventCursor = 0;
	size_t subNodeErrorCursor = 0;
	size_t subNodeFunctionCursor = 0;
	size_t subNodeModifierCursor = 0;
	size_t subNodeUsingDirectiveCursor = 0;
	size_t subNodeVariableDeclarationCursor = 0;
	auto createSupportedSubNode = [&](rust_ffi::WireAstNode const& _node) -> ASTPointer<ASTNode>
	{
		if (_node.kind == rustAstNodeKindStructDefinition)
			if (rust_ffi::WireCompactStructDefinitionDetail const* compactDetail =
				currentCompactStructDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserStructDefinition> compactStruct =
					structDefinitionFromCompact(*compactDetail, _sourceName))
					return createStructDefinitionAstFromRust(*compactStruct);

		auto structDefinition = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_structs,
			subNodeStructCursor,
			_node,
			[](auto const& _struct) -> rust_ffi::WireAstNode const& { return _struct.struct_definition; }
		);
		if (structDefinition)
		{
			if (rust_ffi::WireCompactStructDefinitionDetail const* compactDetail =
				currentCompactStructDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserStructDefinition> compactStruct =
					structDefinitionFromCompact(*compactDetail, _sourceName))
					return createStructDefinitionAstFromRust(*compactStruct);
			return createStructDefinitionAstFromRust(structDefinitionFromWire(*structDefinition, _sourceName));
		}

		if (_node.kind == rustAstNodeKindEnumDefinition)
			if (rust_ffi::WireCompactEnumDefinitionDetail const* compactDetail =
				currentCompactEnumDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserEnumDefinition> compactEnum =
					enumDefinitionFromCompact(*compactDetail, _sourceName))
					return createEnumDefinitionAstFromRust(*compactEnum);

		auto enumDefinition = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_enums,
			subNodeEnumCursor,
			_node,
			[](auto const& _enum) -> rust_ffi::WireAstNode const& { return _enum.enum_definition; }
		);
		if (enumDefinition)
		{
			if (rust_ffi::WireCompactEnumDefinitionDetail const* compactDetail =
				currentCompactEnumDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserEnumDefinition> compactEnum =
					enumDefinitionFromCompact(*compactDetail, _sourceName))
					return createEnumDefinitionAstFromRust(*compactEnum);
			return createEnumDefinitionAstFromRust(enumDefinitionFromWire(*enumDefinition, _sourceName));
		}

		auto valueType = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_user_defined_value_types,
			subNodeValueTypeCursor,
			_node,
			[](auto const& _typeDefinition) -> rust_ffi::WireAstNode const&
			{
				return _typeDefinition.user_defined_value_type_definition;
			}
		);
		if (valueType)
		{
			if (rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const* compactDetail =
				currentCompactUserDefinedValueTypeDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserUserDefinedValueTypeDefinition> compactValueType =
					userDefinedValueTypeDefinitionFromCompact(*compactDetail, _sourceName))
					return createUserDefinedValueTypeDefinitionAstFromRust(*compactValueType);
			return createUserDefinedValueTypeDefinitionAstFromRust(
				userDefinedValueTypeDefinitionFromWire(*valueType, _sourceName)
			);
		}

		if (_node.kind == rustAstNodeKindUserDefinedValueTypeDefinition)
			if (rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const* compactDetail =
				currentCompactUserDefinedValueTypeDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserUserDefinedValueTypeDefinition> compactValueType =
					userDefinedValueTypeDefinitionFromCompact(*compactDetail, _sourceName))
					return createUserDefinedValueTypeDefinitionAstFromRust(*compactValueType);

		if (_node.kind == rustAstNodeKindEventDefinition)
			if (rust_ffi::WireCompactEventDefinitionDetail const* compactDetail =
				currentCompactEventDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserEventDefinition> compactEvent =
					eventDefinitionFromCompact(*compactDetail, _sourceName))
					return createEventDefinitionAstFromRust(*compactEvent);

		auto eventDefinition = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_events,
			subNodeEventCursor,
			_node,
			[](auto const& _event) -> rust_ffi::WireAstNode const& { return _event.event_definition; }
		);
		if (eventDefinition)
		{
			if (rust_ffi::WireCompactEventDefinitionDetail const* compactDetail =
				currentCompactEventDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserEventDefinition> compactEvent =
					eventDefinitionFromCompact(*compactDetail, _sourceName))
					return createEventDefinitionAstFromRust(*compactEvent);
			return createEventDefinitionAstFromRust(eventDefinitionFromWire(*eventDefinition, _sourceName));
		}

		if (_node.kind == rustAstNodeKindErrorDefinition)
			if (rust_ffi::WireCompactErrorDefinitionDetail const* compactDetail =
				currentCompactErrorDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserErrorDefinition> compactError =
					errorDefinitionFromCompact(*compactDetail, _sourceName))
					return createErrorDefinitionAstFromRust(*compactError);

		auto errorDefinition = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_errors,
			subNodeErrorCursor,
			_node,
			[](auto const& _error) -> rust_ffi::WireAstNode const& { return _error.error_definition; }
		);
		if (errorDefinition)
		{
			if (rust_ffi::WireCompactErrorDefinitionDetail const* compactDetail =
				currentCompactErrorDefinitionDetailByWireNode(_node))
				if (std::optional<RustParserErrorDefinition> compactError =
					errorDefinitionFromCompact(*compactDetail, _sourceName))
					return createErrorDefinitionAstFromRust(*compactError);
			return createErrorDefinitionAstFromRust(errorDefinitionFromWire(*errorDefinition, _sourceName));
		}

		if (_node.kind == rustAstNodeKindFunctionDefinition)
			if (rust_ffi::WireCompactFunctionDefinitionDetail const* compactDetail =
				currentCompactFunctionDefinitionDetailByWireNode(_node))
			{
				if (compactDetail->is_free_function)
					return nullptr;
				if (ASTPointer<FunctionDefinition> compactFunction =
					createFunctionDefinitionAstFromRustCompact(*compactDetail, _sourceName))
					return compactFunction;
			}

		auto functionDefinition = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_functions,
			subNodeFunctionCursor,
			_node,
			[](auto const& _function) -> rust_ffi::WireAstNode const& { return _function.function_definition; }
		);
		if (functionDefinition)
		{
			if (functionDefinition->is_free_function)
				return nullptr;
			if (rust_ffi::WireCompactFunctionDefinitionDetail const* compactDetail =
				currentCompactFunctionDefinitionDetailByWireNode(_node))
				if (ASTPointer<FunctionDefinition> compactFunction =
					createFunctionDefinitionAstFromRustCompact(*compactDetail, _sourceName))
					return compactFunction;
			return createFunctionDefinitionAstFromRustWire(*functionDefinition, _sourceName);
		}

		if (_node.kind == rustAstNodeKindModifierDefinition)
			if (rust_ffi::WireCompactModifierDefinitionDetail const* compactDetail =
				currentCompactModifierDefinitionDetailByWireNode(_node))
				if (ASTPointer<ModifierDefinition> compactModifier =
					createModifierDefinitionAstFromRustCompact(*compactDetail, _sourceName))
					return compactModifier;

		auto modifierDefinition = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_modifiers,
			subNodeModifierCursor,
			_node,
			[](auto const& _modifier) -> rust_ffi::WireAstNode const& { return _modifier.modifier_definition; }
		);
		if (modifierDefinition)
		{
			if (rust_ffi::WireCompactModifierDefinitionDetail const* compactDetail =
				currentCompactModifierDefinitionDetailByWireNode(_node))
				if (ASTPointer<ModifierDefinition> compactModifier =
					createModifierDefinitionAstFromRustCompact(*compactDetail, _sourceName))
					return compactModifier;
			return createModifierDefinitionAstFromRustWire(*modifierDefinition, _sourceName);
		}

		auto usingDirective = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_using_directives,
			subNodeUsingDirectiveCursor,
			_node,
			[](auto const& _using) -> rust_ffi::WireAstNode const& { return _using.using_directive; }
		);
		if (usingDirective)
		{
			if (rust_ffi::WireCompactUsingDirectiveDetail const* compactDetail =
				currentCompactUsingDirectiveDetailByWireNode(_node))
				if (std::optional<RustParserUsingDirective> compactUsing =
					usingDirectiveFromCompact(*compactDetail, _sourceName))
					return createUsingDirectiveAstFromRust(*compactUsing);
			return createUsingDirectiveAstFromRust(usingDirectiveFromWire(*usingDirective, _sourceName));
		}

		if (_node.kind == rustAstNodeKindUsingForDirective)
			if (rust_ffi::WireCompactUsingDirectiveDetail const* compactDetail =
				currentCompactUsingDirectiveDetailByWireNode(_node))
				if (std::optional<RustParserUsingDirective> compactUsing =
					usingDirectiveFromCompact(*compactDetail, _sourceName))
					return createUsingDirectiveAstFromRust(*compactUsing);

		if (_node.kind == rustAstNodeKindVariableDeclaration)
			if (rust_ffi::WireCompactVariableDeclarationDetail const* compactDetail =
				currentCompactVariableDeclarationDetailByWireNode(_node))
				if (std::optional<RustParserVariableDeclaration> compactVariable =
					variableDeclarationFromCompact(*compactDetail, _sourceName))
				{
					if (!variableDeclarationIsStateVariableCompatible(*compactVariable))
						return nullptr;
					return createVariableDeclarationAstFromRust(*compactVariable);
				}

		auto variableDeclaration = findMatchingWireDetailByNodeWithCursor(
			_contract.sub_node_variable_declarations,
			subNodeVariableDeclarationCursor,
			_node,
			[](auto const& _variable) -> rust_ffi::WireAstNode const& { return _variable.variable_declaration; }
		);
		if (variableDeclaration)
		{
			std::optional<RustParserVariableDeclaration> compactVariable;
			if (rust_ffi::WireCompactVariableDeclarationDetail const* compactDetail =
				currentCompactVariableDeclarationDetailByWireNode(_node))
				compactVariable = variableDeclarationFromCompact(*compactDetail, _sourceName);
			RustParserVariableDeclaration variable =
				compactVariable ? std::move(*compactVariable) : variableDeclarationFromWire(*variableDeclaration, _sourceName);
			if (!variableDeclarationIsStateVariableCompatible(variable))
				return nullptr;
			return createVariableDeclarationAstFromRust(variable);
		}

		return nullptr;
	};

	std::vector<ASTPointer<ASTNode>> subNodes;
	subNodes.reserve(_contract.sub_nodes.size());
	for (rust_ffi::WireAstNode const& subNode: _contract.sub_nodes)
	{
		subNodes.push_back(createSupportedSubNode(subNode));
		if (!subNodes.back())
			return nullptr;
	}

	std::vector<ASTPointer<InheritanceSpecifier>> baseContracts;
	baseContracts.reserve(_contract.base_contracts.size());
	size_t baseContractDetailCursor = 0;
	for (rust_ffi::WireAstNode const& baseContract: _contract.base_contracts)
	{
		if (rust_ffi::WireCompactInheritanceSpecifierDetail const* compactDetail =
			currentCompactInheritanceSpecifierDetailByWireNode(baseContract))
		{
			baseContracts.push_back(createInheritanceSpecifierAstFromRustCompact(*compactDetail, _sourceName));
			if (!baseContracts.back())
				return nullptr;
			continue;
		}

		auto baseContractDetail = findMatchingWireDetailByNodeWithCursor(
			_contract.base_contract_details,
			baseContractDetailCursor,
			baseContract,
			[](auto const& _baseContract) -> rust_ffi::WireAstNode const&
			{
				return _baseContract.inheritance_specifier;
			}
		);
		if (!baseContractDetail)
			return nullptr;

		baseContracts.push_back(createInheritanceSpecifierAstFromRustWire(*baseContractDetail, _sourceName));
		if (!baseContracts.back())
			return nullptr;
	}

	RustParserAstNode contract = astNodeFromWire(_contract.contract_definition, _sourceName);
	return std::make_shared<ContractDefinition>(
		contract.nodeID,
		contract.location,
		astStringFromWire(_contract.name),
		sourceLocation(_contract.name_location, _sourceName),
		createStructuredDocumentationAstFromRust(documentation),
		std::move(baseContracts),
		std::move(subNodes),
		*contractKind,
		_contract.is_abstract,
		std::move(storageLayoutSpecifier)
	);
}

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

static ASTPointer<ASTNode> createSourceUnitChildAstFromRustCompact(
	rust_ffi::WireAstNode const& _node,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	switch (_node.kind)
	{
	case rustAstNodeKindPragmaDirective:
		if (rust_ffi::WireCompactPragmaDirectiveDetail const* detail =
			currentCompactPragmaDetailByWireNode(_node))
		{
			std::optional<RustParserPragmaDirective> pragma = pragmaDirectiveFromCompact(*detail, _sourceName);
			if (pragma)
				return createPragmaDirectiveAstFromRust(*pragma);
		}
		break;
	case rustAstNodeKindImportDirective:
		if (rust_ffi::WireCompactImportDirectiveDetail const* detail =
			currentCompactImportDetailByWireNode(_node))
		{
			std::optional<RustParserImportDirective> import = importDirectiveFromCompact(*detail, _sourceName);
			if (import)
				return createImportDirectiveAstFromRust(*import);
		}
		break;
	case rustAstNodeKindContractDefinition:
		if (rust_ffi::WireCompactContractDefinitionDetail const* detail =
			currentCompactContractDefinitionDetailByWireNode(_node))
			if (ASTPointer<ContractDefinition> contractDefinition =
				createContractDefinitionAstFromRustCompact(*detail, _sourceName))
				return contractDefinition;
		break;
	case rustAstNodeKindEnumDefinition:
		if (rust_ffi::WireCompactEnumDefinitionDetail const* detail =
			currentCompactEnumDefinitionDetailByWireNode(_node))
		{
			std::optional<RustParserEnumDefinition> enumDefinition = enumDefinitionFromCompact(*detail, _sourceName);
			if (enumDefinition)
				return createEnumDefinitionAstFromRust(*enumDefinition);
		}
		break;
	case rustAstNodeKindStructDefinition:
		if (rust_ffi::WireCompactStructDefinitionDetail const* detail =
			currentCompactStructDefinitionDetailByWireNode(_node))
		{
			std::optional<RustParserStructDefinition> structDefinition = structDefinitionFromCompact(*detail, _sourceName);
			if (structDefinition)
				return createStructDefinitionAstFromRust(*structDefinition);
		}
		break;
	case rustAstNodeKindEventDefinition:
		if (rust_ffi::WireCompactEventDefinitionDetail const* detail =
			currentCompactEventDefinitionDetailByWireNode(_node))
		{
			std::optional<RustParserEventDefinition> eventDefinition = eventDefinitionFromCompact(*detail, _sourceName);
			if (eventDefinition)
				return createEventDefinitionAstFromRust(*eventDefinition);
		}
		break;
	case rustAstNodeKindErrorDefinition:
		if (rust_ffi::WireCompactErrorDefinitionDetail const* detail =
			currentCompactErrorDefinitionDetailByWireNode(_node))
		{
			std::optional<RustParserErrorDefinition> errorDefinition = errorDefinitionFromCompact(*detail, _sourceName);
			if (errorDefinition)
				return createErrorDefinitionAstFromRust(*errorDefinition);
		}
		break;
	case rustAstNodeKindUserDefinedValueTypeDefinition:
		if (rust_ffi::WireCompactUserDefinedValueTypeDefinitionDetail const* detail =
			currentCompactUserDefinedValueTypeDefinitionDetailByWireNode(_node))
		{
			std::optional<RustParserUserDefinedValueTypeDefinition> valueType =
				userDefinedValueTypeDefinitionFromCompact(*detail, _sourceName);
			if (valueType)
				return createUserDefinedValueTypeDefinitionAstFromRust(*valueType);
		}
		break;
	case rustAstNodeKindFunctionDefinition:
		if (rust_ffi::WireCompactFunctionDefinitionDetail const* detail =
			currentCompactFunctionDefinitionDetailByWireNode(_node))
		{
			if (!detail->is_free_function)
				return nullptr;
			if (ASTPointer<FunctionDefinition> functionDefinition =
				createFunctionDefinitionAstFromRustCompact(*detail, _sourceName))
				return functionDefinition;
		}
		break;
	case rustAstNodeKindForAllQuantifier:
		if (rust_ffi::WireCompactForAllQuantifierDetail const* detail =
			currentCompactForAllQuantifierDetailByWireNode(_node))
			if (ASTPointer<ForAllQuantifier> quantifier =
				createForAllQuantifierAstFromRustCompact(*detail, _sourceName))
				return quantifier;
		break;
	case rustAstNodeKindTypeDefinition:
		if (rust_ffi::WireCompactTypeDefinitionDetail const* detail =
			currentCompactTypeDefinitionDetailByWireNode(_node))
			if (ASTPointer<TypeDefinition> typeDefinition =
				createTypeDefinitionAstFromRustCompact(*detail, _sourceName))
				return typeDefinition;
		break;
	case rustAstNodeKindTypeClassDefinition:
		if (rust_ffi::WireCompactTypeClassDefinitionDetail const* detail =
			currentCompactTypeClassDefinitionDetailByWireNode(_node))
			if (ASTPointer<TypeClassDefinition> typeClassDefinition =
				createTypeClassDefinitionAstFromRustCompact(*detail, _sourceName))
				return typeClassDefinition;
		break;
	case rustAstNodeKindTypeClassInstantiation:
		if (rust_ffi::WireCompactTypeClassInstantiationDetail const* detail =
			currentCompactTypeClassInstantiationDetailByWireNode(_node))
			if (ASTPointer<TypeClassInstantiation> typeClassInstantiation =
				createTypeClassInstantiationAstFromRustCompact(*detail, _sourceName))
				return typeClassInstantiation;
		break;
	case rustAstNodeKindVariableDeclaration:
		if (rust_ffi::WireCompactVariableDeclarationDetail const* detail =
			currentCompactVariableDeclarationDetailByWireNode(_node))
		{
			std::optional<RustParserVariableDeclaration> variable = variableDeclarationFromCompact(*detail, _sourceName);
			if (variable && variableDeclarationIsFileLevelCompatible(*variable))
				return createVariableDeclarationAstFromRust(*variable);
		}
		break;
	case rustAstNodeKindUsingForDirective:
		if (rust_ffi::WireCompactUsingDirectiveDetail const* detail =
			currentCompactUsingDirectiveDetailByWireNode(_node))
		{
			std::optional<RustParserUsingDirective> usingDirective = usingDirectiveFromCompact(*detail, _sourceName);
			if (usingDirective)
				return createUsingDirectiveAstFromRust(*usingDirective);
		}
		break;
	default:
		break;
	}

	return nullptr;
}

static ASTPointer<SourceUnit> createSourceUnitAstFromRustCompactIfSupported(
	rust_ffi::WireCompactParseOutput const& _arena,
	RustParserResult const& _result,
	std::string const& _source,
	std::shared_ptr<std::string const> const& _sourceName
)
{
	if (!_result.ok || !_arena.has_root)
		return nullptr;

	rust_ffi::WireCompactNode const* root = compactNodeAt(_arena, _arena.root);
	if (!root || root->kind != rustAstNodeKindSourceUnit)
		return nullptr;

	RustParserReconstructionContext context{
		_source,
		_result.sourceName,
		_result.evmVersion,
		_result.experimentalSolidity
	};
	indexCompactReconstructionDetails(context, _arena);
	RustParserReconstructionContext const* previousContext = currentRustParserReconstructionContext;
	currentRustParserReconstructionContext = &context;
	ScopeGuard resetContext([&] { currentRustParserReconstructionContext = previousContext; });

	std::optional<std::vector<rust_ffi::WireAstNode>> compactNodes = sourceUnitChildNodesFromCompact(_arena);
	if (!compactNodes)
		return nullptr;

	std::vector<ASTPointer<ASTNode>> nodes;
	nodes.reserve(compactNodes->size());
	for (rust_ffi::WireAstNode const& node: *compactNodes)
	{
		nodes.push_back(createSourceUnitChildAstFromRustCompact(node, _sourceName));
		if (!nodes.back())
		{
			debugCompactNodeFailure("source unit child", node);
			return nullptr;
		}
	}

	std::optional<std::string> license;
	if (_result.hasLicense)
		license = _result.license;

	RustParserAstNode sourceUnit = astNodeFromCompact(_arena, *root, _sourceName);
	return std::make_shared<SourceUnit>(
		sourceUnit.nodeID,
		sourceUnit.location,
		std::move(license),
		std::move(nodes),
		_result.experimentalSolidity
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
		_result.experimentalSolidity
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

static ::rust::Box<rust_ffi::CompactParserHandle> parseSourceUnitCompactWithRust(
	CharStream& _sourceCopy,
	EVMVersion _evmVersion,
	std::int64_t _currentNodeID,
	bool _includeLegacyWire = false,
	RustParserPhaseTiming* _timing = nullptr
)
{
	auto setInputStart = RustParserPhaseClock::now();
	rust_ffi::set_parser_source_input_with_evm_version(
		wireString(_sourceCopy.source()),
		_currentNodeID,
		std::string(VersionString),
		_evmVersion.name()
	);
	auto setInputEnd = RustParserPhaseClock::now();
	if (_timing)
		_timing->scannerInputNs += elapsedNs(setInputStart, setInputEnd);

	auto parseStart = RustParserPhaseClock::now();
	auto result = _includeLegacyWire ? rust_ffi::parse_compact_with_legacy() : rust_ffi::parse_compact();
	auto parseEnd = RustParserPhaseClock::now();
	if (_timing)
		_timing->rustParseNs += elapsedNs(parseStart, parseEnd);
	return result;
}

static RustParserResult parserResultMetadataFromCompact(
	rust_ffi::WireCompactParseOutput const& _arena,
	CharStream const& _sourceCopy,
	CharStream const& _charStream,
	EVMVersion _evmVersion,
	std::shared_ptr<std::string const> const& _sourceName,
	bool _includeSource = true
)
{
	RustParserResult result;
	result.ok = _arena.ok;
	result.errorCode = errorCodeFromWire(_arena.error_code);
	result.errorMessage = std::string(_arena.error_message);
	if (_includeSource)
		result.source = _sourceCopy.source();
	result.sourceName = _charStream.name();
	result.evmVersion = _evmVersion;
	result.errors = cppVectorFromRust<RustParserDiagnostic>(
		_arena.diagnostics,
		[&](rust_ffi::WireCompactDiagnostic const& _error)
		{
			return diagnosticFromCompact(_arena, _error, _sourceName);
		}
	);
	result.warnings = cppVectorFromRust<RustParserDiagnostic>(
		_arena.warnings,
		[&](rust_ffi::WireCompactDiagnostic const& _error)
		{
			return diagnosticFromCompact(_arena, _error, _sourceName);
		}
	);
	result.hasLicense = _arena.has_license;
	result.license = compactText(_arena, _arena.license);
	result.experimentalSolidity = _arena.experimental_solidity;
	result.maxID = _arena.max_id;
	return result;
}

RustParserResult solidity::frontend::parseSourceUnitWithRust(
	CharStream const& _charStream,
	EVMVersion _evmVersion,
	std::int64_t _currentNodeID
)
{
	CharStream sourceCopy(_charStream.source(), _charStream.name(), _charStream.isImportedFromAST());
	auto rustParserHandle = parseSourceUnitCompactWithRust(sourceCopy, _evmVersion, _currentNodeID, true);
	auto const& rustResult = rust_ffi::compact_parser_legacy_wire_result(*rustParserHandle);
	auto const& compactArena = rust_ffi::compact_parser_arena(*rustParserHandle);
	auto sourceName = std::make_shared<std::string const>(_charStream.name());

	RustParserResult result = parserResultMetadataFromCompact(compactArena, sourceCopy, _charStream, _evmVersion, sourceName);
	if (!result.ok)
		return result;

	if (
		compactArena.has_root &&
		compactNodeAt(compactArena, compactArena.root) &&
		compactNodeAt(compactArena, compactArena.root)->kind == rustAstNodeKindSourceUnit
	)
		result.sourceUnit = astNodeFromCompact(
			compactArena,
			*compactNodeAt(compactArena, compactArena.root),
			sourceName
		);
	else
		result.sourceUnit = astNodeFromWire(rustResult.source_unit, sourceName);

	std::optional<std::vector<rust_ffi::WireAstNode>> compactNodes = sourceUnitChildNodesFromCompact(compactArena);
	if (compactNodes)
	{
		result.sourceUnitNodes.reserve(compactNodes->size());
		for (rust_ffi::WireAstNode const& node: *compactNodes)
			result.sourceUnitNodes.push_back(astNodeFromWire(node, sourceName));
	}
	else
	{
		result.sourceUnitNodes = cppVectorFromRust<RustParserAstNode>(
			rustResult.source_unit_nodes,
			[&](rust_ffi::WireAstNode const& _node) { return astNodeFromWire(_node, sourceName); }
		);
	}
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

RustParserSourceUnitResult solidity::frontend::parseSourceUnitWithRustIfSupported(
	CharStream const& _charStream,
	EVMVersion _evmVersion,
	std::int64_t _currentNodeID
)
{
	RustParserPhaseTiming timing;
	auto totalStart = RustParserPhaseClock::now();
	auto sourcePrepStart = RustParserPhaseClock::now();
	CharStream sourceCopy(_charStream.source(), _charStream.name(), _charStream.isImportedFromAST());
	auto sourceName = std::make_shared<std::string const>(_charStream.name());
	auto sourcePrepEnd = RustParserPhaseClock::now();
	timing.sourcePrepNs = elapsedNs(sourcePrepStart, sourcePrepEnd);

	RustParserSourceUnitResult result;
	RustParserPhaseClock::time_point legacyDropStart;
	{
		auto rustParserHandle = parseSourceUnitCompactWithRust(sourceCopy, _evmVersion, _currentNodeID, false, &timing);
		auto const& compactArena = rust_ffi::compact_parser_arena(*rustParserHandle);
		timing.compactExpressionDetails = compactArena.expression_details.size();
		timing.compactStatementDetails = compactArena.statement_details.size();
		timing.compactTryCatchClauseDetails = compactArena.try_catch_clause_details.size();

		auto metadataStart = RustParserPhaseClock::now();
		result.result = parserResultMetadataFromCompact(compactArena, sourceCopy, _charStream, _evmVersion, sourceName, false);
		auto metadataEnd = RustParserPhaseClock::now();
		timing.metadataNs = elapsedNs(metadataStart, metadataEnd);
		if (result.result.ok)
		{
			auto astStart = RustParserPhaseClock::now();
			if (compactDetailTablesAreIndexed(compactArena))
				result.sourceUnit = createSourceUnitAstFromRustCompactIfSupported(
					compactArena,
					result.result,
					sourceCopy.source(),
					sourceName
				);
			auto astEnd = RustParserPhaseClock::now();
			timing.astReconstructionNs = elapsedNs(astStart, astEnd);
		}

		legacyDropStart = RustParserPhaseClock::now();
	}
	timing.legacyDropNs = elapsedNs(legacyDropStart, RustParserPhaseClock::now());
	timing.totalNs = elapsedNs(totalStart, RustParserPhaseClock::now());
	if (rustParserPhaseTimingEnabled())
		std::cerr
			<< "RUST_PARSER_PHASE"
			<< ",source_bytes=" << sourceCopy.source().size()
			<< ",source_prep_ns=" << timing.sourcePrepNs
			<< ",scanner_input_ns=" << timing.scannerInputNs
			<< ",rust_parse_ns=" << timing.rustParseNs
			<< ",metadata_ns=" << timing.metadataNs
			<< ",ast_reconstruction_ns=" << timing.astReconstructionNs
			<< ",legacy_drop_ns=" << timing.legacyDropNs
			<< ",compact_expression_details=" << timing.compactExpressionDetails
			<< ",compact_statement_details=" << timing.compactStatementDetails
			<< ",compact_try_catch_clause_details=" << timing.compactTryCatchClauseDetails
			<< ",total_ns=" << timing.totalNs
			<< ",ok=" << (result.sourceUnit ? 1 : 0)
			<< "\n";
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
