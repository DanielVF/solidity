set(sources
	AbstractAssemblyStack.h
	Assembly.cpp
	Assembly.h
	AssemblyItem.cpp
	AssemblyItem.h
	Ethdebug.cpp
	Ethdebug.h
	EthdebugSchema.cpp
	EthdebugSchema.h
	EVMAssemblyStack.cpp
	EVMAssemblyStack.h
	BlockDeduplicator.cpp
	BlockDeduplicator.h
	CommonSubexpressionEliminator.cpp
	CommonSubexpressionEliminator.h
	ConstantOptimiser.cpp
	ConstantOptimiser.h
	ControlFlowGraph.cpp
	ControlFlowGraph.h
	Disassemble.cpp
	Disassemble.h
	Exceptions.h
	ExpressionClasses.cpp
	ExpressionClasses.h
	GasMeter.cpp
	GasMeter.h
	Inliner.cpp
	Inliner.h
	Instruction.cpp
	Instruction.h
	JumpdestRemover.cpp
	JumpdestRemover.h
	KnownState.cpp
	KnownState.h
	LinkerObject.cpp
	LinkerObject.h
	PathGasMeter.cpp
	PathGasMeter.h
	PeepholeOptimiser.cpp
	PeepholeOptimiser.h
	RustOptimizerBridge.cpp
	RustOptimizerBridge.h
	SemanticInformation.cpp
	SemanticInformation.h
	SimplificationRule.h
	SimplificationRules.cpp
	SimplificationRules.h
	SubAssemblyID.h
)

add_library(evmasm ${sources})
target_link_libraries(evmasm PUBLIC solutil fmt::fmt-header-only)

if (USE_RUST_EVMASM_OPTIMIZER)
	target_link_libraries(evmasm PUBLIC evmasm_optimizer_bridge PRIVATE evmasm_optimizer)
	target_compile_definitions(evmasm PUBLIC SOLIDITY_USE_RUST_EVMASM_OPTIMIZER=1)
endif()
