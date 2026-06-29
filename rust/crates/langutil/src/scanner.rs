use crate::char_stream::CharStream;
use crate::common::{
    hex_value, is_decimal_digit, is_hex_digit, is_identifier_part, is_identifier_start,
    is_white_space,
};
use crate::token;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScannerKind {
    Solidity,
    Yul,
    ExperimentalSolidity,
    SpecialComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScannerError {
    NoError,
    IllegalToken,
    IllegalHexString,
    IllegalHexDigit,
    IllegalCommentTerminator,
    IllegalEscapeSequence,
    UnicodeCharacterInNonUnicodeString,
    IllegalCharacterInString,
    IllegalStringEndQuote,
    IllegalNumberSeparator,
    IllegalExponent,
    IllegalNumberEnd,
    DirectionalOverrideUnderflow,
    DirectionalOverrideMismatch,
    OctalNotAllowed,
}

impl ScannerError {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoError => "No error.",
            Self::IllegalToken => "Invalid token.",
            Self::IllegalHexString => "Expected even number of hex-nibbles.",
            Self::IllegalHexDigit => "Hexadecimal digit missing or invalid.",
            Self::IllegalCommentTerminator => "Expected multi-line comment-terminator.",
            Self::IllegalEscapeSequence => "Invalid escape sequence.",
            Self::UnicodeCharacterInNonUnicodeString => {
                "Invalid character in string. If you are trying to use Unicode characters, use a unicode\"...\" string literal."
            }
            Self::IllegalCharacterInString => "Invalid character in string.",
            Self::IllegalStringEndQuote => "Expected string end-quote.",
            Self::IllegalNumberSeparator => "Invalid use of number separator '_'.",
            Self::IllegalExponent => "Invalid exponent.",
            Self::IllegalNumberEnd => "Identifier-start is not allowed at end of a number.",
            Self::DirectionalOverrideUnderflow => {
                "Unicode direction override underflow in comment or string literal."
            }
            Self::DirectionalOverrideMismatch => {
                "Mismatching directional override markers in comment or string literal."
            }
            Self::OctalNotAllowed => "Octal numbers not allowed.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub start: i64,
    pub end: i64,
    pub source_id: i64,
}

impl SourceLocation {
    fn with_source(start: usize, end: usize) -> Self {
        Self {
            start: start as i64,
            end: end as i64,
            source_id: 0,
        }
    }
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            start: -1,
            end: -1,
            source_id: -1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedToken {
    pub token: u32,
    pub literal: Vec<u8>,
    pub first_number: u32,
    pub second_number: u32,
    pub error: String,
    pub location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub literal: Vec<u8>,
    pub location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannerOutput {
    pub tokens: Vec<LocatedToken>,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug)]
struct TokenDesc {
    token: u32,
    location: SourceLocation,
    literal: Vec<u8>,
    error: ScannerError,
    first_number: u32,
    second_number: u32,
}

impl Default for TokenDesc {
    fn default() -> Self {
        Self {
            token: token::TOKEN_EOS,
            location: SourceLocation::default(),
            literal: Vec::new(),
            error: ScannerError::NoError,
            first_number: 0,
            second_number: 0,
        }
    }
}

#[derive(Clone, Copy)]
enum TokenIndex {
    Current = 0,
    Next = 1,
    NextNext = 2,
}

pub fn scan(source: Vec<u8>, kind: ScannerKind) -> ScannerOutput {
    Scanner::new(source, kind).scan_to_end()
}

pub fn scan_borrowed(source: &[u8], kind: ScannerKind) -> ScannerOutput {
    Scanner::new_borrowed(source, kind).scan_to_end()
}

pub struct Scanner<'a> {
    source: CharStream<'a>,
    kind: ScannerKind,
    current_char: u8,
    tokens: [TokenDesc; 3],
    skipped_comments: [TokenDesc; 3],
}

impl Scanner<'static> {
    pub fn new(source: Vec<u8>, kind: ScannerKind) -> Self {
        Self::from_char_stream(CharStream::new(source), kind)
    }
}

impl<'a> Scanner<'a> {
    pub fn new_borrowed(source: &'a [u8], kind: ScannerKind) -> Self {
        Self::from_char_stream(CharStream::borrowed(source), kind)
    }

    fn from_char_stream(source: CharStream<'a>, kind: ScannerKind) -> Self {
        let mut scanner = Self {
            source,
            kind: ScannerKind::Solidity,
            current_char: 0,
            tokens: std::array::from_fn(|_| TokenDesc::default()),
            skipped_comments: std::array::from_fn(|_| TokenDesc::default()),
        };
        scanner.reset();
        if kind != ScannerKind::Solidity {
            scanner.set_scanner_mode(kind);
        }
        scanner
    }

