pub const TOKEN_EOS: u32 = 0;
pub const TOKEN_LPAREN: u32 = 1;
pub const TOKEN_RPAREN: u32 = 2;
pub const TOKEN_LBRACK: u32 = 3;
pub const TOKEN_RBRACK: u32 = 4;
pub const TOKEN_LBRACE: u32 = 5;
pub const TOKEN_RBRACE: u32 = 6;
pub const TOKEN_COLON: u32 = 7;
pub const TOKEN_SEMICOLON: u32 = 8;
pub const TOKEN_PERIOD: u32 = 9;
pub const TOKEN_CONDITIONAL: u32 = 10;
pub const TOKEN_DOUBLE_ARROW: u32 = 11;
pub const TOKEN_RIGHT_ARROW: u32 = 12;
pub const TOKEN_ASSIGN: u32 = 13;
pub const TOKEN_ASSIGN_BIT_OR: u32 = 14;
pub const TOKEN_ASSIGN_BIT_XOR: u32 = 15;
pub const TOKEN_ASSIGN_BIT_AND: u32 = 16;
pub const TOKEN_ASSIGN_SHL: u32 = 17;
pub const TOKEN_ASSIGN_SAR: u32 = 18;
pub const TOKEN_ASSIGN_SHR: u32 = 19;
pub const TOKEN_ASSIGN_ADD: u32 = 20;
pub const TOKEN_ASSIGN_SUB: u32 = 21;
pub const TOKEN_ASSIGN_MUL: u32 = 22;
pub const TOKEN_ASSIGN_DIV: u32 = 23;
pub const TOKEN_ASSIGN_MOD: u32 = 24;
pub const TOKEN_COMMA: u32 = 25;
pub const TOKEN_OR: u32 = 26;
pub const TOKEN_AND: u32 = 27;
pub const TOKEN_BIT_OR: u32 = 28;
pub const TOKEN_BIT_XOR: u32 = 29;
pub const TOKEN_BIT_AND: u32 = 30;
pub const TOKEN_SHL: u32 = 31;
pub const TOKEN_SAR: u32 = 32;
pub const TOKEN_SHR: u32 = 33;
pub const TOKEN_ADD: u32 = 34;
pub const TOKEN_SUB: u32 = 35;
pub const TOKEN_MUL: u32 = 36;
pub const TOKEN_DIV: u32 = 37;
pub const TOKEN_MOD: u32 = 38;
pub const TOKEN_EXP: u32 = 39;
pub const TOKEN_EQUAL: u32 = 40;
pub const TOKEN_NOT_EQUAL: u32 = 41;
pub const TOKEN_LESS_THAN: u32 = 42;
pub const TOKEN_GREATER_THAN: u32 = 43;
pub const TOKEN_LESS_THAN_OR_EQUAL: u32 = 44;
pub const TOKEN_GREATER_THAN_OR_EQUAL: u32 = 45;
pub const TOKEN_NOT: u32 = 46;
pub const TOKEN_BIT_NOT: u32 = 47;
pub const TOKEN_INC: u32 = 48;
pub const TOKEN_DEC: u32 = 49;
pub const TOKEN_DELETE: u32 = 50;
pub const TOKEN_ASSEMBLY_ASSIGN: u32 = 51;
pub const TOKEN_ABSTRACT: u32 = 52;
pub const TOKEN_ANONYMOUS: u32 = 53;
pub const TOKEN_AS: u32 = 54;
pub const TOKEN_ASSEMBLY: u32 = 55;
pub const TOKEN_BREAK: u32 = 56;
pub const TOKEN_CATCH: u32 = 57;
pub const TOKEN_CONSTANT: u32 = 58;
pub const TOKEN_CONSTRUCTOR: u32 = 59;
pub const TOKEN_CONTINUE: u32 = 60;
pub const TOKEN_CONTRACT: u32 = 61;
pub const TOKEN_DO: u32 = 62;
pub const TOKEN_ELSE: u32 = 63;
pub const TOKEN_ENUM: u32 = 64;
pub const TOKEN_EMIT: u32 = 65;
pub const TOKEN_EVENT: u32 = 66;
pub const TOKEN_EXTERNAL: u32 = 67;
pub const TOKEN_FALLBACK: u32 = 68;
pub const TOKEN_FOR: u32 = 69;
pub const TOKEN_FUNCTION: u32 = 70;
pub const TOKEN_HEX: u32 = 71;
pub const TOKEN_IF: u32 = 72;
pub const TOKEN_INDEXED: u32 = 73;
pub const TOKEN_INTERFACE: u32 = 74;
pub const TOKEN_INTERNAL: u32 = 75;
pub const TOKEN_IMMUTABLE: u32 = 76;
pub const TOKEN_IMPORT: u32 = 77;
pub const TOKEN_IS: u32 = 78;
pub const TOKEN_LIBRARY: u32 = 79;
pub const TOKEN_MAPPING: u32 = 80;
pub const TOKEN_MEMORY: u32 = 81;
pub const TOKEN_MODIFIER: u32 = 82;
pub const TOKEN_NEW: u32 = 83;
pub const TOKEN_OVERRIDE: u32 = 84;
pub const TOKEN_PAYABLE: u32 = 85;
pub const TOKEN_PUBLIC: u32 = 86;
pub const TOKEN_PRAGMA: u32 = 87;
pub const TOKEN_PRIVATE: u32 = 88;
pub const TOKEN_PURE: u32 = 89;
pub const TOKEN_RECEIVE: u32 = 90;
pub const TOKEN_RETURN: u32 = 91;
pub const TOKEN_RETURNS: u32 = 92;
pub const TOKEN_STORAGE: u32 = 93;
pub const TOKEN_CALLDATA: u32 = 94;
pub const TOKEN_STRUCT: u32 = 95;
pub const TOKEN_THROW: u32 = 96;
pub const TOKEN_TRY: u32 = 97;
pub const TOKEN_TYPE: u32 = 98;
pub const TOKEN_UNCHECKED: u32 = 99;
pub const TOKEN_UNICODE: u32 = 100;
pub const TOKEN_USING: u32 = 101;
pub const TOKEN_VIEW: u32 = 102;
pub const TOKEN_VIRTUAL: u32 = 103;
pub const TOKEN_WHILE: u32 = 104;
pub const TOKEN_SUB_WEI: u32 = 105;
pub const TOKEN_SUB_GWEI: u32 = 106;
pub const TOKEN_SUB_ETHER: u32 = 107;
pub const TOKEN_SUB_SECOND: u32 = 108;
pub const TOKEN_SUB_MINUTE: u32 = 109;
pub const TOKEN_SUB_HOUR: u32 = 110;
pub const TOKEN_SUB_DAY: u32 = 111;
pub const TOKEN_SUB_WEEK: u32 = 112;
pub const TOKEN_SUB_YEAR: u32 = 113;
pub const TOKEN_INT: u32 = 114;
pub const TOKEN_UINT: u32 = 115;
pub const TOKEN_BYTES: u32 = 116;
pub const TOKEN_STRING: u32 = 117;
pub const TOKEN_ADDRESS: u32 = 118;
pub const TOKEN_BOOL: u32 = 119;
pub const TOKEN_FIXED: u32 = 120;
pub const TOKEN_UFIXED: u32 = 121;
pub const TOKEN_INT_M: u32 = 122;
pub const TOKEN_UINT_M: u32 = 123;
pub const TOKEN_BYTES_M: u32 = 124;
pub const TOKEN_FIXED_MXN: u32 = 125;
pub const TOKEN_UFIXED_MXN: u32 = 126;
pub const TOKEN_TYPES_END: u32 = 127;
pub const TOKEN_TRUE_LITERAL: u32 = 128;
pub const TOKEN_FALSE_LITERAL: u32 = 129;
pub const TOKEN_NUMBER: u32 = 130;
pub const TOKEN_STRING_LITERAL: u32 = 131;
pub const TOKEN_UNICODE_STRING_LITERAL: u32 = 132;
pub const TOKEN_HEX_STRING_LITERAL: u32 = 133;
pub const TOKEN_COMMENT_LITERAL: u32 = 134;
pub const TOKEN_IDENTIFIER: u32 = 135;
pub const TOKEN_AFTER: u32 = 136;
pub const TOKEN_ALIAS: u32 = 137;
pub const TOKEN_APPLY: u32 = 138;
pub const TOKEN_AUTO: u32 = 139;
pub const TOKEN_BYTE: u32 = 140;
pub const TOKEN_CASE: u32 = 141;
pub const TOKEN_COPY_OF: u32 = 142;
pub const TOKEN_DEFAULT: u32 = 143;
pub const TOKEN_DEFINE: u32 = 144;
pub const TOKEN_FINAL: u32 = 145;
pub const TOKEN_IMPLEMENTS: u32 = 146;
pub const TOKEN_IN: u32 = 147;
pub const TOKEN_INLINE: u32 = 148;
pub const TOKEN_LET: u32 = 149;
pub const TOKEN_MACRO: u32 = 150;
pub const TOKEN_MATCH: u32 = 151;
pub const TOKEN_MUTABLE: u32 = 152;
pub const TOKEN_NULL_LITERAL: u32 = 153;
pub const TOKEN_OF: u32 = 154;
pub const TOKEN_PARTIAL: u32 = 155;
pub const TOKEN_PROMISE: u32 = 156;
pub const TOKEN_REFERENCE: u32 = 157;
pub const TOKEN_RELOCATABLE: u32 = 158;
pub const TOKEN_SEALED: u32 = 159;
pub const TOKEN_SIZEOF: u32 = 160;
pub const TOKEN_STATIC: u32 = 161;
pub const TOKEN_SUPPORTS: u32 = 162;
pub const TOKEN_SWITCH: u32 = 163;
pub const TOKEN_TYPEDEF: u32 = 164;
pub const TOKEN_TYPEOF: u32 = 165;
pub const TOKEN_VAR: u32 = 166;
pub const TOKEN_LEAVE: u32 = 167;
pub const TOKEN_NON_EXPERIMENTAL_END: u32 = 168;
pub const TOKEN_CLASS: u32 = 169;
pub const TOKEN_INSTANTIATION: u32 = 170;
pub const TOKEN_INTEGER: u32 = 171;
pub const TOKEN_ITSELF: u32 = 172;
pub const TOKEN_STATIC_ASSERT: u32 = 173;
pub const TOKEN_BUILTIN: u32 = 174;
pub const TOKEN_FORALL: u32 = 175;
pub const TOKEN_EXPERIMENTAL_END: u32 = 176;
pub const TOKEN_ILLEGAL: u32 = 177;
pub const TOKEN_WHITESPACE: u32 = 178;
pub const TOKEN_NUM_TOKENS: u32 = 179;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementaryTypeNameToken {
    token: u32,
    first_number: u32,
    second_number: u32,
}

