{
  function cleanup(value) -> cleaned, small {
    cleaned := and(value, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
    small := and(value, 0xffff)
  }
}
// ====
// EVMVersion: >=constantinople
// ----
// step: constantOptimiser
//
// {
//     function cleanup(value) -> cleaned, small
//     {
//         cleaned := shr(8, shl(8, value))
//         small := and(value, 0xffff)
//     }
// }