    pub fn reset(&mut self) {
        self.source.reset();
        self.kind = ScannerKind::Solidity;
        self.current_char = self.source.get(0);
        self.skip_whitespace();
        self.next();
        self.next();
        self.next();
    }

    pub fn set_scanner_mode(&mut self, kind: ScannerKind) {
        self.kind = kind;
        self.rescan();
    }

    pub fn set_position(&mut self, offset: usize) {
        self.current_char = self.source.set_position(offset);
        self.scan_token();
        self.next();
        self.next();
    }

    pub fn current_token(&self) -> u32 {
        self.tokens[TokenIndex::Current as usize].token
    }

    pub fn current_location(&self) -> SourceLocation {
        self.tokens[TokenIndex::Current as usize].location.clone()
    }

    pub fn current_literal(&self) -> &[u8] {
        &self.tokens[TokenIndex::Current as usize].literal
    }

    pub fn current_token_info(&self) -> (u32, u32) {
        let current = &self.tokens[TokenIndex::Current as usize];
        (current.first_number, current.second_number)
    }

    pub fn current_error(&self) -> ScannerError {
        self.tokens[TokenIndex::Current as usize].error
    }

    pub fn current_comment_location(&self) -> SourceLocation {
        self.skipped_comments[TokenIndex::Current as usize]
            .location
            .clone()
    }

    pub fn current_comment_literal(&self) -> &[u8] {
        &self.skipped_comments[TokenIndex::Current as usize].literal
    }

    pub fn clear_current_comment_literal(&mut self) {
        self.skipped_comments[TokenIndex::Current as usize]
            .literal
            .clear();
    }

    pub fn peek_next_token(&self) -> u32 {
        self.tokens[TokenIndex::Next as usize].token
    }

    pub fn peek_next_next_token(&self) -> u32 {
        self.tokens[TokenIndex::NextNext as usize].token
    }

    pub fn next(&mut self) -> u32 {
        self.tokens[TokenIndex::Current as usize] =
            std::mem::take(&mut self.tokens[TokenIndex::Next as usize]);
        self.tokens[TokenIndex::Next as usize] =
            std::mem::take(&mut self.tokens[TokenIndex::NextNext as usize]);
        self.skipped_comments[TokenIndex::Current as usize] =
            std::mem::take(&mut self.skipped_comments[TokenIndex::Next as usize]);
        self.skipped_comments[TokenIndex::Next as usize] =
            std::mem::take(&mut self.skipped_comments[TokenIndex::NextNext as usize]);

        self.scan_token();
        self.current_token()
    }

    pub fn scan_to_end(mut self) -> ScannerOutput {
        let mut tokens = Vec::new();
        let mut comments = Vec::new();

        loop {
            let current = self.current_located_token();
            let is_eos = current.token == token::TOKEN_EOS;
            tokens.push(current);
            comments.push(self.current_comment());
            if is_eos {
                break;
            }
            self.next();
        }

        ScannerOutput { tokens, comments }
    }

    fn current_located_token(&self) -> LocatedToken {
        let current = &self.tokens[TokenIndex::Current as usize];
        LocatedToken {
            token: current.token,
            literal: current.literal.clone(),
            first_number: current.first_number,
            second_number: current.second_number,
            error: if current.token == token::TOKEN_ILLEGAL {
                current.error.message().to_string()
            } else {
                String::new()
            },
            location: current.location.clone(),
        }
    }

    fn current_comment(&self) -> Comment {
        let current = &self.skipped_comments[TokenIndex::Current as usize];
        Comment {
            literal: current.literal.clone(),
            location: current.location.clone(),
        }
    }

    fn set_error(&mut self, error: ScannerError) -> u32 {
        self.tokens[TokenIndex::NextNext as usize].error = error;
        token::TOKEN_ILLEGAL
    }

    fn add_literal_char(&mut self, byte: u8) {
        self.tokens[TokenIndex::NextNext as usize]
            .literal
            .push(byte);
    }

    fn add_comment_literal_char(&mut self, byte: u8) {
        self.skipped_comments[TokenIndex::NextNext as usize]
            .literal
            .push(byte);
    }

    fn add_literal_char_and_advance(&mut self) {
        self.add_literal_char(self.current_char);
        self.advance();
    }

