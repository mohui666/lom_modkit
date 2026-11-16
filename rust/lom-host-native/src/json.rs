//! MiniJson's UTF-16 structure/string scanner, not a stricter JSON grammar.
//! Exact i32 integers and four ASCII-hex-unit escapes are decoded in this scan.
//! Other numeric/HexNumber conversions remain ordered validations for the owning
//! runtime, preserving its rounding, unusual accepted input and first error.
use std::{ptr, slice};

pub(crate) const NULL: i32 = 0;
pub(crate) const FALSE: i32 = 1;
pub(crate) const TRUE: i32 = 2;
pub(crate) const NUMBER: i32 = 3;
pub(crate) const STRING: i32 = 4;
pub(crate) const ARRAY: i32 = 5;
pub(crate) const OBJECT: i32 = 6;
pub(crate) const VALIDATE_NUMBER: i32 = 0;
pub(crate) const VALIDATE_HEX: i32 = 1;

/// Preorder tape. `next` skips the entire subtree; container `len` counts direct
/// children. String locations >= 0 refer to input UTF-16; negative locations
/// refer to `text[-1-location]`. `key_len == -1` means no object property key.
/// `number` holds an exact i32 value, or is filled by the runtime's double.TryParse.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub kind: i32,
    pub start: i32,
    pub len: i32,
    pub next: i32,
    pub key_start: i32,
    pub key_len: i32,
    pub number: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Validation {
    pub kind: i32,
    pub start: i32,
    pub len: i32,
    /// Number node index, or UTF-16 arena index for a HexNumber escape.
    pub target: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub code: i32,
    pub position: i32,
    pub detail: i32,
}

#[derive(Default, Debug)]
pub(crate) struct Document {
    pub nodes: Vec<Node>,
    pub text: Vec<u16>,
    pub validations: Vec<Validation>,
    pub error: Option<ParseError>,
}

/// Current Unity Mono char.IsWhiteSpace(char), checked over all 65536 code
/// units. Surrogates, U+180E and U+FEFF are deliberately not whitespace.
pub(crate) fn is_whitespace(unit: u16) -> bool {
    matches!(unit, 0x0009..=0x000d | 0x0020 | 0x0085 | 0x00a0 | 0x1680 |
        0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000)
}

struct Parser<'a> {
    input: &'a [u16],
    position: usize,
    document: Document,
}

// Every i32 has an exact binary64 representation. Leave negative zero to the
// runtime: old Mono and current .NET differ in its parsed sign bit. Nonstandard
// number spellings and all rounding-sensitive values also stay with that parser.
fn exact_i32(units: &[u16]) -> Option<f64> {
    let negative = units.first() == Some(&0x2d);
    let digits = if negative { units.get(1..)? } else { units };
    if digits.is_empty() || digits.len() > 10 {
        return None;
    }
    let limit = if negative {
        2_147_483_648u64
    } else {
        2_147_483_647u64
    };
    let mut value = 0u64;
    for &unit in digits {
        let digit = unit.wrapping_sub(0x30);
        if digit > 9 {
            return None;
        }
        value = value * 10 + u64::from(digit);
        if value > limit {
            return None;
        }
    }
    if negative && value == 0 {
        None
    } else {
        Some(if negative {
            -(value as f64)
        } else {
            value as f64
        })
    }
}

fn exact_hex4(units: &[u16]) -> Option<u16> {
    let mut value = 0u16;
    for &unit in units {
        let digit = match unit {
            0x30..=0x39 => unit - 0x30,
            0x41..=0x46 => unit - 0x41 + 10,
            0x61..=0x66 => unit - 0x61 + 10,
            _ => return None,
        };
        value = (value << 4) | digit;
    }
    Some(value)
}

