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

#include <libyul/RustYulOptimizerBridge.h>

#if defined(SOLIDITY_USE_RUST_YUL_OPTIMIZER)

#include <yul_optimizer_bridge/bridge.h>

#include <libyul/Builtins.h>
#include <libyul/Dialect.h>
#include <libyul/Object.h>
#include <libyul/Utilities.h>
#include <libyul/backends/evm/EVMDialect.h>
#include <libyul/backends/evm/EVMBuiltins.h>

#include <liblangutil/DebugData.h>
#include <liblangutil/EVMVersion.h>

#include <libsolutil/Numeric.h>

#include <algorithm>
#include <limits>
#include <map>
#include <memory>
#include <stdexcept>
#include <utility>
#include <vector>

using namespace solidity;
using namespace solidity::yul;

namespace
{

namespace rust_ffi = solidity::yul::rust;

static_assert(sizeof(size_t) <= sizeof(std::uint64_t));

constexpr std::uint8_t c_statementExpression = 0;
constexpr std::uint8_t c_statementAssignment = 1;
constexpr std::uint8_t c_statementVariableDeclaration = 2;
constexpr std::uint8_t c_statementFunctionDefinition = 3;
constexpr std::uint8_t c_statementIf = 4;
constexpr std::uint8_t c_statementSwitch = 5;
constexpr std::uint8_t c_statementForLoop = 6;
constexpr std::uint8_t c_statementBreak = 7;
constexpr std::uint8_t c_statementContinue = 8;
constexpr std::uint8_t c_statementLeave = 9;
constexpr std::uint8_t c_statementBlock = 10;

constexpr std::uint8_t c_expressionFunctionCall = 0;
constexpr std::uint8_t c_expressionIdentifier = 1;
constexpr std::uint8_t c_expressionLiteral = 2;

constexpr std::uint8_t c_functionNameIdentifier = 0;
constexpr std::uint8_t c_functionNameBuiltin = 1;

constexpr std::uint8_t c_literalNumber = 0;
constexpr std::uint8_t c_literalBoolean = 1;
constexpr std::uint8_t c_literalString = 2;
constexpr std::uint8_t c_literalArgumentUnrestricted = 0xff;

RustYulOptimizerErrorCode errorCodeFromWire(std::uint8_t _code)
{
	switch (_code)
	{
	case 0:
		return RustYulOptimizerErrorCode::None;
	case 1:
		return RustYulOptimizerErrorCode::InvalidWire;
	default:
		return RustYulOptimizerErrorCode::Unknown;
	}
}

RustYulOptimizerResult errorResult(RustYulOptimizerErrorCode _code, std::string _message)
{
	return RustYulOptimizerResult{false, _code, std::move(_message), {}};
}

::rust::Vec<std::uint8_t> rustBytes(std::string_view _bytes)
{
	::rust::Vec<std::uint8_t> output;
	for (char byte: _bytes)
		output.push_back(static_cast<std::uint8_t>(byte));
	return output;
}

::rust::Vec<std::uint8_t> rustBytes(bytes const& _bytes)
{
	::rust::Vec<std::uint8_t> output;
	for (std::uint8_t byte: _bytes)
		output.push_back(byte);
	return output;
}

bytes cppBytes(::rust::Vec<std::uint8_t> const& _bytes)
{
	bytes output;
	output.reserve(_bytes.size());
	for (std::uint8_t byte: _bytes)
		output.push_back(byte);
	return output;
}

std::string cppString(rust_ffi::WireString const& _string)
{
	std::string output;
	output.reserve(_string.bytes.size());
	for (std::uint8_t byte: _string.bytes)
		output.push_back(static_cast<char>(byte));
	return output;
}

::rust::Vec<std::uint8_t> wireU256(u256 const& _value)
{
	return rustBytes(toBigEndian(_value));
}

u256 cppU256(::rust::Vec<std::uint8_t> const& _bytes)
{
	if (_bytes.size() != 32)
		throw std::runtime_error("Rust Yul optimizer returned an invalid integer width.");
	return fromBigEndian<u256>(cppBytes(_bytes));
}

std::uint8_t wireLiteralKind(LiteralKind _kind)
{
	switch (_kind)
	{
	case LiteralKind::Number:
		return c_literalNumber;
	case LiteralKind::Boolean:
		return c_literalBoolean;
	case LiteralKind::String:
		return c_literalString;
	}
	util::unreachable();
}

LiteralKind literalKind(std::uint8_t _kind)
{
	switch (_kind)
	{
	case c_literalNumber:
		return LiteralKind::Number;
	case c_literalBoolean:
		return LiteralKind::Boolean;
	case c_literalString:
		return LiteralKind::String;
	default:
		throw std::runtime_error("Rust Yul optimizer returned an invalid literal kind.");
	}
}

std::uint8_t wireEffect(SideEffects::Effect _effect)
{
	return static_cast<std::uint8_t>(_effect);
}

::rust::Vec<std::uint64_t> wireStringIDs(std::set<std::string> const& _strings, class StringTable& _stringTable);

class StringTable
{
public:
	StringTable()
	{
		id("");
	}

