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

#include <libevmasm/Assembly.h>
#include <libevmasm/AssemblyItem.h>

#include <liblangutil/EVMVersion.h>

#include <libsolutil/FixedHash.h>
#include <libsolutil/Numeric.h>

#include <cstdint>
#include <map>
#include <set>
#include <string>

namespace solidity::evmasm
{

#if defined(SOLIDITY_USE_RUST_EVMASM_OPTIMIZER)

enum class RustOptimizerErrorCode: std::uint8_t
{
	None = 0,
	InvalidWire = 1,
	InvalidTag = 2,
	InvalidState = 3,
	Unknown = 255,
};

struct RustOptimizerResult
{
	bool ok = false;
	RustOptimizerErrorCode errorCode = RustOptimizerErrorCode::Unknown;
	std::string errorMessage;
	AssemblyItems optimizedItems;
	std::map<u256, u256> tagReplacements;
	std::map<util::h256, bytes> dataEntries;
};

RustOptimizerResult optimizeAssemblyItemsWithRust(
	AssemblyItems const& _items,
	Assembly::OptimiserSettings const& _settings,
	langutil::EVMVersion _evmVersion,
	bool _creation,
	std::set<size_t> const& _tagsReferencedFromOutside
);

AssemblyItems roundTripAssemblyItemsThroughRustForTesting(
	AssemblyItems const& _items,
	langutil::EVMVersion _evmVersion = langutil::EVMVersion()
);

#endif

}