    fn add_unicode_as_utf8(&mut self, codepoint: u32) {
        if codepoint <= 0x7f {
            self.add_literal_char(codepoint as u8);
        } else if codepoint <= 0x7ff {
            self.add_literal_char((0xc0 | (codepoint >> 6)) as u8);
            self.add_literal_char((0x80 | (codepoint & 0x3f)) as u8);
        } else {
            self.add_literal_char((0xe0 | (codepoint >> 12)) as u8);
            self.add_literal_char((0x80 | ((codepoint >> 6) & 0x3f)) as u8);
            self.add_literal_char((0x80 | (codepoint & 0x3f)) as u8);
        }
    }

    fn advance(&mut self) -> bool {
        self.current_char = self.source.advance_and_get(1);
        !self.source.is_past_end_of_input(0)
    }

    fn rollback(&mut self, amount: usize) {
        self.current_char = self.source.rollback(amount);
    }

    fn rescan(&mut self) {
        let current = TokenIndex::Current as usize;
        let rollback_to = if self.skipped_comments[current].literal.is_empty() {
            self.tokens[current].location.start
        } else {
            self.skipped_comments[current].location.start
        };
        let rollback_to = rollback_to.max(0) as usize;
        let amount = self.source.position().saturating_sub(rollback_to);
        self.current_char = self.source.rollback(amount);
        self.next();
        self.next();
        self.next();
    }

    fn select_error_token(&mut self, error: ScannerError) -> u32 {
        self.advance();
        self.set_error(error)
    }

    fn select_token(&mut self, selected: u32) -> u32 {
        self.advance();
        selected
    }

    fn select_token_if(&mut self, next: u8, then_token: u32, else_token: u32) -> u32 {
        self.advance();
        if self.current_char == next {
            self.select_token(then_token)
        } else {
            else_token
        }
    }

    fn scan_hex_byte(&mut self) -> Option<u8> {
        let mut value = 0u8;
        for index in 0..2 {
            let Some(digit) = hex_value(self.current_char) else {
                self.rollback(index);
                return None;
            };
            value = value.wrapping_mul(16).wrapping_add(digit);
            self.advance();
        }
        Some(value)
    }

    fn scan_unicode(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for index in 0..4 {
            let Some(digit) = hex_value(self.current_char) else {
                self.rollback(index);
                return None;
            };
            value = value * 16 + u32::from(digit);
            self.advance();
        }
        Some(value)
    }

    fn skip_whitespace(&mut self) -> bool {
        let start_position = self.source_pos();
        while is_white_space(self.current_char) {
            self.advance();
        }
        self.source_pos() != start_position
    }

    fn skip_whitespace_except_unicode_linebreak(&mut self) -> bool {
        let start_position = self.source_pos();
        while is_white_space(self.current_char) && !self.is_unicode_linebreak() {
            self.advance();
        }
        self.source_pos() != start_position
    }

    fn skip_single_line_comment(&mut self) -> u32 {
        let start_position = self.source.position();
        while !self.is_unicode_linebreak() {
            if !self.advance() {
                break;
            }
        }

        let unicode_direction_error = validate_bidi_markup(&self.source, start_position);
        if unicode_direction_error != ScannerError::NoError {
            return self.set_error(unicode_direction_error);
        }

        token::TOKEN_WHITESPACE
    }

    fn at_end_of_line(&self) -> bool {
        self.current_char == b'\n' || self.current_char == b'\r'
    }

    fn try_scan_end_of_line(&mut self) -> bool {
        if self.current_char == b'\n' {
            self.advance();
            return true;
        }

        if self.current_char == b'\r' {
            if self.advance() && self.current_char == b'\n' {
                self.advance();
            }
            return true;
        }

        false
    }

    fn scan_single_line_doc_comment(&mut self) -> usize {
        self.skipped_comments[TokenIndex::NextNext as usize]
            .literal
            .clear();
        let mut end_position = self.source.position();

        self.skip_whitespace_except_unicode_linebreak();

        while !self.is_source_past_end_of_input() {
            end_position = self.source.position();
            if self.try_scan_end_of_line() {
                if !self.skip_whitespace_except_unicode_linebreak() {
                    end_position = self.source.position();
                }

                if !self.source.is_past_end_of_input(3)
                    && self.source.get(0) == b'/'
                    && self.source.get(1) == b'/'
                    && self.source.get(2) == b'/'
                {
                    if !self.source.is_past_end_of_input(4) && self.source.get(3) == b'/' {
                        break;
                    }
                    self.current_char = self.source.advance_and_get(3);
                    if self.at_end_of_line() {
                        continue;
                    }
                    self.add_comment_literal_char(b'\n');
                } else {
                    break;
                }
            } else if self.is_unicode_linebreak() {
                break;
            }
            self.add_comment_literal_char(self.current_char);
            self.advance();
        }
        end_position
    }