	std::uint64_t id(std::string_view _string)
	{
		std::string key{_string};
		if (auto it = m_ids.find(key); it != m_ids.end())
			return it->second;

		std::uint64_t newID = static_cast<std::uint64_t>(m_strings.size());
		rust_ffi::WireString wireString;
		wireString.bytes = rustBytes(key);
		m_strings.push_back(std::move(wireString));
		m_ids.emplace(std::move(key), newID);
		return newID;
	}

	::rust::Vec<rust_ffi::WireString> takeStrings()
	{
		return std::move(m_strings);
	}

private:
	std::map<std::string, std::uint64_t, std::less<>> m_ids;
	::rust::Vec<rust_ffi::WireString> m_strings;
};

::rust::Vec<std::uint64_t> wireStringIDs(std::set<std::string> const& _strings, StringTable& _stringTable)
{
	::rust::Vec<std::uint64_t> output;
	for (std::string const& item: _strings)
		output.push_back(_stringTable.id(item));
	return output;
}

class DebugDataTable
{
public:
	std::uint64_t id(langutil::DebugData::ConstPtr const& _debugData)
	{
		auto [it, inserted] = m_ids.emplace(_debugData, m_debugDataByID.size());
		if (inserted)
			m_debugDataByID.push_back(_debugData);
		return static_cast<std::uint64_t>(it->second);
	}

	langutil::DebugData::ConstPtr debugData(std::uint64_t _id) const
	{
		if (_id == std::numeric_limits<std::uint64_t>::max())
			return langutil::DebugData::create();
		if (_id > std::numeric_limits<size_t>::max() || static_cast<size_t>(_id) >= m_debugDataByID.size())
			throw std::runtime_error("Rust Yul optimizer returned an unknown debug data ID.");
		return m_debugDataByID[static_cast<size_t>(_id)];
	}

private:
	std::map<langutil::DebugData::ConstPtr, size_t, std::owner_less<langutil::DebugData::ConstPtr>> m_ids;
	std::vector<langutil::DebugData::ConstPtr> m_debugDataByID;
};

template <class T>
T const& checkedAt(::rust::Vec<T> const& _items, std::uint64_t _id, char const* _description)
{
	if (_id > std::numeric_limits<size_t>::max() || static_cast<size_t>(_id) >= _items.size())
		throw std::runtime_error(
			std::string("Rust Yul optimizer returned an invalid ") + _description + " ID."
		);
	return _items[static_cast<size_t>(_id)];
}

struct WireArena
{
	::rust::Vec<rust_ffi::WireNameWithDebugData> names;
	::rust::Vec<rust_ffi::WireIdentifier> identifiers;
	::rust::Vec<rust_ffi::WireBlock> blocks;
	::rust::Vec<rust_ffi::WireStatement> statements;
	::rust::Vec<rust_ffi::WireExpression> expressions;
	::rust::Vec<rust_ffi::WireCase> cases;
};

class BlockEncoder
{
public:
	BlockEncoder(StringTable& _stringTable, DebugDataTable& _debugDataTable):
		m_stringTable(_stringTable),
		m_debugDataTable(_debugDataTable)
	{
	}

	std::uint64_t block(Block const& _block)
	{
		rust_ffi::WireBlock wireBlock;
		wireBlock.debug_data_id = m_debugDataTable.id(_block.debugData);
		for (Statement const& statement: _block.statements)
			wireBlock.statement_ids.push_back(this->statement(statement));

		return push(m_arena.blocks, std::move(wireBlock));
	}