impl ElementaryTypeNameToken {
    pub fn new(token: u32, first_number: u32, second_number: u32) -> Self {
        elementary_type_name_token_assert_details(token, first_number, second_number);
        Self {
            token,
            first_number,
            second_number,
        }
    }

    pub fn first_number(&self) -> u32 {
        self.first_number
    }

    pub fn second_number(&self) -> u32 {
        self.second_number
    }

    pub fn token(&self) -> u32 {
        self.token
    }

    pub fn to_string(self, token_value: bool) -> String {
        elementary_type_name_token_to_string(
            self.token,
            self.first_number,
            self.second_number,
            token_value,
        )
    }
}

pub fn count() -> usize {
    TOKEN_NUM_TOKENS as usize
}

pub fn is_elementary_type_name(token: u32) -> bool {
    (TOKEN_INT..TOKEN_TYPES_END).contains(&token)
}

pub fn is_assignment_op(token: u32) -> bool {
    (TOKEN_ASSIGN..=TOKEN_ASSIGN_MOD).contains(&token)
}

pub fn is_binary_op(token: u32) -> bool {
    (TOKEN_COMMA..=TOKEN_EXP).contains(&token)
}

pub fn is_commutative_op(token: u32) -> bool {
    matches!(
        token,
        TOKEN_BIT_OR
            | TOKEN_BIT_XOR
            | TOKEN_BIT_AND
            | TOKEN_ADD
            | TOKEN_MUL
            | TOKEN_EQUAL
            | TOKEN_NOT_EQUAL
    )
}