    fn skip_multi_line_comment(&mut self) -> u32 {
        let start_position = self.source.position();
        while !self.is_source_past_end_of_input() {
            let previous_char = self.current_char;
            self.advance();

            if previous_char == b'*' && self.current_char == b'/' {
                let unicode_direction_error = validate_bidi_markup(&self.source, start_position);
                if unicode_direction_error != ScannerError::NoError {
                    return self.set_error(unicode_direction_error);
                }

                self.current_char = b' ';
                return token::TOKEN_WHITESPACE;
            }
        }
        self.set_error(ScannerError::IllegalCommentTerminator)
    }

    fn scan_multi_line_doc_comment(&mut self) -> u32 {
        self.skipped_comments[TokenIndex::NextNext as usize]
            .literal
            .clear();
        let mut end_found = false;
        let mut chars_added = false;

        while is_white_space(self.current_char) && !self.at_end_of_line() {
            self.advance();
        }

        while !self.is_source_past_end_of_input() {
            if self.at_end_of_line() {
                self.skip_whitespace();
                if !self.source.is_past_end_of_input(1)
                    && self.source.get(0) == b'*'
                    && self.source.get(1) == b'*'
                {
                    self.add_comment_literal_char(b'*');
                    self.advance();
                } else if !self.source.is_past_end_of_input(1)
                    && self.source.get(0) == b'*'
                    && self.source.get(1) != b'/'
                {
                    self.current_char = self.source.advance_and_get(1);
                    if self.at_end_of_line() {
                        continue;
                    }
                    if chars_added {
                        self.add_comment_literal_char(b'\n');
                    }
                } else if !self.source.is_past_end_of_input(1)
                    && self.source.get(0) == b'*'
                    && self.source.get(1) == b'/'
                {
                    self.current_char = self.source.advance_and_get(2);
                    end_found = true;
                    break;
                } else if chars_added {
                    self.add_comment_literal_char(b'\n');
                }
            }

            if !self.source.is_past_end_of_input(1)
                && self.source.get(0) == b'*'
                && self.source.get(1) == b'/'
            {
                self.current_char = self.source.advance_and_get(2);
                end_found = true;
                break;
            }
            self.add_comment_literal_char(self.current_char);
            chars_added = true;
            self.advance();
        }

        if !end_found {
            self.skipped_comments[TokenIndex::NextNext as usize]
                .literal
                .clear();
            self.set_error(ScannerError::IllegalCommentTerminator)
        } else {
            token::TOKEN_COMMENT_LITERAL
        }
    }

    fn scan_slash(&mut self) -> u32 {
        let first_slash_position = self.source_pos();
        self.advance();
        if self.current_char == b'/' {
            if !self.advance() {
                token::TOKEN_WHITESPACE
            } else if self.current_char == b'/' {
                self.advance();

                if self.current_char == b'/' {
                    return self.skip_single_line_comment();
                }

                let next_next = TokenIndex::NextNext as usize;
                self.skipped_comments[next_next].location =
                    SourceLocation::with_source(first_slash_position, -1i64.max(0) as usize);
                self.skipped_comments[next_next].token = token::TOKEN_COMMENT_LITERAL;
                let end = self.scan_single_line_doc_comment();
                self.skipped_comments[next_next].location =
                    SourceLocation::with_source(first_slash_position, end);
                token::TOKEN_WHITESPACE
            } else {
                self.skip_single_line_comment()
            }
        } else if self.current_char == b'*' {
            if !self.advance() {
                self.set_error(ScannerError::IllegalCommentTerminator)
            } else if self.current_char == b'*' {
                self.advance();

                if self.current_char == b'/' {
                    self.advance();
                    return token::TOKEN_WHITESPACE;
                }
                if self.current_char == b'*' {
                    return self.skip_multi_line_comment();
                }

                let next_next = TokenIndex::NextNext as usize;
                self.skipped_comments[next_next].location =
                    SourceLocation::with_source(first_slash_position, first_slash_position);
                let comment = self.scan_multi_line_doc_comment();
                self.skipped_comments[next_next].location =
                    SourceLocation::with_source(first_slash_position, self.source_pos());
                self.skipped_comments[next_next].token = comment;
                if comment == token::TOKEN_ILLEGAL {
                    token::TOKEN_ILLEGAL
                } else {
                    token::TOKEN_WHITESPACE
                }
            } else {
                self.skip_multi_line_comment()
            }
        } else if self.current_char == b'=' {
            self.select_token(token::TOKEN_ASSIGN_DIV)
        } else {
            token::TOKEN_DIV
        }
    }