	WireArena takeArena()
	{
		return std::move(m_arena);
	}

private:
	template <class T>
	std::uint64_t push(::rust::Vec<T>& _items, T _item)
	{
		std::uint64_t id = static_cast<std::uint64_t>(_items.size());
		_items.push_back(std::move(_item));
		return id;
	}

	std::uint64_t nameWithDebugData(NameWithDebugData const& _name)
	{
		rust_ffi::WireNameWithDebugData wireName;
		wireName.debug_data_id = m_debugDataTable.id(_name.debugData);
		wireName.name_id = m_stringTable.id(_name.name.str());
		return push(m_arena.names, std::move(wireName));
	}

	std::uint64_t identifier(Identifier const& _identifier)
	{
		rust_ffi::WireIdentifier wireIdentifier;
		wireIdentifier.debug_data_id = m_debugDataTable.id(_identifier.debugData);
		wireIdentifier.name_id = m_stringTable.id(_identifier.name.str());
		return push(m_arena.identifiers, std::move(wireIdentifier));
	}

	std::uint64_t statement(Statement const& _statement)
	{
		return std::visit([&](auto const& _typedStatement) {
			return this->statement(_typedStatement);
		}, _statement);
	}

	std::uint64_t statement(ExpressionStatement const& _statement)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementExpression;
		wireStatement.debug_data_id = m_debugDataTable.id(_statement.debugData);
		wireStatement.expression_id = expression(_statement.expression);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(Assignment const& _assignment)
	{
		yulAssert(_assignment.value, "Assignment without value cannot be serialized.");
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementAssignment;
		wireStatement.debug_data_id = m_debugDataTable.id(_assignment.debugData);
		for (Identifier const& variable: _assignment.variableNames)
			wireStatement.variable_ids.push_back(identifier(variable));
		wireStatement.has_value = true;
		wireStatement.value_expression_id = expression(*_assignment.value);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(VariableDeclaration const& _varDecl)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementVariableDeclaration;
		wireStatement.debug_data_id = m_debugDataTable.id(_varDecl.debugData);
		for (NameWithDebugData const& variable: _varDecl.variables)
			wireStatement.variable_ids.push_back(nameWithDebugData(variable));
		wireStatement.has_value = static_cast<bool>(_varDecl.value);
		if (_varDecl.value)
			wireStatement.value_expression_id = expression(*_varDecl.value);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(FunctionDefinition const& _function)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementFunctionDefinition;
		wireStatement.debug_data_id = m_debugDataTable.id(_function.debugData);
		wireStatement.name_id = m_stringTable.id(_function.name.str());
		for (NameWithDebugData const& parameter: _function.parameters)
			wireStatement.parameter_ids.push_back(nameWithDebugData(parameter));
		for (NameWithDebugData const& returnVariable: _function.returnVariables)
			wireStatement.return_variable_ids.push_back(nameWithDebugData(returnVariable));
		wireStatement.body_block_id = block(_function.body);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(If const& _if)
	{
		yulAssert(_if.condition, "If without condition cannot be serialized.");
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementIf;
		wireStatement.debug_data_id = m_debugDataTable.id(_if.debugData);
		wireStatement.condition_expression_id = expression(*_if.condition);
		wireStatement.body_block_id = block(_if.body);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(Switch const& _switch)
	{
		yulAssert(_switch.expression, "Switch without expression cannot be serialized.");
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementSwitch;
		wireStatement.debug_data_id = m_debugDataTable.id(_switch.debugData);
		wireStatement.switch_expression_id = expression(*_switch.expression);
		for (Case const& switchCase: _switch.cases)
			wireStatement.case_ids.push_back(this->switchCase(switchCase));
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(ForLoop const& _forLoop)
	{
		yulAssert(_forLoop.condition, "For loop without condition cannot be serialized.");
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementForLoop;
		wireStatement.debug_data_id = m_debugDataTable.id(_forLoop.debugData);
		wireStatement.pre_block_id = block(_forLoop.pre);
		wireStatement.condition_expression_id = expression(*_forLoop.condition);
		wireStatement.post_block_id = block(_forLoop.post);
		wireStatement.body_block_id = block(_forLoop.body);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(Break const& _break)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementBreak;
		wireStatement.debug_data_id = m_debugDataTable.id(_break.debugData);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(Continue const& _continue)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementContinue;
		wireStatement.debug_data_id = m_debugDataTable.id(_continue.debugData);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(Leave const& _leave)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementLeave;
		wireStatement.debug_data_id = m_debugDataTable.id(_leave.debugData);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t statement(Block const& _block)
	{
		rust_ffi::WireStatement wireStatement;
		wireStatement.kind = c_statementBlock;
		wireStatement.debug_data_id = m_debugDataTable.id(_block.debugData);
		wireStatement.block_id = block(_block);
		return push(m_arena.statements, std::move(wireStatement));
	}

	std::uint64_t expression(Expression const& _expression)
	{
		return std::visit([&](auto const& _typedExpression) {
			return this->expression(_typedExpression);
		}, _expression);
	}

	std::uint64_t expression(FunctionCall const& _functionCall)
	{
		rust_ffi::WireExpression wireExpression;
		wireExpression.kind = c_expressionFunctionCall;
		wireExpression.debug_data_id = m_debugDataTable.id(_functionCall.debugData);
		std::visit([&](auto const& _functionName) {
			this->functionName(_functionName, wireExpression);
		}, _functionCall.functionName);
		for (Expression const& argument: _functionCall.arguments)
			wireExpression.argument_expression_ids.push_back(expression(argument));
		return push(m_arena.expressions, std::move(wireExpression));
	}

	std::uint64_t expression(Identifier const& _identifier)
	{
		rust_ffi::WireExpression wireExpression;
		wireExpression.kind = c_expressionIdentifier;
		wireExpression.debug_data_id = m_debugDataTable.id(_identifier.debugData);
		wireExpression.name_id = m_stringTable.id(_identifier.name.str());
		return push(m_arena.expressions, std::move(wireExpression));
	}

	std::uint64_t expression(Literal const& _literal)
	{
		rust_ffi::WireExpression wireExpression;
		wireExpression.kind = c_expressionLiteral;
		wireExpression.debug_data_id = m_debugDataTable.id(_literal.debugData);
		wireExpression.literal_kind = wireLiteralKind(_literal.kind);
		wireExpression.literal_unlimited = _literal.value.unlimited();
		if (_literal.value.unlimited())
			wireExpression.literal_string_id = m_stringTable.id(_literal.value.builtinStringLiteralValue());
		else
		{
			wireExpression.literal_value = wireU256(_literal.value.value());
			wireExpression.has_literal_hint = static_cast<bool>(_literal.value.hint());
			if (_literal.value.hint())
				wireExpression.literal_hint_id = m_stringTable.id(*_literal.value.hint());
		}
		return push(m_arena.expressions, std::move(wireExpression));
	}

	void functionName(Identifier const& _identifier, rust_ffi::WireExpression& _wireExpression)
	{
		_wireExpression.function_name_kind = c_functionNameIdentifier;
		_wireExpression.function_name_debug_data_id = m_debugDataTable.id(_identifier.debugData);
		_wireExpression.function_name_name_id = m_stringTable.id(_identifier.name.str());
	}

	void functionName(BuiltinName const& _builtinName, rust_ffi::WireExpression& _wireExpression)
	{
		_wireExpression.function_name_kind = c_functionNameBuiltin;
		_wireExpression.function_name_debug_data_id = m_debugDataTable.id(_builtinName.debugData);
		_wireExpression.function_name_builtin_handle = static_cast<std::uint64_t>(_builtinName.handle.id);
	}

	std::uint64_t switchCase(Case const& _case)
	{
		rust_ffi::WireCase wireCase;
		wireCase.debug_data_id = m_debugDataTable.id(_case.debugData);
		wireCase.has_value = static_cast<bool>(_case.value);
		if (_case.value)
			wireCase.value_expression_id = expression(*_case.value);
		wireCase.body_block_id = block(_case.body);
		return push(m_arena.cases, std::move(wireCase));
	}

	StringTable& m_stringTable;
	DebugDataTable& m_debugDataTable;
	WireArena m_arena;
};

class BlockDecoder
{
public:
	BlockDecoder(rust_ffi::OptimizerResult const& _result, DebugDataTable const& _debugDataTable):
		m_result(_result),
		m_debugDataTable(_debugDataTable)
	{
	}

	Block block(std::uint64_t _id)
	{
		rust_ffi::WireBlock const& wireBlock = checkedAt(m_result.blocks, _id, "block");
		Block output;
		output.debugData = m_debugDataTable.debugData(wireBlock.debug_data_id);
		for (std::uint64_t statementID: wireBlock.statement_ids)
			output.statements.push_back(statement(statementID));
		return output;
	}

private:
	std::string string(std::uint64_t _id) const
	{
		return cppString(checkedAt(m_result.strings, _id, "string"));
	}

	YulName name(std::uint64_t _id) const
	{
		return YulName{string(_id)};
	}

	NameWithDebugData nameWithDebugData(std::uint64_t _id)
	{
		rust_ffi::WireNameWithDebugData const& wireName = checkedAt(m_result.names, _id, "name");
		return NameWithDebugData{m_debugDataTable.debugData(wireName.debug_data_id), name(wireName.name_id)};
	}

	Identifier identifier(std::uint64_t _id)
	{
		rust_ffi::WireIdentifier const& wireIdentifier = checkedAt(m_result.identifiers, _id, "identifier");
		return Identifier{m_debugDataTable.debugData(wireIdentifier.debug_data_id), name(wireIdentifier.name_id)};
	}

	Statement statement(std::uint64_t _id)
	{
		rust_ffi::WireStatement const& wireStatement = checkedAt(m_result.statements, _id, "statement");
		auto debugData = m_debugDataTable.debugData(wireStatement.debug_data_id);
		switch (wireStatement.kind)
		{
		case c_statementExpression:
			return ExpressionStatement{debugData, expression(wireStatement.expression_id)};
		case c_statementAssignment:
		{
			std::vector<Identifier> variableNames;
			for (std::uint64_t variableID: wireStatement.variable_ids)
				variableNames.push_back(identifier(variableID));
			return Assignment{
				debugData,
				std::move(variableNames),
				std::make_unique<Expression>(expression(wireStatement.value_expression_id))
			};
		}
		case c_statementVariableDeclaration:
		{
			NameWithDebugDataList variables;
			for (std::uint64_t variableID: wireStatement.variable_ids)
				variables.push_back(nameWithDebugData(variableID));
			return VariableDeclaration{
				debugData,
				std::move(variables),
				wireStatement.has_value ?
					std::make_unique<Expression>(expression(wireStatement.value_expression_id)) :
					nullptr
			};
		}
		case c_statementFunctionDefinition:
		{
			NameWithDebugDataList parameters;
			for (std::uint64_t parameterID: wireStatement.parameter_ids)
				parameters.push_back(nameWithDebugData(parameterID));
			NameWithDebugDataList returnVariables;
			for (std::uint64_t returnVariableID: wireStatement.return_variable_ids)
				returnVariables.push_back(nameWithDebugData(returnVariableID));
			return FunctionDefinition{
				debugData,
				name(wireStatement.name_id),
				std::move(parameters),
				std::move(returnVariables),
				block(wireStatement.body_block_id)
			};
		}
		case c_statementIf:
			return If{
				debugData,
				std::make_unique<Expression>(expression(wireStatement.condition_expression_id)),
				block(wireStatement.body_block_id)
			};
		case c_statementSwitch:
		{
			std::vector<Case> cases;
			for (std::uint64_t caseID: wireStatement.case_ids)
				cases.push_back(switchCase(caseID));
			return Switch{
				debugData,
				std::make_unique<Expression>(expression(wireStatement.switch_expression_id)),
				std::move(cases)
			};
		}
		case c_statementForLoop:
			return ForLoop{
				debugData,
				block(wireStatement.pre_block_id),
				std::make_unique<Expression>(expression(wireStatement.condition_expression_id)),
				block(wireStatement.post_block_id),
				block(wireStatement.body_block_id)
			};
		case c_statementBreak:
			return Break{debugData};
		case c_statementContinue:
			return Continue{debugData};
		case c_statementLeave:
			return Leave{debugData};
		case c_statementBlock:
			return block(wireStatement.block_id);
		default:
			throw std::runtime_error("Rust Yul optimizer returned an invalid statement kind.");
		}
	}

	Expression expression(std::uint64_t _id)
	{
		rust_ffi::WireExpression const& wireExpression = checkedAt(m_result.expressions, _id, "expression");
		auto debugData = m_debugDataTable.debugData(wireExpression.debug_data_id);
		switch (wireExpression.kind)
		{
		case c_expressionFunctionCall:
		{
			std::vector<Expression> arguments;
			for (std::uint64_t argumentID: wireExpression.argument_expression_ids)
				arguments.push_back(expression(argumentID));
			return FunctionCall{debugData, functionName(wireExpression), std::move(arguments)};
		}
		case c_expressionIdentifier:
			return Identifier{debugData, name(wireExpression.name_id)};
		case c_expressionLiteral:
			return literal(wireExpression);
		default:
			throw std::runtime_error("Rust Yul optimizer returned an invalid expression kind.");
		}
	}

	FunctionName functionName(rust_ffi::WireExpression const& _wireExpression)
	{
		auto debugData = m_debugDataTable.debugData(_wireExpression.function_name_debug_data_id);
		switch (_wireExpression.function_name_kind)
		{
		case c_functionNameIdentifier:
			return Identifier{debugData, name(_wireExpression.function_name_name_id)};
		case c_functionNameBuiltin:
			if (_wireExpression.function_name_builtin_handle > std::numeric_limits<size_t>::max())
				throw std::runtime_error("Rust Yul optimizer returned an invalid builtin handle.");
			return BuiltinName{
				debugData,
				BuiltinHandle{static_cast<size_t>(_wireExpression.function_name_builtin_handle)}
			};
		default:
			throw std::runtime_error("Rust Yul optimizer returned an invalid function name kind.");
		}
	}

	Literal literal(rust_ffi::WireExpression const& _wireExpression)
	{
		auto debugData = m_debugDataTable.debugData(_wireExpression.debug_data_id);
		LiteralKind kind = literalKind(_wireExpression.literal_kind);
		if (_wireExpression.literal_unlimited)
			return Literal{debugData, kind, LiteralValue{string(_wireExpression.literal_string_id)}};

		std::optional<std::string> hint;
		if (_wireExpression.has_literal_hint)
			hint = string(_wireExpression.literal_hint_id);
		return Literal{debugData, kind, LiteralValue{cppU256(_wireExpression.literal_value), hint}};
	}

	Case switchCase(std::uint64_t _id)
	{
		rust_ffi::WireCase const& wireCase = checkedAt(m_result.cases, _id, "case");
		Case output;
		output.debugData = m_debugDataTable.debugData(wireCase.debug_data_id);
		if (wireCase.has_value)
		{
			Expression caseValue = expression(wireCase.value_expression_id);
			if (!std::holds_alternative<Literal>(caseValue))
				throw std::runtime_error("Rust Yul optimizer returned a non-literal switch case value.");
			output.value = std::make_unique<Literal>(std::move(std::get<Literal>(caseValue)));
		}
		output.body = block(wireCase.body_block_id);
		return output;
	}

	rust_ffi::OptimizerResult const& m_result;
	DebugDataTable const& m_debugDataTable;
};

rust_ffi::WireSpecialHandles wireSpecialHandles(Dialect const& _dialect)
{
	rust_ffi::WireSpecialHandles handles;

	auto set = [](std::optional<BuiltinHandle> _handle, bool& _hasHandle, std::uint64_t& _value) {
		_hasHandle = _handle.has_value();
		_value = _handle ? static_cast<std::uint64_t>(_handle->id) : 0;
	};

	set(_dialect.discardFunctionHandle(), handles.has_discard, handles.discard);
	set(_dialect.equalityFunctionHandle(), handles.has_equality, handles.equality);
	set(_dialect.booleanNegationFunctionHandle(), handles.has_boolean_negation, handles.boolean_negation);
	set(_dialect.memoryStoreFunctionHandle(), handles.has_memory_store, handles.memory_store);
	set(_dialect.memoryLoadFunctionHandle(), handles.has_memory_load, handles.memory_load);
	set(_dialect.storageStoreFunctionHandle(), handles.has_storage_store, handles.storage_store);
	set(_dialect.storageLoadFunctionHandle(), handles.has_storage_load, handles.storage_load);
	set(_dialect.hashFunctionHandle(), handles.has_hash, handles.hash);
	set(_dialect.findBuiltin("memoryguard"), handles.has_memoryguard, handles.memoryguard);

	if (auto const* evmDialect = dynamic_cast<EVMDialect const*>(&_dialect))
	{
		auto const& auxiliaryHandles = evmDialect->auxiliaryBuiltinHandles();
		set(auxiliaryHandles.add, handles.has_add, handles.add);
		set(auxiliaryHandles.exp, handles.has_exp, handles.exp);
		set(auxiliaryHandles.mul, handles.has_mul, handles.mul);
		set(auxiliaryHandles.not_, handles.has_not, handles.not_);
		set(auxiliaryHandles.shl, handles.has_shl, handles.shl);
		set(auxiliaryHandles.sub, handles.has_sub, handles.sub);
	}

	return handles;
}

rust_ffi::WireDialect wireDialect(Dialect const& _dialect)
{
	rust_ffi::WireDialect wireDialect;
	if (auto const* evmDialect = dynamic_cast<EVMDialect const*>(&_dialect))
	{
		auto versions = langutil::EVMVersion::allVersions();
		auto it = std::find(versions.begin(), versions.end(), evmDialect->evmVersion());
		yulAssert(it != versions.end(), "");
		wireDialect.evm_version = static_cast<std::uint16_t>(it - versions.begin());
		wireDialect.provides_object_access = evmDialect->providesObjectAccess();
		wireDialect.can_overcharge_gas_for_call = evmDialect->evmVersion().canOverchargeGasForCall();
	}
	return wireDialect;
}

rust_ffi::WireBuiltinFunction wireBuiltinFunction(
	BuiltinHandle _handle,
	BuiltinFunctionForEVM const& _builtin,
	StringTable& _stringTable
)
{
	rust_ffi::WireBuiltinFunction wireBuiltin;
	wireBuiltin.handle_id = static_cast<std::uint64_t>(_handle.id);
	wireBuiltin.name_id = _stringTable.id(_builtin.name);
	wireBuiltin.num_parameters = static_cast<std::uint64_t>(_builtin.numParameters);
	wireBuiltin.num_returns = static_cast<std::uint64_t>(_builtin.numReturns);
	wireBuiltin.movable = _builtin.sideEffects.movable;
	wireBuiltin.movable_apart_from_effects = _builtin.sideEffects.movableApartFromEffects;
	wireBuiltin.can_be_removed = _builtin.sideEffects.canBeRemoved;
	wireBuiltin.can_be_removed_if_no_msize = _builtin.sideEffects.canBeRemovedIfNoMSize;
	wireBuiltin.cannot_loop = _builtin.sideEffects.cannotLoop;
	wireBuiltin.other_state = wireEffect(_builtin.sideEffects.otherState);
	wireBuiltin.storage = wireEffect(_builtin.sideEffects.storage);
	wireBuiltin.memory = wireEffect(_builtin.sideEffects.memory);
	wireBuiltin.transient_storage = wireEffect(_builtin.sideEffects.transientStorage);
	wireBuiltin.control_flow_can_terminate = _builtin.controlFlowSideEffects.canTerminate;
	wireBuiltin.control_flow_can_revert = _builtin.controlFlowSideEffects.canRevert;
	wireBuiltin.control_flow_can_continue = _builtin.controlFlowSideEffects.canContinue;
	wireBuiltin.is_msize = _builtin.isMSize;

	for (size_t i = 0; i < _builtin.literalArguments.size(); ++i)
	{
		std::optional<LiteralKind> literalKind = _builtin.literalArgument(i);
		wireBuiltin.literal_argument_kinds.push_back(
			literalKind ? wireLiteralKind(*literalKind) : c_literalArgumentUnrestricted
		);
	}

	wireBuiltin.has_evm_opcode = _builtin.instruction.has_value();
	wireBuiltin.evm_opcode = _builtin.instruction ?
		static_cast<std::uint16_t>(*_builtin.instruction) :
		0;

	return wireBuiltin;
}

::rust::Vec<rust_ffi::WireBuiltinFunction> wireBuiltins(Dialect const& _dialect, StringTable& _stringTable)
{
	::rust::Vec<rust_ffi::WireBuiltinFunction> output;

	if (auto const* evmDialect = dynamic_cast<EVMDialect const*>(&_dialect))
		for (std::string_view builtinName: evmDialect->builtinFunctionNames())
		{
			std::optional<BuiltinHandle> handle = evmDialect->findBuiltin(builtinName);
			yulAssert(handle.has_value(), "");
			output.push_back(wireBuiltinFunction(*handle, evmDialect->builtin(*handle), _stringTable));
		}

	return output;
}

rust_ffi::WireObjectContext wireObjectContext(Object const& _object, StringTable& _stringTable)
{
	rust_ffi::WireObjectContext context;
	context.object_name_id = _stringTable.id(_object.name);
	Object::Structure structure = _object.summarizeStructure();
	context.object_paths = wireStringIDs(structure.objectPaths, _stringTable);
	context.data_paths = wireStringIDs(structure.dataPaths, _stringTable);
	return context;
}

::rust::Vec<std::uint64_t> wireReservedIdentifiers(
	std::set<YulName> const& _reservedIdentifiers,
	StringTable& _stringTable
)
{
	::rust::Vec<std::uint64_t> output;
	for (YulName const& identifier: _reservedIdentifiers)
		output.push_back(_stringTable.id(identifier.str()));
	return output;
}

rust_ffi::WireYulOptimizerSettings wireSettings(
	bool _optimizeStackAllocation,
	std::string_view _optimisationSequence,
	std::string_view _optimisationCleanupSequence,
	std::optional<size_t> _expectedExecutionsPerDeployment,
	StringTable& _stringTable
)
{
	rust_ffi::WireYulOptimizerSettings settings;
	settings.optimize_stack_allocation = _optimizeStackAllocation;
	settings.optimization_sequence_id = _stringTable.id(_optimisationSequence);
	settings.cleanup_sequence_id = _stringTable.id(_optimisationCleanupSequence);
	settings.has_expected_executions_per_deployment = _expectedExecutionsPerDeployment.has_value();
	settings.expected_executions_per_deployment =
		_expectedExecutionsPerDeployment ?
		static_cast<std::uint64_t>(*_expectedExecutionsPerDeployment) :
		0;
	settings.creation = !_expectedExecutionsPerDeployment.has_value();
	return settings;
}

} // namespace

RustYulOptimizerResult solidity::yul::optimizeYulWithRust(
	Block const& _ast,
	Dialect const& _dialect,
	Object const& _object,
	bool _optimizeStackAllocation,
	std::string_view _optimisationSequence,
	std::string_view _optimisationCleanupSequence,
	std::optional<size_t> _expectedExecutionsPerDeployment,
	std::set<YulName> const& _externallyUsedIdentifiers
)
{
	try
	{
		StringTable stringTable;
		DebugDataTable debugDataTable;
		BlockEncoder encoder{stringTable, debugDataTable};
		std::uint64_t rootBlockID = encoder.block(_ast);

		rust_ffi::WireYulOptimizerRequest request;
		request.settings = wireSettings(
			_optimizeStackAllocation,
			_optimisationSequence,
			_optimisationCleanupSequence,
			_expectedExecutionsPerDeployment,
			stringTable
		);
		request.dialect = wireDialect(_dialect);
		request.special_handles = wireSpecialHandles(_dialect);
		request.builtins = wireBuiltins(_dialect, stringTable);
		request.object_context = wireObjectContext(_object, stringTable);
		request.reserved_identifier_ids = wireReservedIdentifiers(_externallyUsedIdentifiers, stringTable);

		WireArena arena = encoder.takeArena();
		request.names = std::move(arena.names);
		request.identifiers = std::move(arena.identifiers);
		request.blocks = std::move(arena.blocks);
		request.statements = std::move(arena.statements);
		request.expressions = std::move(arena.expressions);
		request.cases = std::move(arena.cases);
		request.root_block_id = rootBlockID;
		request.strings = stringTable.takeStrings();

		rust_ffi::OptimizerResult rustResult = rust_ffi::optimize_yul(std::move(request));
		if (!rustResult.ok)
			return errorResult(errorCodeFromWire(rustResult.error_code), std::string(rustResult.error_message));

		BlockDecoder decoder{rustResult, debugDataTable};
		return RustYulOptimizerResult{
			true,
			RustYulOptimizerErrorCode::None,
			{},
			decoder.block(rustResult.root_block_id)
		};
	}
	catch (std::exception const& _error)
	{
		return errorResult(RustYulOptimizerErrorCode::InvalidWire, _error.what());
	}
}

#endif
