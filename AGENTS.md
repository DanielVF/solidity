This is the solidity compiler. It is being ported module by module to rust.


To build with the rust modules active:

```
  cmake -S . -B build-rust-all-release -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DTESTS=OFF \
    -DUSE_RUST_EVMASM_OPTIMIZER=ON \
    -DUSE_RUST_YUL_OPTIMIZER=ON \
    -DUSE_RUST_SOLIDITY_PARSER=ON

  cmake --build build-rust-all-release --target solc --parallel "$(nproc)"
```

## When porting a module:

1. Create a new crate that can optionally replace existing functionality. See how the evmasm optimizer crate works as an example.
2. Create stub methods for everthing that will be called into
3. 1 to 1 port c++ methods to the rust crate. Behavior should matche exactly. Do not compile.
4. Ensure rust crate compiles. Make all fixes keeping 1-1 behavior in mind. 
5. Ensure that when compile with the rust crate, that all tests pass. No gaurds or special cases, just 1:1 behavior matching.