pub fn is_arithmetic_op(token: u32) -> bool {
    (TOKEN_ADD..=TOKEN_EXP).contains(&token)
}

pub fn is_compare_op(token: u32) -> bool {
    (TOKEN_EQUAL..=TOKEN_GREATER_THAN_OR_EQUAL).contains(&token)
}

pub fn is_bit_op(token: u32) -> bool {
    (TOKEN_BIT_OR..=TOKEN_BIT_AND).contains(&token) || token == TOKEN_BIT_NOT
}

pub fn is_boolean_op(token: u32) -> bool {
    (TOKEN_OR..=TOKEN_AND).contains(&token) || token == TOKEN_NOT
}

pub fn is_unary_op(token: u32) -> bool {
    (TOKEN_NOT..=TOKEN_DELETE).contains(&token) || token == TOKEN_SUB
}

pub fn is_count_op(token: u32) -> bool {
    token == TOKEN_INC || token == TOKEN_DEC
}

pub fn is_shift_op(token: u32) -> bool {
    (TOKEN_SHL..=TOKEN_SHR).contains(&token)
}

pub fn is_variable_visibility_specifier(token: u32) -> bool {
    token == TOKEN_PUBLIC || token == TOKEN_PRIVATE || token == TOKEN_INTERNAL
}

pub fn is_visibility_specifier(token: u32) -> bool {
    is_variable_visibility_specifier(token) || token == TOKEN_EXTERNAL
}

pub fn is_builtin_type_class_name(token: u32) -> bool {
    token == TOKEN_INTEGER
        || ((TOKEN_COMMA..=TOKEN_EXP).contains(&token) && token != TOKEN_COMMA)
        || (TOKEN_EQUAL..=TOKEN_GREATER_THAN_OR_EQUAL).contains(&token)
        || (TOKEN_NOT..=TOKEN_DELETE).contains(&token)
        || token == TOKEN_SUB
        || ((TOKEN_ASSIGN..=TOKEN_ASSIGN_MOD).contains(&token) && token != TOKEN_ASSIGN)
}

pub fn is_experimental_solidity_keyword(token: u32) -> bool {
    matches!(
        token,
        TOKEN_ASSEMBLY
            | TOKEN_CONTRACT
            | TOKEN_EXTERNAL
            | TOKEN_FALLBACK
            | TOKEN_PRAGMA
            | TOKEN_IMPORT
            | TOKEN_AS
            | TOKEN_FUNCTION
            | TOKEN_LET
            | TOKEN_RETURN
            | TOKEN_TYPE
            | TOKEN_IF
            | TOKEN_ELSE
            | TOKEN_DO
            | TOKEN_WHILE
            | TOKEN_FOR
            | TOKEN_CONTINUE
            | TOKEN_BREAK
    ) || (token > TOKEN_NON_EXPERIMENTAL_END && token < TOKEN_EXPERIMENTAL_END)
}

pub fn is_experimental_solidity_only_keyword(token: u32) -> bool {
    token > TOKEN_NON_EXPERIMENTAL_END && token < TOKEN_EXPERIMENTAL_END
}

pub fn assignment_to_binary_op(token: u32) -> u32 {
    assert!(is_assignment_op(token) && token != TOKEN_ASSIGN);
    token + (TOKEN_BIT_OR - TOKEN_ASSIGN_BIT_OR)
}

pub fn is_state_mutability_specifier(token: u32) -> bool {
    token == TOKEN_PURE || token == TOKEN_VIEW || token == TOKEN_PAYABLE
}

pub fn is_ether_subdenomination(token: u32) -> bool {
    (TOKEN_SUB_WEI..=TOKEN_SUB_ETHER).contains(&token)
}

pub fn is_time_subdenomination(token: u32) -> bool {
    matches!(
        token,
        TOKEN_SUB_SECOND
            | TOKEN_SUB_MINUTE
            | TOKEN_SUB_HOUR
            | TOKEN_SUB_DAY
            | TOKEN_SUB_WEEK
            | TOKEN_SUB_YEAR
    )
}

pub fn is_reserved_keyword(token: u32) -> bool {
    (TOKEN_AFTER..=TOKEN_VAR).contains(&token)
}

pub fn is_yul_keyword_token(token: u32) -> bool {
    matches!(
        token,
        TOKEN_FUNCTION
            | TOKEN_LET
            | TOKEN_IF
            | TOKEN_SWITCH
            | TOKEN_CASE
            | TOKEN_DEFAULT
            | TOKEN_FOR
            | TOKEN_BREAK
            | TOKEN_CONTINUE
            | TOKEN_LEAVE
            | TOKEN_TRUE_LITERAL
            | TOKEN_FALSE_LITERAL
            | TOKEN_HEX_STRING_LITERAL
            | TOKEN_HEX
    )
}

pub fn is_yul_keyword(literal: &[u8]) -> bool {
    matches!(
        literal,
        b"function"
            | b"let"
            | b"if"
            | b"switch"
            | b"case"
            | b"default"
            | b"for"
            | b"break"
            | b"continue"
            | b"true"
            | b"false"
            | b"hex"
    )
}

pub fn is_future_yul_keyword(literal: &[u8]) -> bool {
    literal == b"leave"
}

pub fn is_future_solidity_keyword(literal: &[u8]) -> bool {
    literal == b"transient"
        || literal == b"layout"
        || literal == b"at"
        || literal == b"error"
        || literal == b"super"
        || literal == b"this"
        || is_future_yul_keyword(literal)
}

pub fn is_future_yul_reserved_identifier(literal: &[u8]) -> bool {
    literal == b"basefee"
        || literal == b"blobbasefee"
        || literal == b"blobhash"
        || literal == b"clz"
        || literal == b"mcopy"
        || literal == b"memoryguard"
        || literal == b"prevrandao"
        || literal == b"tload"
        || literal == b"tstore"
}