    fn scan_token(&mut self) {
        let next_next = TokenIndex::NextNext as usize;
        self.tokens[next_next] = TokenDesc::default();
        self.skipped_comments[next_next] = TokenDesc::default();

        let mut scanned_token;
        let mut first_number = 0;
        let mut second_number = 0;
        loop {
            self.tokens[next_next].location.start = self.source_pos() as i64;
            scanned_token = match self.current_char {
                b'"' | b'\'' => self.scan_string(false),
                b'<' => {
                    self.advance();
                    if self.current_char == b'=' {
                        self.select_token(token::TOKEN_LESS_THAN_OR_EQUAL)
                    } else if self.current_char == b'<' {
                        self.select_token_if(b'=', token::TOKEN_ASSIGN_SHL, token::TOKEN_SHL)
                    } else {
                        token::TOKEN_LESS_THAN
                    }
                }
                b'>' => {
                    self.advance();
                    if self.current_char == b'=' {
                        self.select_token(token::TOKEN_GREATER_THAN_OR_EQUAL)
                    } else if self.current_char == b'>' {
                        self.advance();
                        if self.current_char == b'=' {
                            self.select_token(token::TOKEN_ASSIGN_SAR)
                        } else if self.current_char == b'>' {
                            self.select_token_if(b'=', token::TOKEN_ASSIGN_SHR, token::TOKEN_SHR)
                        } else {
                            token::TOKEN_SAR
                        }
                    } else {
                        token::TOKEN_GREATER_THAN
                    }
                }
                b'=' => {
                    self.advance();
                    if self.current_char == b'=' {
                        self.select_token(token::TOKEN_EQUAL)
                    } else if self.current_char == b'>' {
                        self.select_token(token::TOKEN_DOUBLE_ARROW)
                    } else {
                        token::TOKEN_ASSIGN
                    }
                }
                b'!' => {
                    self.advance();
                    if self.current_char == b'=' {
                        self.select_token(token::TOKEN_NOT_EQUAL)
                    } else {
                        token::TOKEN_NOT
                    }
                }
                b'+' => {
                    self.advance();
                    if self.current_char == b'+' {
                        self.select_token(token::TOKEN_INC)
                    } else if self.current_char == b'=' {
                        self.select_token(token::TOKEN_ASSIGN_ADD)
                    } else {
                        token::TOKEN_ADD
                    }
                }
                b'-' => {
                    self.advance();
                    if self.current_char == b'-' {
                        self.select_token(token::TOKEN_DEC)
                    } else if self.current_char == b'=' {
                        self.select_token(token::TOKEN_ASSIGN_SUB)
                    } else if self.current_char == b'>' {
                        self.select_token(token::TOKEN_RIGHT_ARROW)
                    } else {
                        token::TOKEN_SUB
                    }
                }
                b'*' => {
                    self.advance();
                    if self.current_char == b'*' {
                        self.select_token(token::TOKEN_EXP)
                    } else if self.current_char == b'=' {
                        self.select_token(token::TOKEN_ASSIGN_MUL)
                    } else {
                        token::TOKEN_MUL
                    }
                }
                b'%' => self.select_token_if(b'=', token::TOKEN_ASSIGN_MOD, token::TOKEN_MOD),
                b'/' => self.scan_slash(),
                b'&' => {
                    self.advance();
                    if self.current_char == b'&' {
                        self.select_token(token::TOKEN_AND)
                    } else if self.current_char == b'=' {
                        self.select_token(token::TOKEN_ASSIGN_BIT_AND)
                    } else {
                        token::TOKEN_BIT_AND
                    }
                }
                b'|' => {
                    self.advance();
                    if self.current_char == b'|' {
                        self.select_token(token::TOKEN_OR)
                    } else if self.current_char == b'=' {
                        self.select_token(token::TOKEN_ASSIGN_BIT_OR)
                    } else {
                        token::TOKEN_BIT_OR
                    }
                }
                b'^' => {
                    self.select_token_if(b'=', token::TOKEN_ASSIGN_BIT_XOR, token::TOKEN_BIT_XOR)
                }
                b'.' => {
                    self.advance();
                    if self.kind != ScannerKind::ExperimentalSolidity
                        && is_decimal_digit(self.current_char)
                    {
                        self.scan_number(Some(b'.'))
                    } else {
                        token::TOKEN_PERIOD
                    }
                }
                b':' => {
                    self.advance();
                    if self.current_char == b'=' {
                        self.select_token(token::TOKEN_ASSEMBLY_ASSIGN)
                    } else {
                        token::TOKEN_COLON
                    }
                }
                b';' => self.select_token(token::TOKEN_SEMICOLON),
                b',' => self.select_token(token::TOKEN_COMMA),
                b'(' => self.select_token(token::TOKEN_LPAREN),
                b')' => self.select_token(token::TOKEN_RPAREN),
                b'[' => self.select_token(token::TOKEN_LBRACK),
                b']' => self.select_token(token::TOKEN_RBRACK),
                b'{' => self.select_token(token::TOKEN_LBRACE),
                b'}' => self.select_token(token::TOKEN_RBRACE),
                b'?' => self.select_token(token::TOKEN_CONDITIONAL),
                b'~' => self.select_token(token::TOKEN_BIT_NOT),
                _ => {
                    if is_identifier_start(self.current_char) {
                        let (identifier_token, m, n) = self.scan_identifier_or_keyword();
                        first_number = m;
                        second_number = n;

                        if identifier_token == token::TOKEN_HEX {
                            first_number = 0;
                            second_number = 0;
                            if self.current_char == b'"' || self.current_char == b'\'' {
                                self.scan_hex_string()
                            } else {
                                self.set_error(ScannerError::IllegalToken)
                            }
                        } else if identifier_token == token::TOKEN_UNICODE
                            && self.kind != ScannerKind::Yul
                        {
                            first_number = 0;
                            second_number = 0;
                            if self.current_char == b'"' || self.current_char == b'\'' {
                                self.scan_string(true)
                            } else {
                                self.set_error(ScannerError::IllegalToken)
                            }
                        } else {
                            identifier_token
                        }
                    } else if is_decimal_digit(self.current_char) {
                        self.scan_number(None)
                    } else if self.skip_whitespace() {
                        token::TOKEN_WHITESPACE
                    } else if self.is_source_past_end_of_input() {
                        token::TOKEN_EOS
                    } else {
                        self.select_error_token(ScannerError::IllegalToken)
                    }
                }
            };

            if scanned_token != token::TOKEN_WHITESPACE {
                break;
            }
        }

        self.tokens[next_next].location.end = self.source_pos() as i64;
        self.tokens[next_next].location.source_id = 0;
        self.tokens[next_next].token = scanned_token;
        self.tokens[next_next].first_number = first_number;
        self.tokens[next_next].second_number = second_number;
    }

