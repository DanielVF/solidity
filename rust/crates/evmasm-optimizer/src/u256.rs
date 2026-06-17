use primitive_types::U256;

pub fn from_be_bytes(bytes: &[u8]) -> Option<U256> {
    if bytes.len() != 32 {
        return None;
    }
    Some(U256::from_big_endian(bytes))
}

pub fn to_be_bytes(value: U256) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    value.to_big_endian(&mut out);
    out
}

pub fn max_value() -> U256 {
    !U256::zero()
}

pub fn number_encoding_size(value: U256) -> usize {
    let bytes = to_be_bytes(value);
    bytes
        .iter()
        .position(|byte| *byte != 0)
        .map_or(0, |index| bytes.len() - index)
}

pub fn wrapping_neg(value: U256) -> U256 {
    (!value).overflowing_add(U256::one()).0
}

pub fn signed_is_negative(value: U256) -> bool {
    value.bit(255)
}

pub fn signed_cmp(a: U256, b: U256) -> std::cmp::Ordering {
    match (signed_is_negative(a), signed_is_negative(b)) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.cmp(&b),
    }
}

pub fn signed_abs(value: U256) -> (bool, U256) {
    if signed_is_negative(value) {
        (true, wrapping_neg(value))
    } else {
        (false, value)
    }
}

pub fn signed_div(a: U256, b: U256) -> U256 {
    if b.is_zero() {
        return U256::zero();
    }
    let (a_neg, a_abs) = signed_abs(a);
    let (b_neg, b_abs) = signed_abs(b);
    let quotient = a_abs / b_abs;
    if a_neg ^ b_neg {
        wrapping_neg(quotient)
    } else {
        quotient
    }
}

pub fn signed_mod(a: U256, b: U256) -> U256 {
    if b.is_zero() {
        return U256::zero();
    }
    let (a_neg, a_abs) = signed_abs(a);
    let (_, b_abs) = signed_abs(b);
    let remainder = a_abs % b_abs;
    if a_neg {
        wrapping_neg(remainder)
    } else {
        remainder
    }
}

pub fn signextend(index: U256, value: U256) -> U256 {
    if index >= U256::from(31u8) {
        return value;
    }
    let byte_index = index.low_u32() as usize;
    let test_bit = byte_index * 8 + 7;
    let mask = (U256::one() << test_bit) - U256::one();
    if value.bit(test_bit) {
        value | !mask
    } else {
        value & mask
    }
}

pub fn byte(index: U256, value: U256) -> U256 {
    if index >= U256::from(32u8) {
        return U256::zero();
    }
    let shift = 8 * (31 - index.low_u32() as usize);
    (value >> shift) & U256::from(0xffu16)
}