fn keyword_by_name(literal: &[u8]) -> u32 {
    match literal {
        b"delete" => TOKEN_DELETE,
        b"abstract" => TOKEN_ABSTRACT,
        b"anonymous" => TOKEN_ANONYMOUS,
        b"as" => TOKEN_AS,
        b"assembly" => TOKEN_ASSEMBLY,
        b"break" => TOKEN_BREAK,
        b"catch" => TOKEN_CATCH,
        b"constant" => TOKEN_CONSTANT,
        b"constructor" => TOKEN_CONSTRUCTOR,
        b"continue" => TOKEN_CONTINUE,
        b"contract" => TOKEN_CONTRACT,
        b"do" => TOKEN_DO,
        b"else" => TOKEN_ELSE,
        b"enum" => TOKEN_ENUM,
        b"emit" => TOKEN_EMIT,
        b"event" => TOKEN_EVENT,
        b"external" => TOKEN_EXTERNAL,
        b"fallback" => TOKEN_FALLBACK,
        b"for" => TOKEN_FOR,
        b"function" => TOKEN_FUNCTION,
        b"hex" => TOKEN_HEX,
        b"if" => TOKEN_IF,
        b"indexed" => TOKEN_INDEXED,
        b"interface" => TOKEN_INTERFACE,
        b"internal" => TOKEN_INTERNAL,
        b"immutable" => TOKEN_IMMUTABLE,
        b"import" => TOKEN_IMPORT,
        b"is" => TOKEN_IS,
        b"library" => TOKEN_LIBRARY,
        b"mapping" => TOKEN_MAPPING,
        b"memory" => TOKEN_MEMORY,
        b"modifier" => TOKEN_MODIFIER,
        b"new" => TOKEN_NEW,
        b"override" => TOKEN_OVERRIDE,
        b"payable" => TOKEN_PAYABLE,
        b"public" => TOKEN_PUBLIC,
        b"pragma" => TOKEN_PRAGMA,
        b"private" => TOKEN_PRIVATE,
        b"pure" => TOKEN_PURE,
        b"receive" => TOKEN_RECEIVE,
        b"return" => TOKEN_RETURN,
        b"returns" => TOKEN_RETURNS,
        b"storage" => TOKEN_STORAGE,
        b"calldata" => TOKEN_CALLDATA,
        b"struct" => TOKEN_STRUCT,
        b"throw" => TOKEN_THROW,
        b"try" => TOKEN_TRY,
        b"type" => TOKEN_TYPE,
        b"unchecked" => TOKEN_UNCHECKED,
        b"unicode" => TOKEN_UNICODE,
        b"using" => TOKEN_USING,
        b"view" => TOKEN_VIEW,
        b"virtual" => TOKEN_VIRTUAL,
        b"while" => TOKEN_WHILE,
        b"wei" => TOKEN_SUB_WEI,
        b"gwei" => TOKEN_SUB_GWEI,
        b"ether" => TOKEN_SUB_ETHER,
        b"seconds" => TOKEN_SUB_SECOND,
        b"minutes" => TOKEN_SUB_MINUTE,
        b"hours" => TOKEN_SUB_HOUR,
        b"days" => TOKEN_SUB_DAY,
        b"weeks" => TOKEN_SUB_WEEK,
        b"years" => TOKEN_SUB_YEAR,
        b"int" => TOKEN_INT,
        b"uint" => TOKEN_UINT,
        b"bytes" => TOKEN_BYTES,
        b"string" => TOKEN_STRING,
        b"address" => TOKEN_ADDRESS,
        b"bool" => TOKEN_BOOL,
        b"fixed" => TOKEN_FIXED,
        b"ufixed" => TOKEN_UFIXED,
        b"true" => TOKEN_TRUE_LITERAL,
        b"false" => TOKEN_FALSE_LITERAL,
        b"after" => TOKEN_AFTER,
        b"alias" => TOKEN_ALIAS,
        b"apply" => TOKEN_APPLY,
        b"auto" => TOKEN_AUTO,
        b"byte" => TOKEN_BYTE,
        b"case" => TOKEN_CASE,
        b"copyof" => TOKEN_COPY_OF,
        b"default" => TOKEN_DEFAULT,
        b"define" => TOKEN_DEFINE,
        b"final" => TOKEN_FINAL,
        b"implements" => TOKEN_IMPLEMENTS,
        b"in" => TOKEN_IN,
        b"inline" => TOKEN_INLINE,
        b"let" => TOKEN_LET,
        b"macro" => TOKEN_MACRO,
        b"match" => TOKEN_MATCH,
        b"mutable" => TOKEN_MUTABLE,
        b"null" => TOKEN_NULL_LITERAL,
        b"of" => TOKEN_OF,
        b"partial" => TOKEN_PARTIAL,
        b"promise" => TOKEN_PROMISE,
        b"reference" => TOKEN_REFERENCE,
        b"relocatable" => TOKEN_RELOCATABLE,
        b"sealed" => TOKEN_SEALED,
        b"sizeof" => TOKEN_SIZEOF,
        b"static" => TOKEN_STATIC,
        b"supports" => TOKEN_SUPPORTS,
        b"switch" => TOKEN_SWITCH,
        b"typedef" => TOKEN_TYPEDEF,
        b"typeof" => TOKEN_TYPEOF,
        b"var" => TOKEN_VAR,
        b"class" => TOKEN_CLASS,
        b"instantiation" => TOKEN_INSTANTIATION,
        b"Integer" => TOKEN_INTEGER,
        b"itself" => TOKEN_ITSELF,
        b"static_assert" => TOKEN_STATIC_ASSERT,
        b"__builtin" => TOKEN_BUILTIN,
        b"forall" => TOKEN_FORALL,
        _ => TOKEN_IDENTIFIER,
    }
}

