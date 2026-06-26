use crate::bridge::ffi;

const ERROR_NO_PARAM_NAME: u32 = 3335;
const ERROR_NO_PARAM_DESCRIPTION: u32 = 9942;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DocTag {
    content: Vec<u8>,
    param_name: Vec<u8>,
}

#[derive(Clone, Debug)]
struct DocStringParsingError {
    error_id: u32,
    location: ffi::WireSourceLocation,
    message: Vec<u8>,
}

#[derive(Clone, Debug)]
struct DocStringParser {
    text: Vec<u8>,
    location: ffi::WireSourceLocation,
    doc_tags: Vec<(Vec<u8>, DocTag)>,
    last_tag: Option<usize>,
    errors: Vec<DocStringParsingError>,
}

impl DocStringParser {
    fn new(text: Vec<u8>, location: ffi::WireSourceLocation) -> Self {
        Self {
            text,
            location,
            doc_tags: Vec::new(),
            last_tag: None,
            errors: Vec::new(),
        }
    }

    fn parse(mut self) -> ffi::WireDocStringResult {
        self.last_tag = None;
        self.doc_tags.clear();

        let mut curr_pos = 0;
        let end = self.text.len();

        while curr_pos != end {
            let tag_pos = find_byte(&self.text, curr_pos, end, b'@');
            let nl_pos = find_byte(&self.text, curr_pos, end, b'\n');

            if tag_pos != end && tag_pos < nl_pos {
                let tag_name_end_pos = first_whitespace_or_newline(&self.text, tag_pos, end);
                let tag_name = self.text[tag_pos + 1..tag_name_end_pos].to_vec();
                let tag_data_pos = if tag_name_end_pos != end {
                    tag_name_end_pos + 1
                } else {
                    tag_name_end_pos
                };
                curr_pos = self.parse_doc_tag(tag_data_pos, end, &tag_name);
            } else if self.last_tag.is_some() {
                curr_pos = self.parse_doc_tag_line(curr_pos, end, true);
            } else if curr_pos != end {
                if curr_pos == 0 {
                    curr_pos = self.parse_doc_tag(curr_pos, end, b"notice");
                    continue;
                } else if nl_pos == end {
                    break;
                }

                curr_pos = nl_pos + 1;
            }
        }

        self.into_wire_result()
    }

    fn parse_doc_tag_line(&mut self, mut pos: usize, end: usize, appending: bool) -> usize {
        let last_tag = self.last_tag.expect("parseDocTagLine requires m_lastTag");
        let nl_pos = find_byte(&self.text, pos, end, b'\n');

        if appending && pos != end && self.text[pos] != b' ' && self.text[pos] != b'\t' {
            self.doc_tags[last_tag].1.content.push(b' ');
        } else if !appending {
            pos = skip_whitespace(&self.text, pos, end);
        }

        self.doc_tags[last_tag]
            .1
            .content
            .extend_from_slice(&self.text[pos..nl_pos]);

        skip_line_or_eos(nl_pos, end)
    }

    fn parse_doc_tag_param(&mut self, pos: usize, end: usize) -> usize {
        let name_start_pos = skip_whitespace(&self.text, pos, end);
        if name_start_pos == end {
            self.error(ERROR_NO_PARAM_NAME, b"No param name given".to_vec());
            return end;
        }

        let name_end_pos = first_non_identifier(&self.text, name_start_pos, end);
        let param_name = self.text[name_start_pos..name_end_pos].to_vec();

        let desc_start_pos = skip_whitespace(&self.text, name_end_pos, end);
        let nl_pos = find_byte(&self.text, desc_start_pos, end, b'\n');

        if desc_start_pos == nl_pos {
            let mut message = b"No description given for param ".to_vec();
            message.extend_from_slice(&param_name);
            self.error(ERROR_NO_PARAM_DESCRIPTION, message);
            return end;
        }

        let param_desc = self.text[desc_start_pos..nl_pos].to_vec();
        self.new_tag(b"param".to_vec());
        let last_tag = self.last_tag.expect("newTag sets m_lastTag");
        self.doc_tags[last_tag].1.param_name = param_name;
        self.doc_tags[last_tag].1.content = param_desc;

        skip_line_or_eos(nl_pos, end)
    }

    fn parse_doc_tag(&mut self, pos: usize, end: usize, tag: &[u8]) -> usize {
        if self.last_tag.is_none() || !tag.is_empty() {
            if tag == b"param" {
                self.parse_doc_tag_param(pos, end)
            } else {
                self.new_tag(tag.to_vec());
                self.parse_doc_tag_line(pos, end, false)
            }
        } else {
            self.parse_doc_tag_line(pos, end, true)
        }
    }

    fn new_tag(&mut self, tag_name: Vec<u8>) {
        self.doc_tags.push((tag_name, DocTag::default()));
        self.last_tag = Some(self.doc_tags.len() - 1);
    }

    fn error(&mut self, error_id: u32, message: Vec<u8>) {
        self.errors.push(DocStringParsingError {
            error_id,
            location: self.location.clone(),
            message,
        });
    }

    fn into_wire_result(mut self) -> ffi::WireDocStringResult {
        self.doc_tags.sort_by_key(|(name, _)| name.clone());

        ffi::WireDocStringResult {
            tags: self
                .doc_tags
                .into_iter()
                .map(|(name, tag)| ffi::WireDocTag {
                    name: ffi::WireString { bytes: name },
                    content: ffi::WireString { bytes: tag.content },
                    param_name: ffi::WireString {
                        bytes: tag.param_name,
                    },
                })
                .collect(),
            errors: self
                .errors
                .into_iter()
                .map(|error| ffi::WireDocStringError {
                    error_id: error.error_id,
                    location: error.location,
                    message: ffi::WireString {
                        bytes: error.message,
                    },
                })
                .collect(),
        }
    }
}