impl Parser<'_> {
    fn error(&self, code: i32) -> ParseError {
        ParseError {
            code,
            position: self.position as i32,
            detail: 0,
        }
    }

    fn whitespace(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(|&unit| is_whitespace(unit))
        {
            self.position += 1;
        }
    }

    fn node(&mut self, kind: i32, start: i32, len: i32) -> usize {
        let index = self.document.nodes.len();
        self.document.nodes.push(Node {
            kind,
            start,
            len,
            next: index as i32 + 1,
            key_start: 0,
            key_len: -1,
            number: 0.0,
        });
        index
    }

    fn value(&mut self, depth: usize) -> Result<usize, ParseError> {
        self.whitespace();
        let Some(&unit) = self.input.get(self.position) else {
            return Err(self.error(1));
        };
        match unit {
            0x007b => self.container(depth + 1, true),
            0x005b => self.container(depth + 1, false),
            0x0022 => {
                let (start, len) = self.string()?;
                Ok(self.node(STRING, start, len))
            }
            0x0074 => self.literal(b"true", TRUE),
            0x0066 => self.literal(b"false", FALSE),
            0x006e => self.literal(b"null", NULL),
            _ => {
                let start = self.position;
                while self.input.get(self.position).is_some_and(|&unit| {
                    matches!(
                        unit,
                        0x0030..=0x0039 | 0x002b | 0x002d | 0x002e | 0x0065 | 0x0045
                    )
                }) {
                    self.position += 1;
                }
                let len = (self.position - start) as i32;
                let index = self.node(NUMBER, start as i32, len);
                if let Some(value) = exact_i32(&self.input[start..self.position]) {
                    self.document.nodes[index].number = value;
                } else {
                    self.document.validations.push(Validation {
                        kind: VALIDATE_NUMBER,
                        start: start as i32,
                        len,
                        target: index as i32,
                    });
                }
                // Even an empty/invalid number becomes an ordered validation.
                // A later structural error must not mask its earlier failure.
                Ok(index)
            }
        }
    }

    fn literal(&mut self, literal: &[u8], kind: i32) -> Result<usize, ParseError> {
        if self
            .input
            .get(self.position..self.position + literal.len())
            .is_none_or(|units| {
                !units
                    .iter()
                    .zip(literal)
                    .all(|(&unit, &byte)| unit == u16::from(byte))
            })
        {
            return Err(self.error(12));
        }
        self.position += literal.len();
        Ok(self.node(kind, 0, 0))
    }

    fn container(&mut self, depth: usize, object: bool) -> Result<usize, ParseError> {
        if depth > 128 {
            return Err(self.error(11));
        }
        let index = self.node(if object { OBJECT } else { ARRAY }, 0, 0);
        self.position += 1;
        self.whitespace();
        let close = if object { 0x007d } else { 0x005d };
        if self.input.get(self.position) == Some(&close) {
            self.position += 1;
            return Ok(index);
        }
        let mut count = 0;
        loop {
            let key = if object {
                self.whitespace();
                if self.input.get(self.position) != Some(&0x0022) {
                    return Err(self.error(2));
                }
                let key = self.string()?;
                self.whitespace();
                if self.input.get(self.position) != Some(&0x003a) {
                    return Err(self.error(3));
                }
                self.position += 1;
                Some(key)
            } else {
                None
            };
            let child = self.value(depth)?;
            if let Some((start, len)) = key {
                self.document.nodes[child].key_start = start;
                self.document.nodes[child].key_len = len;
            }
            count += 1;
            self.whitespace();
            match self.input.get(self.position) {
                None => return Err(self.error(if object { 4 } else { 6 })),
                Some(&0x002c) => self.position += 1,
                Some(&unit) if unit == close => {
                    self.position += 1;
                    self.document.nodes[index].len = count;
                    self.document.nodes[index].next = self.document.nodes.len() as i32;
                    return Ok(index);
                }
                _ => return Err(self.error(if object { 5 } else { 7 })),
            }
        }
    }

    fn string(&mut self) -> Result<(i32, i32), ParseError> {
        self.position += 1;
        let start = self.position;
        let mut segment = start;
        let mut decoded = None;
        while self.position < self.input.len() {
            let at = self.position;
            let unit = self.input[at];
            self.position += 1;
            if unit == 0x0022 {
                return if let Some(begin) = decoded {
                    self.document
                        .text
                        .extend_from_slice(&self.input[segment..at]);
                    Ok((-1 - begin as i32, (self.document.text.len() - begin) as i32))
                } else {
                    Ok((start as i32, (at - start) as i32))
                };
            }
            if unit != 0x005c {
                continue;
            }
            decoded.get_or_insert(self.document.text.len());
            self.document
                .text
                .extend_from_slice(&self.input[segment..at]);
            let Some(&escape) = self.input.get(self.position) else {
                break;
            };
            self.position += 1;
            let value = match escape {
                0x0022 | 0x005c | 0x002f => escape,
                0x0062 => 0x0008,
                0x0066 => 0x000c,
                0x006e => 0x000a,
                0x0072 => 0x000d,
                0x0074 => 0x0009,
                0x0075 => {
                    if self.position + 4 > self.input.len() {
                        return Err(self.error(8));
                    }
                    let value = exact_hex4(&self.input[self.position..self.position + 4]);
                    if value.is_none() {
                        self.document.validations.push(Validation {
                            kind: VALIDATE_HEX,
                            start: self.position as i32,
                            len: 4,
                            target: self.document.text.len() as i32,
                        });
                    }
                    self.position += 4;
                    value.unwrap_or(0) // Unusual spellings use runtime HexNumber.
                }
                _ => {
                    return Err(ParseError {
                        detail: i32::from(escape),
                        ..self.error(9)
                    })
                }
            };
            self.document.text.push(value);
            segment = self.position;
        }
        Err(self.error(10))
    }
}

