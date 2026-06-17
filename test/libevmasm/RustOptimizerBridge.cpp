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

#if defined(SOLIDITY_USE_RUST_EVMASM_OPTIMIZER)

#include <libevmasm/RustOptimizerBridge.h>

#include <liblangutil/DebugData.h>

#include <boost/test/unit_test.hpp>

using namespace solidity;
using namespace solidity::evmasm;
using namespace solidity::langutil;

namespace solidity::frontend::test
{

namespace
{

DebugData::ConstPtr debugData(size_t _index)
{
	auto sourceName = std::make_shared<std::string const>("rust-optimizer-bridge.asm");
	return DebugData::create(
		SourceLocation{static_cast<int>(_index), static_cast<int>(_index + 1), sourceName},
		SourceLocation{static_cast<int>(_index + 2), static_cast<int>(_index + 3), sourceName},
		static_cast<int64_t>(_index)
	);
}

void annotate(AssemblyItem& _item, size_t _index)
{
	_item.setDebugData(debugData(_index));
	_item.m_modifierDepth = _index;
	switch (_index % 3)
	{
	case 1:
		_item.setJumpType(AssemblyItem::JumpType::IntoFunction);
		break;
	case 2:
		_item.setJumpType(AssemblyItem::JumpType::OutOfFunction);
		break;
	default:
		_item.setJumpType(AssemblyItem::JumpType::Ordinary);
		break;
	}
}

AssemblyItems allRoundTripItems()
{
	AssemblyItems items{
		AssemblyItem{Instruction::JUMP},
		AssemblyItem{u256("0x010203040506070809")},
		AssemblyItem{PushTag, u256(17)},
		AssemblyItem{PushSub, u256(3)},
		AssemblyItem{PushSubSize, u256(4)},
		AssemblyItem{PushProgramSize, u256(0)},
		AssemblyItem{Tag, u256(99)},
		AssemblyItem{PushData, u256("0xabcdef")},
		AssemblyItem{PushLibraryAddress, u256("0x11223344556677889900")},
		AssemblyItem{PushDeployTimeAddress, u256("0x99887766554433221100")},
		AssemblyItem{PushImmutable, u256("0x1234567890abcdef")},
		AssemblyItem{AssignImmutable, u256("0xfedcba0987654321")},
		AssemblyItem{bytes{0xde, 0xad, 0xbe, 0xef}, 2, 1},
	};
	items[4].setPushedValue(u256("0x1020304050"));
	items[11].setImmutableOccurrences(3);

	for (size_t i = 0; i < items.size(); ++i)
		annotate(items[i], i);

	return items;
}

void checkItemEqual(AssemblyItem const& _expected, AssemblyItem const& _actual)
{
	BOOST_CHECK(_expected == _actual);
	BOOST_CHECK(_expected.debugData() == _actual.debugData());
	BOOST_CHECK(_expected.getJumpType() == _actual.getJumpType());
	BOOST_CHECK_EQUAL(_expected.m_modifierDepth, _actual.m_modifierDepth);

	if (_expected.pushedValue())
	{
		BOOST_REQUIRE(_actual.pushedValue());
		BOOST_CHECK_EQUAL(*_expected.pushedValue(), *_actual.pushedValue());
	}
	else
		BOOST_CHECK(!_actual.pushedValue());

	BOOST_CHECK(_expected.immutableOccurrences() == _actual.immutableOccurrences());

	if (_expected.type() == VerbatimBytecode)
	{
		BOOST_CHECK_EQUAL(_expected.arguments(), _actual.arguments());
		BOOST_CHECK_EQUAL(_expected.returnValues(), _actual.returnValues());
		BOOST_CHECK_EQUAL_COLLECTIONS(
			_expected.verbatimData().begin(),
			_expected.verbatimData().end(),
			_actual.verbatimData().begin(),
			_actual.verbatimData().end()
		);
	}
}

}

BOOST_AUTO_TEST_SUITE(RustOptimizerBridge)

BOOST_AUTO_TEST_CASE(round_trips_assembly_item_wire_fields)
{
	AssemblyItems input = allRoundTripItems();
	AssemblyItems output = roundTripAssemblyItemsThroughRustForTesting(input);

	BOOST_REQUIRE_EQUAL(input.size(), output.size());
	for (size_t i = 0; i < input.size(); ++i)
		checkItemEqual(input[i], output[i]);
}

BOOST_AUTO_TEST_SUITE_END()

}

#endif