pub fn parse_doc_string(
    text: ffi::WireString,
    location: ffi::WireSourceLocation,
) -> ffi::WireDocStringResult {
    DocStringParser::new(text.bytes, location).parse()
}

fn skip_line_or_eos(nl_pos: usize, end: usize) -> usize {
    if nl_pos == end {
        end
    } else {
        nl_pos + 1
    }
}

fn first_non_identifier(text: &[u8], pos: usize, end: usize) -> usize {
    let mut curr_pos = pos;
    if is_identifier_start(text[curr_pos]) {
        curr_pos += 1;
        while curr_pos != end && is_identifier_part(text[curr_pos]) {
            curr_pos += 1;
        }
    }
    curr_pos
}

fn first_whitespace_or_newline(text: &[u8], pos: usize, end: usize) -> usize {
    let mut curr_pos = pos;
    while curr_pos != end
        && text[curr_pos] != b' '
        && text[curr_pos] != b'\t'
        && text[curr_pos] != b'\n'
    {
        curr_pos += 1;
    }
    curr_pos
}

fn skip_whitespace(text: &[u8], pos: usize, end: usize) -> usize {
    let mut curr_pos = pos;
    while curr_pos != end && (text[curr_pos] == b' ' || text[curr_pos] == b'\t') {
        curr_pos += 1;
    }
    curr_pos
}

fn find_byte(text: &[u8], pos: usize, end: usize, needle: u8) -> usize {
    let mut curr_pos = pos;
    while curr_pos != end && text[curr_pos] != needle {
        curr_pos += 1;
    }
    curr_pos
}

fn is_decimal_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte == b'$' || byte.is_ascii_lowercase() || byte.is_ascii_uppercase()
}

fn is_identifier_part(byte: u8) -> bool {
    is_identifier_start(byte) || is_decimal_digit(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location() -> ffi::WireSourceLocation {
        ffi::WireSourceLocation {
            start: 1,
            end: 2,
            source_id: 3,
        }
    }

    fn parse(text: &[u8]) -> ffi::WireDocStringResult {
        parse_doc_string(
            ffi::WireString {
                bytes: text.to_vec(),
            },
            location(),
        )
    }

    fn assert_is_test_location(location: &ffi::WireSourceLocation) {
        assert_eq!(location.start, 1);
        assert_eq!(location.end, 2);
        assert_eq!(location.source_id, 3);
    }

    #[test]
    fn parses_leading_notice() {
        let result = parse(b"hello\nworld");

        assert!(result.errors.is_empty());
        assert_eq!(result.tags.len(), 1);
        assert_eq!(result.tags[0].name.bytes, b"notice");
        assert_eq!(result.tags[0].content.bytes, b"hello world");
        assert!(result.tags[0].param_name.bytes.is_empty());
    }

    #[test]
    fn parses_param_tag() {
        let result = parse(b"@param value description");

        assert!(result.errors.is_empty());
        assert_eq!(result.tags.len(), 1);
        assert_eq!(result.tags[0].name.bytes, b"param");
        assert_eq!(result.tags[0].param_name.bytes, b"value");
        assert_eq!(result.tags[0].content.bytes, b"description");
    }

    #[test]
    fn parses_empty_tag_name() {
        let result = parse(b"@ empty");

        assert!(result.errors.is_empty());
        assert_eq!(result.tags.len(), 1);
        assert!(result.tags[0].name.bytes.is_empty());
        assert_eq!(result.tags[0].content.bytes, b"empty");
        assert!(result.tags[0].param_name.bytes.is_empty());
    }

    #[test]
    fn reports_param_without_name() {
        let result = parse(b"@param");

        assert!(result.tags.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].error_id, ERROR_NO_PARAM_NAME);
        assert_eq!(result.errors[0].message.bytes, b"No param name given");
        assert_is_test_location(&result.errors[0].location);
    }

    #[test]
    fn reports_param_without_description() {
        let result = parse(b"@param value");

        assert!(result.tags.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].error_id, ERROR_NO_PARAM_DESCRIPTION);
        assert_eq!(
            result.errors[0].message.bytes,
            b"No description given for param value"
        );
        assert_is_test_location(&result.errors[0].location);
    }

    #[test]
    fn preserves_continuation_line_spacing() {
        let result = parse(b"@notice first\nsecond\n third");

        assert!(result.errors.is_empty());
        assert_eq!(result.tags.len(), 1);
        assert_eq!(result.tags[0].name.bytes, b"notice");
        assert_eq!(result.tags[0].content.bytes, b"first second third");
    }

    #[test]
    fn sorts_tags_like_multimap_and_preserves_equal_key_order() {
        let result = parse(b"@z first\n@a only\n@z second");

        assert!(result.errors.is_empty());
        assert_eq!(result.tags.len(), 3);
        assert_eq!(result.tags[0].name.bytes, b"a");
        assert_eq!(result.tags[0].content.bytes, b"only");
        assert_eq!(result.tags[1].name.bytes, b"z");
        assert_eq!(result.tags[1].content.bytes, b"first");
        assert_eq!(result.tags[2].name.bytes, b"z");
        assert_eq!(result.tags[2].content.bytes, b"second");
    }
}