/// Scan without normalizing/replacing any UTF-16 code unit. Objects retain
/// source order and duplicate keys; the managed Dictionary applies last-write-
/// wins. Validation events precede `error` and must run in their recorded order
/// before an error is raised or the tape is used as a fully validated document.
pub(crate) fn parse_document(input: &[u16]) -> Document {
    let mut parser = Parser {
        input,
        position: 0,
        document: Document::default(),
    };
    let outcome = parser.value(0).and_then(|_| {
        parser.whitespace();
        if parser.position == input.len() {
            Ok(())
        } else {
            Err(parser.error(13))
        }
    });
    parser.document.error = outcome.err();
    parser.document
}

/// Three pointers first, then six i32 fields. C layout is 48 bytes on x64,
/// 36 bytes on x86; Node/Validation layouts are 32/16 bytes on both platforms.
#[repr(C)]
pub struct JsonResult {
    nodes: *mut Node,
    text: *mut u16,
    validations: *const Validation,
    node_count: i32,
    text_count: i32,
    validation_count: i32,
    error_code: i32,
    error_position: i32,
    error_detail: i32,
}

#[repr(C)]
struct OwnedResult {
    header: JsonResult,
    _document: Document,
}

/// Batch parse a borrowed, pinned UTF-16 string, with no callbacks or encoding
/// conversion. A non-null result, including a JSON syntax error, must be freed
/// exactly once with lom_json_free_v1. Nodes/text may be written only to apply
/// the recorded numeric/HexNumber validation results. The source is unchanged.
/// A null result denotes invalid ABI arguments, never valid JSON null.
///
/// # Safety
/// Nonempty input must be aligned/readable for `length` UTF-16 code units in one
/// allocation without address wraparound and stay alive while borrowed strings
/// are materialized. Result pointers remain valid only until the matching free.
#[no_mangle]
pub unsafe extern "C" fn lom_json_parse_utf16_v1(
    source: *const u16,
    length: i32,
) -> *mut JsonResult {
    if length < 0
        || length as usize > isize::MAX as usize / 2
        || length != 0 && (source.is_null() || !source.is_aligned())
    {
        return ptr::null_mut();
    }
    let input = if length == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(source, length as usize) }
    };
    let mut document = parse_document(input);
    let error = document.error.unwrap_or(ParseError {
        code: 0,
        position: 0,
        detail: 0,
    });
    let header = JsonResult {
        nodes: document.nodes.as_mut_ptr(),
        text: document.text.as_mut_ptr(),
        validations: document.validations.as_ptr(),
        node_count: document.nodes.len() as i32,
        text_count: document.text.len() as i32,
        validation_count: document.validations.len() as i32,
        error_code: error.code,
        error_position: error.position,
        error_detail: error.detail,
    };
    Box::into_raw(Box::new(OwnedResult {
        header,
        _document: document,
    }))
    .cast()
}

