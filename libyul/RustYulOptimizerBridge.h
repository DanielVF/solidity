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

#include <libyul/AST.h>
#include <libyul/YulName.h>

#include <cstdint>
#include <optional>
#include <set>
#include <string>
#include <string_view>

namespace solidity::yul
{

class Dialect;
class Object;

#if defined(SOLIDITY_USE_RUST_YUL_OPTIMIZER)

enum class RustYulOptimizerErrorCode: std::uint8_t
{
	None = 0,
	InvalidWire = 1,
	Unknown = 255,
};

struct RustYulOptimizerResult
{
	bool ok = false;
	RustYulOptimizerErrorCode errorCode = RustYulOptimizerErrorCode::Unknown;
	std::string errorMessage;
	Block optimizedBlock;
};

RustYulOptimizerResult optimizeYulWithRust(
	Block const& _ast,
	Dialect const& _dialect,
	Object const& _object,
	bool _optimizeStackAllocation,
	std::string_view _optimisationSequence,
	std::string_view _optimisationCleanupSequence,
	std::optional<size_t> _expectedExecutionsPerDeployment,
	std::set<YulName> const& _externallyUsedIdentifiers
);

#endif

}
