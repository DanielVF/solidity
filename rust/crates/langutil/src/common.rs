pub fn is_decimal_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

pub fn is_hex_digit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

pub fn is_white_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\t' | b'\r')
}

pub fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte == b'$' || byte.is_ascii_alphabetic()
}

pub fn is_identifier_part(byte: u8) -> bool {
    is_identifier_start(byte) || is_decimal_digit(byte)
}

pub fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