pub fn from_identifier_or_keyword(literal: &[u8]) -> (u32, u32, u32) {
    fn parse_size(input: &[u8]) -> i32 {
        if input.is_empty() {
            return -1;
        }

        if input.len() > 1 && input[0] == b'0' {
            return -1;
        }

        let mut ret = 0i32;
        for &byte in input {
            if !byte.is_ascii_digit() {
                return -1;
            }
            if ret >= 256 {
                return -1;
            }
            ret *= 10;
            ret += i32::from(byte - b'0');
        }
        ret
    }

    if let Some(position_m) = literal.iter().position(u8::is_ascii_digit) {
        let base_type = &literal[..position_m];
        let position_x = literal[position_m..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .map(|offset| position_m + offset)
            .unwrap_or(literal.len());
        let m = parse_size(&literal[position_m..position_x]);
        let keyword = keyword_by_name(base_type);

        if keyword == TOKEN_BYTES {
            if 0 < m && m <= 32 && position_x == literal.len() {
                return (TOKEN_BYTES_M, m as u32, 0);
            }
        } else if keyword == TOKEN_UINT || keyword == TOKEN_INT {
            if 0 < m && m <= 256 && m % 8 == 0 && position_x == literal.len() {
                if keyword == TOKEN_UINT {
                    return (TOKEN_UINT_M, m as u32, 0);
                }
                return (TOKEN_INT_M, m as u32, 0);
            }
        } else if (keyword == TOKEN_UFIXED || keyword == TOKEN_FIXED)
            && position_m < position_x
            && position_x < literal.len()
            && literal[position_x] == b'x'
            && literal[position_x + 1..].iter().all(u8::is_ascii_digit)
        {
            let n = parse_size(&literal[position_x + 1..]);
            if (8..=256).contains(&m) && (m as u32).is_multiple_of(8) && (0..=80).contains(&n) {
                if keyword == TOKEN_UFIXED {
                    return (TOKEN_UFIXED_MXN, m as u32, n as u32);
                }
                return (TOKEN_FIXED_MXN, m as u32, n as u32);
            }
        }

        return (TOKEN_IDENTIFIER, 0, 0);
    }

    (keyword_by_name(literal), 0, 0)
}

pub fn to_string(token: u32) -> Option<&'static str> {
    match token {
        TOKEN_EOS => Some("EOS"),
        TOKEN_LPAREN => Some("("),
        TOKEN_RPAREN => Some(")"),
        TOKEN_LBRACK => Some("["),
        TOKEN_RBRACK => Some("]"),
        TOKEN_LBRACE => Some("{"),
        TOKEN_RBRACE => Some("}"),
        TOKEN_COLON => Some(":"),
        TOKEN_SEMICOLON => Some(";"),
        TOKEN_PERIOD => Some("."),
        TOKEN_CONDITIONAL => Some("?"),
        TOKEN_DOUBLE_ARROW => Some("=>"),
        TOKEN_RIGHT_ARROW => Some("->"),
        TOKEN_ASSIGN => Some("="),
        TOKEN_ASSIGN_BIT_OR => Some("|="),
        TOKEN_ASSIGN_BIT_XOR => Some("^="),
        TOKEN_ASSIGN_BIT_AND => Some("&="),
        TOKEN_ASSIGN_SHL => Some("<<="),
        TOKEN_ASSIGN_SAR => Some(">>="),
        TOKEN_ASSIGN_SHR => Some(">>>="),
        TOKEN_ASSIGN_ADD => Some("+="),
        TOKEN_ASSIGN_SUB => Some("-="),
        TOKEN_ASSIGN_MUL => Some("*="),
        TOKEN_ASSIGN_DIV => Some("/="),
        TOKEN_ASSIGN_MOD => Some("%="),
        TOKEN_COMMA => Some(","),
        TOKEN_OR => Some("||"),
        TOKEN_AND => Some("&&"),
        TOKEN_BIT_OR => Some("|"),
        TOKEN_BIT_XOR => Some("^"),
        TOKEN_BIT_AND => Some("&"),
        TOKEN_SHL => Some("<<"),
        TOKEN_SAR => Some(">>"),
        TOKEN_SHR => Some(">>>"),
        TOKEN_ADD => Some("+"),
        TOKEN_SUB => Some("-"),
        TOKEN_MUL => Some("*"),
        TOKEN_DIV => Some("/"),
        TOKEN_MOD => Some("%"),
        TOKEN_EXP => Some("**"),
        TOKEN_EQUAL => Some("=="),
        TOKEN_NOT_EQUAL => Some("!="),
        TOKEN_LESS_THAN => Some("<"),
        TOKEN_GREATER_THAN => Some(">"),
        TOKEN_LESS_THAN_OR_EQUAL => Some("<="),
        TOKEN_GREATER_THAN_OR_EQUAL => Some(">="),
        TOKEN_NOT => Some("!"),
        TOKEN_BIT_NOT => Some("~"),
        TOKEN_INC => Some("++"),
        TOKEN_DEC => Some("--"),
        TOKEN_DELETE => Some("delete"),
        TOKEN_ASSEMBLY_ASSIGN => Some(":="),
        TOKEN_ABSTRACT => Some("abstract"),
        TOKEN_ANONYMOUS => Some("anonymous"),
        TOKEN_AS => Some("as"),
        TOKEN_ASSEMBLY => Some("assembly"),
        TOKEN_BREAK => Some("break"),
        TOKEN_CATCH => Some("catch"),
        TOKEN_CONSTANT => Some("constant"),
        TOKEN_CONSTRUCTOR => Some("constructor"),
        TOKEN_CONTINUE => Some("continue"),
        TOKEN_CONTRACT => Some("contract"),
        TOKEN_DO => Some("do"),
        TOKEN_ELSE => Some("else"),
        TOKEN_ENUM => Some("enum"),
        TOKEN_EMIT => Some("emit"),
        TOKEN_EVENT => Some("event"),
        TOKEN_EXTERNAL => Some("external"),
        TOKEN_FALLBACK => Some("fallback"),
        TOKEN_FOR => Some("for"),
        TOKEN_FUNCTION => Some("function"),
        TOKEN_HEX => Some("hex"),
        TOKEN_IF => Some("if"),
        TOKEN_INDEXED => Some("indexed"),
        TOKEN_INTERFACE => Some("interface"),
        TOKEN_INTERNAL => Some("internal"),
        TOKEN_IMMUTABLE => Some("immutable"),
        TOKEN_IMPORT => Some("import"),
        TOKEN_IS => Some("is"),
        TOKEN_LIBRARY => Some("library"),
        TOKEN_MAPPING => Some("mapping"),
        TOKEN_MEMORY => Some("memory"),
        TOKEN_MODIFIER => Some("modifier"),
        TOKEN_NEW => Some("new"),
        TOKEN_OVERRIDE => Some("override"),
        TOKEN_PAYABLE => Some("payable"),
        TOKEN_PUBLIC => Some("public"),
        TOKEN_PRAGMA => Some("pragma"),
        TOKEN_PRIVATE => Some("private"),
        TOKEN_PURE => Some("pure"),
        TOKEN_RECEIVE => Some("receive"),
        TOKEN_RETURN => Some("return"),
        TOKEN_RETURNS => Some("returns"),
        TOKEN_STORAGE => Some("storage"),
        TOKEN_CALLDATA => Some("calldata"),
        TOKEN_STRUCT => Some("struct"),
        TOKEN_THROW => Some("throw"),
        TOKEN_TRY => Some("try"),
        TOKEN_TYPE => Some("type"),
        TOKEN_UNCHECKED => Some("unchecked"),
        TOKEN_UNICODE => Some("unicode"),
        TOKEN_USING => Some("using"),
        TOKEN_VIEW => Some("view"),
        TOKEN_VIRTUAL => Some("virtual"),
        TOKEN_WHILE => Some("while"),
        TOKEN_SUB_WEI => Some("wei"),
        TOKEN_SUB_GWEI => Some("gwei"),
        TOKEN_SUB_ETHER => Some("ether"),
        TOKEN_SUB_SECOND => Some("seconds"),
        TOKEN_SUB_MINUTE => Some("minutes"),
        TOKEN_SUB_HOUR => Some("hours"),
        TOKEN_SUB_DAY => Some("days"),
        TOKEN_SUB_WEEK => Some("weeks"),
        TOKEN_SUB_YEAR => Some("years"),
        TOKEN_INT => Some("int"),
        TOKEN_UINT => Some("uint"),
        TOKEN_BYTES => Some("bytes"),
        TOKEN_STRING => Some("string"),
        TOKEN_ADDRESS => Some("address"),
        TOKEN_BOOL => Some("bool"),
        TOKEN_FIXED => Some("fixed"),
        TOKEN_UFIXED => Some("ufixed"),
        TOKEN_INT_M => Some("intM"),
        TOKEN_UINT_M => Some("uintM"),
        TOKEN_BYTES_M => Some("bytesM"),
        TOKEN_FIXED_MXN => Some("fixedMxN"),
        TOKEN_UFIXED_MXN => Some("ufixedMxN"),
        TOKEN_TYPES_END => None,
        TOKEN_TRUE_LITERAL => Some("true"),
        TOKEN_FALSE_LITERAL => Some("false"),
        TOKEN_NUMBER => None,
        TOKEN_STRING_LITERAL => None,
        TOKEN_UNICODE_STRING_LITERAL => None,
        TOKEN_HEX_STRING_LITERAL => None,
        TOKEN_COMMENT_LITERAL => None,
        TOKEN_IDENTIFIER => None,
        TOKEN_AFTER => Some("after"),
        TOKEN_ALIAS => Some("alias"),
        TOKEN_APPLY => Some("apply"),
        TOKEN_AUTO => Some("auto"),
        TOKEN_BYTE => Some("byte"),
        TOKEN_CASE => Some("case"),
        TOKEN_COPY_OF => Some("copyof"),
        TOKEN_DEFAULT => Some("default"),
        TOKEN_DEFINE => Some("define"),
        TOKEN_FINAL => Some("final"),
        TOKEN_IMPLEMENTS => Some("implements"),
        TOKEN_IN => Some("in"),
        TOKEN_INLINE => Some("inline"),
        TOKEN_LET => Some("let"),
        TOKEN_MACRO => Some("macro"),
        TOKEN_MATCH => Some("match"),
        TOKEN_MUTABLE => Some("mutable"),
        TOKEN_NULL_LITERAL => Some("null"),
        TOKEN_OF => Some("of"),
        TOKEN_PARTIAL => Some("partial"),
        TOKEN_PROMISE => Some("promise"),
        TOKEN_REFERENCE => Some("reference"),
        TOKEN_RELOCATABLE => Some("relocatable"),
        TOKEN_SEALED => Some("sealed"),
        TOKEN_SIZEOF => Some("sizeof"),
        TOKEN_STATIC => Some("static"),
        TOKEN_SUPPORTS => Some("supports"),
        TOKEN_SWITCH => Some("switch"),
        TOKEN_TYPEDEF => Some("typedef"),
        TOKEN_TYPEOF => Some("typeof"),
        TOKEN_VAR => Some("var"),
        TOKEN_LEAVE => Some("leave"),
        TOKEN_NON_EXPERIMENTAL_END => None,
        TOKEN_CLASS => Some("class"),
        TOKEN_INSTANTIATION => Some("instantiation"),
        TOKEN_INTEGER => Some("Integer"),
        TOKEN_ITSELF => Some("itself"),
        TOKEN_STATIC_ASSERT => Some("static_assert"),
        TOKEN_BUILTIN => Some("__builtin"),
        TOKEN_FORALL => Some("forall"),
        TOKEN_EXPERIMENTAL_END => None,
        TOKEN_ILLEGAL => Some("ILLEGAL"),
        TOKEN_WHITESPACE => None,
        _ => Some(""),
    }
}

