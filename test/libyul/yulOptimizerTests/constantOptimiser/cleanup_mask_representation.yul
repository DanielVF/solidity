{
  let large := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
  let small := 0x1ffff
}
// ====
// EVMVersion: >=constantinople
// ----
// step: constantOptimiser
//
// {
//     let large := shr(8, not(0))
//     let small := 0x1ffff
// }