    fn scan_escape(&mut self) -> bool {
        let mut byte = self.current_char;

        if self.try_scan_end_of_line() {
            return true;
        }
        self.advance();

        match byte {
            b'\'' | b'"' | b'\\' => {}
            b'n' => byte = b'\n',
            b'r' => byte = b'\r',
            b't' => byte = b'\t',
            b'u' => {
                if let Some(codepoint) = self.scan_unicode() {
                    self.add_unicode_as_utf8(codepoint);
                    return true;
                }
                return false;
            }
            b'x' => {
                let Some(scanned) = self.scan_hex_byte() else {
                    return false;
                };
                byte = scanned;
            }
            _ => return false,
        }

        self.add_literal_char(byte);
        true
    }

    fn is_unicode_linebreak(&self) -> bool {
        if (0x0a..=0x0d).contains(&self.current_char) {
            return true;
        }
        if !self.source.is_past_end_of_input(1)
            && self.source.get(0) == 0xc2
            && self.source.get(1) == 0x85
        {
            return true;
        }
        !self.source.is_past_end_of_input(2)
            && self.source.get(0) == 0xe2
            && self.source.get(1) == 0x80
            && matches!(self.source.get(2), 0xa8 | 0xa9)
    }

    fn scan_string(&mut self, is_unicode: bool) -> u32 {
        let start_position = self.source.position();
        let quote = self.current_char;
        self.advance();
        self.tokens[TokenIndex::NextNext as usize].literal.clear();

        while self.current_char != quote
            && !self.is_source_past_end_of_input()
            && (!self.is_unicode_linebreak() || self.kind == ScannerKind::SpecialComment)
        {
            let byte = self.current_char;
            self.advance();

            if self.kind == ScannerKind::SpecialComment {
                if byte == b'\\' {
                    if self.is_source_past_end_of_input() {
                        self.tokens[TokenIndex::NextNext as usize].literal.clear();
                        return self.set_error(ScannerError::IllegalEscapeSequence);
                    }
                    self.advance();
                } else {
                    self.add_literal_char(byte);
                }
            } else if byte == b'\\' {
                if self.is_source_past_end_of_input() || !self.scan_escape() {
                    self.tokens[TokenIndex::NextNext as usize].literal.clear();
                    return self.set_error(ScannerError::IllegalEscapeSequence);
                }
            } else {
                if !is_unicode && (byte <= 0x1f || byte >= 0x7f) {
                    self.tokens[TokenIndex::NextNext as usize].literal.clear();
                    if self.kind == ScannerKind::Yul {
                        return self.set_error(ScannerError::IllegalCharacterInString);
                    }
                    return self.set_error(ScannerError::UnicodeCharacterInNonUnicodeString);
                }
                self.add_literal_char(byte);
            }
        }

        if self.current_char != quote {
            self.tokens[TokenIndex::NextNext as usize].literal.clear();
            return self.set_error(ScannerError::IllegalStringEndQuote);
        }

        if is_unicode {
            let unicode_direction_error = validate_bidi_markup(&self.source, start_position);
            if unicode_direction_error != ScannerError::NoError {
                self.tokens[TokenIndex::NextNext as usize].literal.clear();
                return self.set_error(unicode_direction_error);
            }
        }

        self.advance();
        if is_unicode {
            token::TOKEN_UNICODE_STRING_LITERAL
        } else {
            token::TOKEN_STRING_LITERAL
        }
    }

