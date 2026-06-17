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

#include <libevmasm/RustOptimizerBridge.h>

#if defined(SOLIDITY_USE_RUST_EVMASM_OPTIMIZER)

#include <evmasm_optimizer_bridge/bridge.h>

#include <libsolutil/Assertions.h>

#include <algorithm>
#include <limits>
#include <memory>
#include <optional>
#include <vector>

using namespace solidity;
using namespace solidity::evmasm;

namespace
{

namespace rust_ffi = solidity::evmasm::rust;

static_assert(sizeof(size_t) <= sizeof(std::uint64_t));

constexpr std::uint8_t c_wireKindUndefined = 0;
constexpr std::uint8_t c_wireKindOperation = 1;
constexpr std::uint8_t c_wireKindVerbatimBytecode = 13;

constexpr std::uint8_t c_wireJumpOrdinary = 0;
constexpr std::uint8_t c_wireJumpIntoFunction = 1;
constexpr std::uint8_t c_wireJumpOutOfFunction = 2;

RustOptimizerErrorCode errorCodeFromWire(std::uint8_t _code)
{
	switch (_code)
	{
	case 0:
		return RustOptimizerErrorCode::None;
	case 1:
		return RustOptimizerErrorCode::InvalidWire;
	case 2:
		return RustOptimizerErrorCode::InvalidTag;
	case 3:
		return RustOptimizerErrorCode::InvalidState;
	default:
		return RustOptimizerErrorCode::Unknown;
	}
}

RustOptimizerResult errorResult(RustOptimizerErrorCode _code, std::string _message)
{
	return RustOptimizerResult{
		false,
		_code,
		std::move(_message),
		{},
		{},
		{},
	};
}

std::uint8_t wireKind(AssemblyItemType _type)
{
	return static_cast<std::uint8_t>(_type);
}

AssemblyItemType assemblyItemType(std::uint8_t _type)
{
	return static_cast<AssemblyItemType>(_type);
}

std::uint8_t wireJumpType(AssemblyItem::JumpType _jumpType)
{
	switch (_jumpType)
	{
	case AssemblyItem::JumpType::Ordinary:
		return c_wireJumpOrdinary;
	case AssemblyItem::JumpType::IntoFunction:
		return c_wireJumpIntoFunction;
	case AssemblyItem::JumpType::OutOfFunction:
		return c_wireJumpOutOfFunction;
	}
	util::unreachable();
}

std::optional<AssemblyItem::JumpType> assemblyJumpType(std::uint8_t _jumpType)
{
	switch (_jumpType)
	{
	case c_wireJumpOrdinary:
		return AssemblyItem::JumpType::Ordinary;
	case c_wireJumpIntoFunction:
		return AssemblyItem::JumpType::IntoFunction;
	case c_wireJumpOutOfFunction:
		return AssemblyItem::JumpType::OutOfFunction;
	default:
		return std::nullopt;
	}
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

::rust::Vec<std::uint8_t> wireU256(u256 const& _value)
{
	return rustBytes(toBigEndian(_value));
}

std::optional<u256> cppU256(::rust::Vec<std::uint8_t> const& _bytes)
{
	if (_bytes.size() != 32)
		return std::nullopt;
	return fromBigEndian<u256>(cppBytes(_bytes));
}

rust_ffi::WireOptimizerSettings wireSettings(Assembly::OptimiserSettings const& _settings)
{
	return rust_ffi::WireOptimizerSettings{
		_settings.runInliner,
		_settings.runJumpdestRemover,
		_settings.runPeephole,
		_settings.runDeduplicate,
		_settings.runCSE,
		_settings.runConstantOptimiser,
		static_cast<std::uint64_t>(_settings.expectedExecutionsPerDeployment),
	};
}

rust_ffi::WireEvmVersion wireEVMVersion(langutil::EVMVersion _evmVersion)
{
	auto versions = langutil::EVMVersion::allVersions();
	auto it = std::find(versions.begin(), versions.end(), _evmVersion);
	solAssert(it != versions.end());
	return rust_ffi::WireEvmVersion{static_cast<std::uint16_t>(it - versions.begin())};
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

	std::optional<langutil::DebugData::ConstPtr> debugData(std::uint64_t _id) const
	{
		if (_id > std::numeric_limits<size_t>::max() || static_cast<size_t>(_id) >= m_debugDataByID.size())
			return std::nullopt;
		return m_debugDataByID[static_cast<size_t>(_id)];
	}

private:
	std::map<langutil::DebugData::ConstPtr, size_t, std::owner_less<langutil::DebugData::ConstPtr>> m_ids;
	std::vector<langutil::DebugData::ConstPtr> m_debugDataByID;
};

rust_ffi::WireAssemblyItem wireItem(AssemblyItem const& _item, DebugDataTable& _debugDataTable)
{
	rust_ffi::WireAssemblyItem wire;
	wire.kind = wireKind(_item.type());
	wire.opcode = _item.hasInstruction() ? static_cast<std::uint8_t>(_item.instruction()) : 0;
	wire.data = _item.type() != Operation && _item.type() != VerbatimBytecode ? wireU256(_item.data()) : ::rust::Vec<std::uint8_t>{};
	wire.verbatim_data = _item.type() == VerbatimBytecode ? rustBytes(_item.verbatimData()) : ::rust::Vec<std::uint8_t>{};
	wire.verbatim_arguments = _item.type() == VerbatimBytecode ? static_cast<std::uint64_t>(_item.arguments()) : 0;
	wire.verbatim_return_values = _item.type() == VerbatimBytecode ? static_cast<std::uint64_t>(_item.returnValues()) : 0;
	wire.jump_type = wireJumpType(_item.getJumpType());
	wire.modifier_depth = static_cast<std::uint64_t>(_item.m_modifierDepth);
	wire.debug_data_id = _debugDataTable.id(_item.debugData());
	wire.has_pushed_value = _item.pushedValue() != nullptr;
	wire.pushed_value = _item.pushedValue() ? wireU256(*_item.pushedValue()) : ::rust::Vec<std::uint8_t>{};
	wire.has_immutable_occurrences = _item.immutableOccurrences().has_value();
	wire.immutable_occurrences = _item.immutableOccurrences() ? static_cast<std::uint64_t>(*_item.immutableOccurrences()) : 0;
	return wire;
}

std::optional<AssemblyItem> assemblyItem(rust_ffi::WireAssemblyItem const& _wire, DebugDataTable const& _debugDataTable, std::string& _errorMessage)
{
	auto debugData = _debugDataTable.debugData(_wire.debug_data_id);
	if (!debugData)
	{
		_errorMessage = "Rust optimizer returned an unknown debug data ID.";
		return std::nullopt;
	}

	auto jumpType = assemblyJumpType(_wire.jump_type);
	if (!jumpType)
	{
		_errorMessage = "Rust optimizer returned an invalid jump type.";
		return std::nullopt;
	}

	AssemblyItem item{UndefinedItem};
	if (_wire.kind == c_wireKindOperation)
	{
		auto instruction = static_cast<Instruction>(_wire.opcode);
		if (!isValidInstruction(instruction))
		{
			_errorMessage = "Rust optimizer returned an invalid opcode.";
			return std::nullopt;
		}
		item = AssemblyItem{instruction, *debugData};
	}
	else if (_wire.kind == c_wireKindVerbatimBytecode)
		item = AssemblyItem{
			cppBytes(_wire.verbatim_data),
			static_cast<size_t>(_wire.verbatim_arguments),
			static_cast<size_t>(_wire.verbatim_return_values),
		};
	else if (_wire.kind > c_wireKindUndefined && _wire.kind < c_wireKindVerbatimBytecode)
	{
		auto data = cppU256(_wire.data);
		if (!data)
		{
			_errorMessage = "Rust optimizer returned an invalid integer width.";
			return std::nullopt;
		}
		item = AssemblyItem{assemblyItemType(_wire.kind), *data, *debugData};
	}
	else
	{
		_errorMessage = "Rust optimizer returned an invalid assembly item kind.";
		return std::nullopt;
	}

	if (_wire.kind == c_wireKindVerbatimBytecode)
		item.setDebugData(*debugData);
	item.setJumpType(*jumpType);
	item.m_modifierDepth = static_cast<size_t>(_wire.modifier_depth);
	if (_wire.has_pushed_value)
	{
		auto pushedValue = cppU256(_wire.pushed_value);
		if (!pushedValue)
		{
			_errorMessage = "Rust optimizer returned an invalid pushed value width.";
			return std::nullopt;
		}
		item.setPushedValue(*pushedValue);
	}
	if (_wire.has_immutable_occurrences)
		item.setImmutableOccurrences(static_cast<size_t>(_wire.immutable_occurrences));

	return item;
}

::rust::Vec<rust_ffi::WireAssemblyItem> wireItems(AssemblyItems const& _items, DebugDataTable& _debugDataTable)
{
	::rust::Vec<rust_ffi::WireAssemblyItem> output;
	for (AssemblyItem const& item: _items)
		output.push_back(wireItem(item, _debugDataTable));
	return output;
}

::rust::Vec<std::uint64_t> wireTags(std::set<size_t> const& _tags)
{
	::rust::Vec<std::uint64_t> output;
	for (size_t tag: _tags)
		output.push_back(static_cast<std::uint64_t>(tag));
	return output;
}

} // namespace

RustOptimizerResult solidity::evmasm::optimizeAssemblyItemsWithRust(
	AssemblyItems const& _items,
	Assembly::OptimiserSettings const& _settings,
	langutil::EVMVersion _evmVersion,
	bool _creation,
	std::set<size_t> const& _tagsReferencedFromOutside
)
{
	DebugDataTable debugDataTable;
	auto rustResult = rust_ffi::optimize_assembly_items(
		wireItems(_items, debugDataTable),
		wireSettings(_settings),
		wireEVMVersion(_evmVersion),
		_creation,
		wireTags(_tagsReferencedFromOutside)
	);

	if (!rustResult.ok)
		return errorResult(errorCodeFromWire(rustResult.error_code), std::string(rustResult.error_message));

	AssemblyItems optimizedItems;
	for (rust_ffi::WireAssemblyItem const& wireOptimizedItem: rustResult.optimized_items)
	{
		std::string errorMessage;
		auto item = assemblyItem(wireOptimizedItem, debugDataTable, errorMessage);
		if (!item)
			return errorResult(RustOptimizerErrorCode::InvalidWire, std::move(errorMessage));
		optimizedItems.push_back(std::move(*item));
	}

	std::map<u256, u256> tagReplacements;
	for (rust_ffi::WireTagReplacement const& wireReplacement: rustResult.tag_replacements)
	{
		auto from = cppU256(wireReplacement.from);
		auto to = cppU256(wireReplacement.to);
		if (!from || !to)
			return errorResult(RustOptimizerErrorCode::InvalidWire, "Rust optimizer returned an invalid tag replacement width.");
		tagReplacements[*from] = *to;
	}

	std::map<util::h256, bytes> dataEntries;
	for (rust_ffi::WireDataEntry const& wireDataEntry: rustResult.data_entries)
	{
		if (wireDataEntry.hash.size() != 32)
			return errorResult(RustOptimizerErrorCode::InvalidWire, "Rust optimizer returned an invalid data hash width.");
		dataEntries[util::h256(cppBytes(wireDataEntry.hash))] = cppBytes(wireDataEntry.data);
	}

	return RustOptimizerResult{
		true,
		RustOptimizerErrorCode::None,
		{},
		std::move(optimizedItems),
		std::move(tagReplacements),
		std::move(dataEntries),
	};
}

AssemblyItems solidity::evmasm::roundTripAssemblyItemsThroughRustForTesting(
	AssemblyItems const& _items,
	langutil::EVMVersion _evmVersion
)
{
	auto result = optimizeAssemblyItemsWithRust(_items, Assembly::OptimiserSettings{}, _evmVersion, false, {});
	solAssert(result.ok, result.errorMessage);
	return std::move(result.optimizedItems);
}

#endif