pub fn name(token: u32) -> &'static str {
    assert!((token as usize) < count());
    match token {
        TOKEN_EOS => "EOS",
        TOKEN_LPAREN => "LParen",
        TOKEN_RPAREN => "RParen",
        TOKEN_LBRACK => "LBrack",
        TOKEN_RBRACK => "RBrack",
        TOKEN_LBRACE => "LBrace",
        TOKEN_RBRACE => "RBrace",
        TOKEN_COLON => "Colon",
        TOKEN_SEMICOLON => "Semicolon",
        TOKEN_PERIOD => "Period",
        TOKEN_CONDITIONAL => "Conditional",
        TOKEN_DOUBLE_ARROW => "DoubleArrow",
        TOKEN_RIGHT_ARROW => "RightArrow",
        TOKEN_ASSIGN => "Assign",
        TOKEN_ASSIGN_BIT_OR => "AssignBitOr",
        TOKEN_ASSIGN_BIT_XOR => "AssignBitXor",
        TOKEN_ASSIGN_BIT_AND => "AssignBitAnd",
        TOKEN_ASSIGN_SHL => "AssignShl",
        TOKEN_ASSIGN_SAR => "AssignSar",
        TOKEN_ASSIGN_SHR => "AssignShr",
        TOKEN_ASSIGN_ADD => "AssignAdd",
        TOKEN_ASSIGN_SUB => "AssignSub",
        TOKEN_ASSIGN_MUL => "AssignMul",
        TOKEN_ASSIGN_DIV => "AssignDiv",
        TOKEN_ASSIGN_MOD => "AssignMod",
        TOKEN_COMMA => "Comma",
        TOKEN_OR => "Or",
        TOKEN_AND => "And",
        TOKEN_BIT_OR => "BitOr",
        TOKEN_BIT_XOR => "BitXor",
        TOKEN_BIT_AND => "BitAnd",
        TOKEN_SHL => "SHL",
        TOKEN_SAR => "SAR",
        TOKEN_SHR => "SHR",
        TOKEN_ADD => "Add",
        TOKEN_SUB => "Sub",
        TOKEN_MUL => "Mul",
        TOKEN_DIV => "Div",
        TOKEN_MOD => "Mod",
        TOKEN_EXP => "Exp",
        TOKEN_EQUAL => "Equal",
        TOKEN_NOT_EQUAL => "NotEqual",
        TOKEN_LESS_THAN => "LessThan",
        TOKEN_GREATER_THAN => "GreaterThan",
        TOKEN_LESS_THAN_OR_EQUAL => "LessThanOrEqual",
        TOKEN_GREATER_THAN_OR_EQUAL => "GreaterThanOrEqual",
        TOKEN_NOT => "Not",
        TOKEN_BIT_NOT => "BitNot",
        TOKEN_INC => "Inc",
        TOKEN_DEC => "Dec",
        TOKEN_DELETE => "Delete",
        TOKEN_ASSEMBLY_ASSIGN => "AssemblyAssign",
        TOKEN_ABSTRACT => "Abstract",
        TOKEN_ANONYMOUS => "Anonymous",
        TOKEN_AS => "As",
        TOKEN_ASSEMBLY => "Assembly",
        TOKEN_BREAK => "Break",
        TOKEN_CATCH => "Catch",
        TOKEN_CONSTANT => "Constant",
        TOKEN_CONSTRUCTOR => "Constructor",
        TOKEN_CONTINUE => "Continue",
        TOKEN_CONTRACT => "Contract",
        TOKEN_DO => "Do",
        TOKEN_ELSE => "Else",
        TOKEN_ENUM => "Enum",
        TOKEN_EMIT => "Emit",
        TOKEN_EVENT => "Event",
        TOKEN_EXTERNAL => "External",
        TOKEN_FALLBACK => "Fallback",
        TOKEN_FOR => "For",
        TOKEN_FUNCTION => "Function",
        TOKEN_HEX => "Hex",
        TOKEN_IF => "If",
        TOKEN_INDEXED => "Indexed",
        TOKEN_INTERFACE => "Interface",
        TOKEN_INTERNAL => "Internal",
        TOKEN_IMMUTABLE => "Immutable",
        TOKEN_IMPORT => "Import",
        TOKEN_IS => "Is",
        TOKEN_LIBRARY => "Library",
        TOKEN_MAPPING => "Mapping",
        TOKEN_MEMORY => "Memory",
        TOKEN_MODIFIER => "Modifier",
        TOKEN_NEW => "New",
        TOKEN_OVERRIDE => "Override",
        TOKEN_PAYABLE => "Payable",
        TOKEN_PUBLIC => "Public",
        TOKEN_PRAGMA => "Pragma",
        TOKEN_PRIVATE => "Private",
        TOKEN_PURE => "Pure",
        TOKEN_RECEIVE => "Receive",
        TOKEN_RETURN => "Return",
        TOKEN_RETURNS => "Returns",
        TOKEN_STORAGE => "Storage",
        TOKEN_CALLDATA => "CallData",
        TOKEN_STRUCT => "Struct",
        TOKEN_THROW => "Throw",
        TOKEN_TRY => "Try",
        TOKEN_TYPE => "Type",
        TOKEN_UNCHECKED => "Unchecked",
        TOKEN_UNICODE => "Unicode",
        TOKEN_USING => "Using",
        TOKEN_VIEW => "View",
        TOKEN_VIRTUAL => "Virtual",
        TOKEN_WHILE => "While",
        TOKEN_SUB_WEI => "SubWei",
        TOKEN_SUB_GWEI => "SubGwei",
        TOKEN_SUB_ETHER => "SubEther",
        TOKEN_SUB_SECOND => "SubSecond",
        TOKEN_SUB_MINUTE => "SubMinute",
        TOKEN_SUB_HOUR => "SubHour",
        TOKEN_SUB_DAY => "SubDay",
        TOKEN_SUB_WEEK => "SubWeek",
        TOKEN_SUB_YEAR => "SubYear",
        TOKEN_INT => "Int",
        TOKEN_UINT => "UInt",
        TOKEN_BYTES => "Bytes",
        TOKEN_STRING => "String",
        TOKEN_ADDRESS => "Address",
        TOKEN_BOOL => "Bool",
        TOKEN_FIXED => "Fixed",
        TOKEN_UFIXED => "UFixed",
        TOKEN_INT_M => "IntM",
        TOKEN_UINT_M => "UIntM",
        TOKEN_BYTES_M => "BytesM",
        TOKEN_FIXED_MXN => "FixedMxN",
        TOKEN_UFIXED_MXN => "UFixedMxN",
        TOKEN_TYPES_END => "TypesEnd",
        TOKEN_TRUE_LITERAL => "TrueLiteral",
        TOKEN_FALSE_LITERAL => "FalseLiteral",
        TOKEN_NUMBER => "Number",
        TOKEN_STRING_LITERAL => "StringLiteral",
        TOKEN_UNICODE_STRING_LITERAL => "UnicodeStringLiteral",
        TOKEN_HEX_STRING_LITERAL => "HexStringLiteral",
        TOKEN_COMMENT_LITERAL => "CommentLiteral",
        TOKEN_IDENTIFIER => "Identifier",
        TOKEN_AFTER => "After",
        TOKEN_ALIAS => "Alias",
        TOKEN_APPLY => "Apply",
        TOKEN_AUTO => "Auto",
        TOKEN_BYTE => "Byte",
        TOKEN_CASE => "Case",
        TOKEN_COPY_OF => "CopyOf",
        TOKEN_DEFAULT => "Default",
        TOKEN_DEFINE => "Define",
        TOKEN_FINAL => "Final",
        TOKEN_IMPLEMENTS => "Implements",
        TOKEN_IN => "In",
        TOKEN_INLINE => "Inline",
        TOKEN_LET => "Let",
        TOKEN_MACRO => "Macro",
        TOKEN_MATCH => "Match",
        TOKEN_MUTABLE => "Mutable",
        TOKEN_NULL_LITERAL => "NullLiteral",
        TOKEN_OF => "Of",
        TOKEN_PARTIAL => "Partial",
        TOKEN_PROMISE => "Promise",
        TOKEN_REFERENCE => "Reference",
        TOKEN_RELOCATABLE => "Relocatable",
        TOKEN_SEALED => "Sealed",
        TOKEN_SIZEOF => "Sizeof",
        TOKEN_STATIC => "Static",
        TOKEN_SUPPORTS => "Supports",
        TOKEN_SWITCH => "Switch",
        TOKEN_TYPEDEF => "Typedef",
        TOKEN_TYPEOF => "TypeOf",
        TOKEN_VAR => "Var",
        TOKEN_LEAVE => "Leave",
        TOKEN_NON_EXPERIMENTAL_END => "NonExperimentalEnd",
        TOKEN_CLASS => "Class",
        TOKEN_INSTANTIATION => "Instantiation",
        TOKEN_INTEGER => "Integer",
        TOKEN_ITSELF => "Itself",
        TOKEN_STATIC_ASSERT => "StaticAssert",
        TOKEN_BUILTIN => "Builtin",
        TOKEN_FORALL => "ForAll",
        TOKEN_EXPERIMENTAL_END => "ExperimentalEnd",
        TOKEN_ILLEGAL => "Illegal",
        TOKEN_WHITESPACE => "Whitespace",
        _ => unreachable!(),
    }
}