    fn scan_hex_string(&mut self) -> u32 {
        let quote = self.current_char;
        self.advance();
        self.tokens[TokenIndex::NextNext as usize].literal.clear();
        let mut allow_underscore = false;

        while self.current_char != quote && !self.is_source_past_end_of_input() {
            let byte = self.current_char;
            if let Some(scanned) = self.scan_hex_byte() {
                self.add_literal_char(scanned);
                allow_underscore = true;
            } else if byte == b'_' {
                self.advance();
                if !allow_underscore || self.current_char == quote {
                    self.tokens[TokenIndex::NextNext as usize].literal.clear();
                    return self.set_error(ScannerError::IllegalNumberSeparator);
                }
                allow_underscore = false;
            } else {
                self.tokens[TokenIndex::NextNext as usize].literal.clear();
                return self.set_error(ScannerError::IllegalHexString);
            }
        }

        if self.current_char != quote {
            self.tokens[TokenIndex::NextNext as usize].literal.clear();
            return self.set_error(ScannerError::IllegalStringEndQuote);
        }

        self.advance();
        token::TOKEN_HEX_STRING_LITERAL
    }

    fn scan_decimal_digits(&mut self) {
        if !is_decimal_digit(self.current_char) {
            return;
        }

        loop {
            self.add_literal_char_and_advance();
            if self.source.is_past_end_of_input(0)
                || !(is_decimal_digit(self.current_char) || self.current_char == b'_')
            {
                break;
            }
        }
    }

    fn scan_number(&mut self, char_seen: Option<u8>) -> u32 {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum NumberKind {
            Decimal,
            Hex,
        }

        let mut kind = NumberKind::Decimal;
        self.tokens[TokenIndex::NextNext as usize].literal.clear();
        if char_seen == Some(b'.') {
            self.add_literal_char(b'.');
            if self.current_char == b'_' {
                self.tokens[TokenIndex::NextNext as usize].literal.clear();
                return self.set_error(ScannerError::IllegalToken);
            }
            self.scan_decimal_digits();
        } else {
            assert!(char_seen.is_none());
            if self.current_char == b'0' {
                self.add_literal_char_and_advance();
                if self.current_char == b'x' {
                    kind = NumberKind::Hex;
                    self.add_literal_char_and_advance();
                    if !is_hex_digit(self.current_char) {
                        self.tokens[TokenIndex::NextNext as usize].literal.clear();
                        return self.set_error(ScannerError::IllegalHexDigit);
                    }

                    while is_hex_digit(self.current_char) || self.current_char == b'_' {
                        self.add_literal_char_and_advance();
                    }
                } else if is_decimal_digit(self.current_char) {
                    self.tokens[TokenIndex::NextNext as usize].literal.clear();
                    return self.set_error(ScannerError::OctalNotAllowed);
                }
            }

            if kind == NumberKind::Decimal {
                self.scan_decimal_digits();
                if self.current_char == b'.' {
                    if !self.source.is_past_end_of_input(1) && self.source.get(1) == b'_' {
                        self.add_literal_char_and_advance();
                        self.add_literal_char_and_advance();
                        self.scan_decimal_digits();
                    }
                    if self.source.is_past_end_of_input(0) || !is_decimal_digit(self.source.get(1))
                    {
                        return token::TOKEN_NUMBER;
                    }
                    self.add_literal_char_and_advance();
                    self.scan_decimal_digits();
                }
            }
        }

        if self.current_char == b'e' || self.current_char == b'E' {
            assert!(
                kind != NumberKind::Hex,
                "'e'/'E' must be scanned as part of the hex number"
            );
            if kind != NumberKind::Decimal {
                self.tokens[TokenIndex::NextNext as usize].literal.clear();
                return self.set_error(ScannerError::IllegalExponent);
            } else if !self.source.is_past_end_of_input(1) && self.source.get(1) == b'_' {
                self.add_literal_char_and_advance();
                self.add_literal_char_and_advance();
                self.scan_decimal_digits();
                return token::TOKEN_NUMBER;
            }

            self.add_literal_char_and_advance();
            if self.current_char == b'+' || self.current_char == b'-' {
                self.add_literal_char_and_advance();
            }
            if !is_decimal_digit(self.current_char) {
                self.tokens[TokenIndex::NextNext as usize].literal.clear();
                return self.set_error(ScannerError::IllegalExponent);
            }
            self.scan_decimal_digits();
        }

        if is_decimal_digit(self.current_char) || is_identifier_start(self.current_char) {
            self.tokens[TokenIndex::NextNext as usize].literal.clear();
            return self.set_error(ScannerError::IllegalNumberEnd);
        }
        token::TOKEN_NUMBER
    }