/// Release a result from lom_json_parse_utf16_v1; null is a no-op.
/// # Safety
/// A non-null pointer must be that exact, still-live result, freed only once.
#[no_mangle]
pub unsafe extern "C" fn lom_json_free_v1(result: *mut JsonResult) {
    if !result.is_null() {
        drop(unsafe { Box::from_raw(result.cast::<OwnedResult>()) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> Document {
        parse_document(&text.encode_utf16().collect::<Vec<_>>())
    }

    #[test]
    fn json_tape_preserves_duplicates_lax_numbers_and_ordered_hex_validation() {
        let document = parse(r#"{"a":+1,"a":[.5,01,"x\u 041\n"]}"#);
        assert_eq!(document.error, None);
        assert_eq!(document.nodes[0].kind, OBJECT);
        assert_eq!(document.nodes[0].len, 2);
        assert_eq!(document.nodes[0].next, 6);
        assert_eq!(document.nodes[1].kind, NUMBER);
        assert_eq!(document.nodes[2].kind, ARRAY);
        assert_eq!(
            document
                .validations
                .iter()
                .map(|v| v.kind)
                .collect::<Vec<_>>(),
            [VALIDATE_NUMBER, VALIDATE_NUMBER, VALIDATE_HEX]
        );
        assert_eq!(document.nodes[4].number, 1.0);
        assert_eq!(document.text, [b'x' as u16, 0, b'\n' as u16]);
    }

    #[test]
    fn json_structure_error_keeps_earlier_runtime_validation_and_exact_position() {
        let document = parse(r#"[1e, "\uGGGG", false x]"#);
        assert_eq!(document.validations.len(), 2);
        assert_eq!(document.validations[0].start, 1);
        assert_eq!(document.validations[1].start, 8);
        assert_eq!(
            document.error.unwrap(),
            ParseError {
                code: 7,
                position: 21,
                detail: 0
            }
        );
        assert_eq!(parse("[1,]").validations[0].len, 0);
        assert_eq!(parse("{\"a\":1,}").error.unwrap().code, 2);
    }

    #[test]
    fn exact_numbers_and_hex_do_not_allocate_runtime_validation_events() {
        let document = parse(r#"[-2147483648,2147483647,0009,"\uD800\udc00\u0000"]"#);
        assert_eq!(document.error, None);
        assert!(document.validations.is_empty());
        assert_eq!(document.nodes[1].number, -2147483648.0);
        assert_eq!(document.nodes[2].number, 2147483647.0);
        assert_eq!(document.nodes[3].number, 9.0);
        assert_eq!(document.text, [0xd800, 0xdc00, 0]);
        let document = parse("[-0,-00,2147483648,-2147483649,+1,.5,1e3,1e]");
        assert_eq!(document.validations.len(), 8);
        assert!(document
            .validations
            .iter()
            .all(|v| v.kind == VALIDATE_NUMBER));
    }

    #[test]
    fn json_utf16_depth_and_ffi_lifetime_keep_the_old_boundaries() {
        let source = [0x22u16, 0xd800, 0, 0xdc00, 0x22];
        let document = parse_document(&source);
        assert_eq!(document.nodes[0].start, 1);
        assert_eq!(document.nodes[0].len, 3);
        assert!(document.text.is_empty());
        assert_eq!(
            parse(&("[".repeat(128) + "0" + &"]".repeat(128))).error,
            None
        );
        assert_eq!(
            parse(&("[".repeat(129) + "0" + &"]".repeat(129)))
                .error
                .unwrap()
                .code,
            11
        );
        assert!(is_whitespace(0x0085) && is_whitespace(0x3000));
        assert!(!is_whitespace(0x180e) && !is_whitespace(0xfeff));
        unsafe {
            assert!(lom_json_parse_utf16_v1(ptr::null(), 1).is_null());
            let result = lom_json_parse_utf16_v1(source.as_ptr(), source.len() as i32);
            assert_eq!((*result).error_code, 0);
            assert_eq!((*result).node_count, 1);
            lom_json_free_v1(result);
            let empty = lom_json_parse_utf16_v1(ptr::null(), 0);
            assert_eq!((*empty).error_code, 1);
            lom_json_free_v1(empty);
            lom_json_free_v1(ptr::null_mut());
        }
        assert_eq!(std::mem::size_of::<Node>(), 32);
        assert_eq!(std::mem::size_of::<Validation>(), 16);
        assert_eq!(
            std::mem::size_of::<JsonResult>(),
            if cfg!(target_pointer_width = "64") {
                48
            } else {
                36
            }
        );
    }
}
