{
    // This must not duplicate the `create2` opcode when optimizing the masks away.
    let a := and(create2(0, 0, 0x20, 0), 0xffffffffffffffffffffffffffffffffffffffff)
    let b := and(0xffffffffffffffffffffffffffffffffffffffff, create2(0, 0, 0x20, 0))
    sstore(a, b)
}
// ====
// EVMVersion: >=shanghai
// bytecodeFormat: legacy
// ----
// step: fullSuite
//
// {
//     {
//         let a := shr(96, shl(96, create2(0, 0, 0x20, 0)))
//         sstore(a, shr(96, shl(96, create2(0, 0, 0x20, 0))))
//     }
// }