    fn scan_identifier_or_keyword(&mut self) -> (u32, u32, u32) {
        assert!(is_identifier_start(self.current_char));
        self.tokens[TokenIndex::NextNext as usize].literal.clear();
        self.add_literal_char_and_advance();
        while is_identifier_part(self.current_char)
            || (self.current_char == b'.' && self.kind == ScannerKind::Yul)
        {
            self.add_literal_char_and_advance();
        }

        let literal = self.tokens[TokenIndex::NextNext as usize].literal.clone();
        let scanned = token::from_identifier_or_keyword(&literal);
        match self.kind {
            ScannerKind::SpecialComment => (token::TOKEN_IDENTIFIER, 0, 0),
            ScannerKind::Solidity => {
                if token::is_experimental_solidity_only_keyword(scanned.0) {
                    (token::TOKEN_IDENTIFIER, 0, 0)
                } else {
                    scanned
                }
            }
            ScannerKind::Yul => {
                if literal == b"leave" {
                    (token::TOKEN_LEAVE, 0, 0)
                } else if !token::is_yul_keyword_token(scanned.0) {
                    (token::TOKEN_IDENTIFIER, 0, 0)
                } else {
                    scanned
                }
            }
            ScannerKind::ExperimentalSolidity => {
                if !token::is_experimental_solidity_keyword(scanned.0) {
                    (token::TOKEN_IDENTIFIER, 0, 0)
                } else {
                    scanned
                }
            }
        }
    }

    fn source_pos(&self) -> usize {
        self.source.position()
    }

    fn is_source_past_end_of_input(&self) -> bool {
        self.source.is_past_end_of_input(0)
    }
}

fn validate_bidi_markup(stream: &CharStream, start_position: usize) -> ScannerError {
    const DIRECTIONAL_SEQUENCES: [(&[u8], i32); 5] = [
        (b"\xE2\x80\xAD", 1),
        (b"\xE2\x80\xAE", 1),
        (b"\xE2\x80\xAA", 1),
        (b"\xE2\x80\xAB", 1),
        (b"\xE2\x80\xAC", -1),
    ];

    let end_position = stream.position();
    let mut direction_override_depth = 0;

    for current_position in start_position..end_position {
        for (sequence, depth_change) in DIRECTIONAL_SEQUENCES {
            if prefix_match_at(stream.source(), current_position, sequence) {
                direction_override_depth += depth_change;
            }
        }

        if direction_override_depth < 0 {
            return ScannerError::DirectionalOverrideUnderflow;
        }
    }

    if direction_override_depth > 0 {
        ScannerError::DirectionalOverrideMismatch
    } else {
        ScannerError::NoError
    }
}

fn prefix_match_at(source: &[u8], position: usize, sequence: &[u8]) -> bool {
    if position.saturating_add(sequence.len()) >= source.len() {
        return false;
    }

    source
        .get(position..position + sequence.len())
        .is_some_and(|candidate| candidate == sequence)
}
