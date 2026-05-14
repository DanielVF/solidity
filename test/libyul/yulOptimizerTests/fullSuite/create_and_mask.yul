{
    // This must not duplicate the `create` opcode when optimizing the masks away.
    let a := and(create(0, 0, 0x20), 0xffffffffffffffffffffffffffffffffffffffff)
    let b := and(0xffffffffffffffffffffffffffffffffffffffff, create(0, 0, 0x20))
    sstore(a, b)
}
// ====
// EVMVersion: >=istanbul
// bytecodeFormat: legacy
// ----
// step: fullSuite
//
// {
//     {
//         let a := shr(96, shl(96, create(0, 0, 0x20)))
//         sstore(a, shr(96, shl(96, create(0, 0, 0x20))))
//     }
// }