pub fn friendly_name(token: u32) -> String {
    if let Some(text) = to_string(token) {
        return text.to_string();
    }
    name(token).to_string()
}

pub fn elementary_type_name_token_to_string(
    token: u32,
    first_number: u32,
    second_number: u32,
    token_value: bool,
) -> String {
    let name = to_string(token).unwrap_or("").to_string();
    if token_value || (first_number == 0 && second_number == 0) {
        return name;
    }

    assert!(
        name.len() >= 3,
        "Token name size should be greater than 3. Should not reach here."
    );
    if token == TOKEN_FIXED_MXN || token == TOKEN_UFIXED_MXN {
        format!(
            "{}{}x{}",
            &name[..name.len() - 3],
            first_number,
            second_number
        )
    } else {
        format!("{}{}", &name[..name.len() - 1], first_number)
    }
}

pub fn elementary_type_name_token_assert_details(base_type: u32, first: u32, second: u32) {
    assert!(
        is_elementary_type_name(base_type),
        "Expected elementary type name: {}",
        to_string(base_type).unwrap_or("")
    );

    if base_type == TOKEN_BYTES_M {
        assert!(
            second == 0,
            "There should not be a second size argument to type bytesM."
        );
        assert!(first <= 32, "No elementary type bytes{}.", first);
    } else if base_type == TOKEN_UINT_M || base_type == TOKEN_INT_M {
        assert!(
            second == 0,
            "There should not be a second size argument to type {}.",
            to_string(base_type).unwrap_or("")
        );
        assert!(
            first <= 256 && first.is_multiple_of(8),
            "No elementary type {}{}.",
            to_string(base_type).unwrap_or(""),
            first
        );
    } else if base_type == TOKEN_UFIXED_MXN || base_type == TOKEN_FIXED_MXN {
        assert!(
            (8..=256).contains(&first) && first.is_multiple_of(8) && second <= 80,
            "No elementary type {}{}x{}.",
            to_string(base_type).unwrap_or(""),
            first,
            second
        );
    } else {
        assert!(first == 0 && second == 0, "Unexpected size arguments");
    }
}

pub fn is_location_specifier(token: u32) -> bool {
    token == TOKEN_MEMORY || token == TOKEN_STORAGE || token == TOKEN_CALLDATA
}

pub fn precedence(token: u32) -> i32 {
    match token {
        10 => 3,
        13..=24 => 2,
        25 => 1,
        26 => 4,
        27 => 5,
        28 => 8,
        29 => 9,
        30 => 10,
        31..=33 => 11,
        34..=35 => 12,
        36..=38 => 13,
        39 => 14,
        40..=41 => 6,
        42..=45 => 7,
        51 => 2,
        _ => 0,
    }
}

pub fn has_exp_highest_precedence() -> bool {
    let exp_precedence = precedence(TOKEN_EXP);
    (0..TOKEN_NUM_TOKENS).all(|token| token == TOKEN_EXP || precedence(token) < exp_precedence)
}
