//! Strict UTF-8 event records without whole intermediate byte arrays. Base64
//! accepts Convert.FromBase64String's four whitespace units and ignored pad bits.
use std::slice;

const MAX_BYTES: usize = 128 * 1024;
const MAX_ENCODED: usize = 180_000;
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn valid_buffer(pointer: *const u16, length: i32) -> bool {
    length >= 0
        && length as usize <= isize::MAX as usize / 2
        && (length == 0 || !pointer.is_null() && pointer.is_aligned())
}
struct Output<'a> {
    buffer: Option<&'a mut [u16]>,
    length: usize,
    overflow: bool,
}
impl Output<'_> {
    fn unit(&mut self, unit: u16) {
        if let Some(buffer) = &mut self.buffer {
            if let Some(at) = buffer.get_mut(self.length) {
                *at = unit;
            } else {
                self.overflow = true;
            }
        }
        self.length += 1;
    }
    fn ascii_three(&mut self, a: u8, b: u8, c: u8) {
        if let Some(buffer) = &mut self.buffer {
            if let Some(output) = buffer.get_mut(self.length..self.length + 3) {
                output.copy_from_slice(&[a as u16, b as u16, c as u16]);
            } else {
                self.overflow = true;
            }
        }
        self.length += 3;
    }
}
struct Base64Writer<'a> {
    output: Output<'a>,
    pending: u32,
    count: usize,
}
impl Base64Writer<'_> {
    fn byte(&mut self, byte: u8) {
        self.pending = (self.pending << 8) | byte as u32;
        self.count += 1;
        if self.count == 3 {
            for shift in [18, 12, 6, 0] {
                self.output
                    .unit(BASE64[((self.pending >> shift) & 63) as usize] as u16);
            }
            self.count = 0;
            self.pending = 0;
        }
    }
    fn finish(&mut self) {
        if self.count == 1 {
            self.output
                .unit(BASE64[(self.pending >> 2) as usize] as u16);
            self.output
                .unit(BASE64[((self.pending & 3) << 4) as usize] as u16);
            self.output.unit(61);
            self.output.unit(61);
        } else if self.count == 2 {
            self.output
                .unit(BASE64[(self.pending >> 10) as usize] as u16);
            self.output
                .unit(BASE64[((self.pending >> 4) & 63) as usize] as u16);
            self.output
                .unit(BASE64[((self.pending & 15) << 2) as usize] as u16);
            self.output.unit(61);
        }
    }
}
fn encode(input: &[u16], output: Output<'_>) -> Result<usize, u32> {
    let write = output.buffer.is_some();
    let mut writer = Base64Writer {
        output,
        pending: 0,
        count: 0,
    };
    let (mut at, mut bytes) = (0, 0usize);
    while at < input.len() {
        let mut scalar = input[at] as u32;
        at += 1;
        if (0xd800..=0xdbff).contains(&scalar) {
            if at == input.len() || !(0xdc00..=0xdfff).contains(&input[at]) {
                return Err(2);
            }
            scalar = 0x10000 + ((scalar - 0xd800) << 10) + (input[at] as u32 - 0xdc00);
            at += 1;
        } else if (0xdc00..=0xdfff).contains(&scalar) {
            return Err(2);
        }
        let size = if scalar < 0x80 {
            1
        } else if scalar < 0x800 {
            2
        } else if scalar < 0x10000 {
            3
        } else {
            4
        };
        bytes += size;
        // Even after exceeding the write limit, validate the entire UTF-16
        // input: the original GetBytes throws encoding errors before the cap.
        if write && bytes <= MAX_BYTES {
            match size {
                1 => writer.byte(scalar as u8),
                2 => {
                    writer.byte((0xc0 | (scalar >> 6)) as u8);
                    writer.byte((0x80 | (scalar & 63)) as u8);
                }
                3 => {
                    writer.byte((0xe0 | (scalar >> 12)) as u8);
                    writer.byte((0x80 | ((scalar >> 6) & 63)) as u8);
                    writer.byte((0x80 | (scalar & 63)) as u8);
                }
                _ => {
                    writer.byte((0xf0 | (scalar >> 18)) as u8);
                    writer.byte((0x80 | ((scalar >> 12) & 63)) as u8);
                    writer.byte((0x80 | ((scalar >> 6) & 63)) as u8);
                    writer.byte((0x80 | (scalar & 63)) as u8);
                }
            }
        }
    }
    if bytes > MAX_BYTES {
        return Err(4);
    }
    let length = bytes.div_ceil(3) * 4;
    if write {
        writer.finish();
        if writer.output.overflow || writer.output.length != length {
            return Err(0);
        }
    }
    Ok(length)
}

struct Utf8Writer<'a> {
    output: Output<'a>,
    scalar: u32,
    minimum: u32,
    remaining: u8,
    invalid: bool,
}
impl Utf8Writer<'_> {
    fn triple(&mut self, a: u8, b: u8, c: u8) {
        if self.invalid {
            return;
        }
        if self.remaining == 0 && (a | b | c) & 0x80 == 0 {
            self.output.ascii_three(a, b, c);
        } else {
            self.byte(a);
            self.byte(b);
            self.byte(c);
        }
    }
    fn byte(&mut self, byte: u8) {
        if self.invalid {
            return;
        }
        if self.remaining == 0 {
            match byte {
                0..=0x7f => self.output.unit(byte as u16),
                0xc2..=0xdf => {
                    self.scalar = (byte & 31) as u32;
                    self.minimum = 0x80;
                    self.remaining = 1;
                }
                0xe0..=0xef => {
                    self.scalar = (byte & 15) as u32;
                    self.minimum = 0x800;
                    self.remaining = 2;
                }
                0xf0..=0xf4 => {
                    self.scalar = (byte & 7) as u32;
                    self.minimum = 0x10000;
                    self.remaining = 3;
                }
                _ => self.invalid = true,
            }
        } else if byte & 0xc0 != 0x80 {
            self.invalid = true;
        } else {
            self.scalar = (self.scalar << 6) | (byte & 63) as u32;
            self.remaining -= 1;
            if self.remaining == 0 {
                let scalar = self.scalar;
                if scalar < self.minimum || scalar > 0x10ffff || (0xd800..=0xdfff).contains(&scalar)
                {
                    self.invalid = true;
                } else if scalar < 0x10000 {
                    self.output.unit(scalar as u16);
                } else {
                    let scalar = scalar - 0x10000;
                    self.output.unit((0xd800 + (scalar >> 10)) as u16);
                    self.output.unit((0xdc00 + (scalar & 1023)) as u16);
                }
            }
        }
    }
}
const fn digits() -> [u8; 128] {
    let mut result = [255u8; 128];
    let mut i = 0;
    while i < BASE64.len() {
        result[BASE64[i] as usize] = i as u8;
        i += 1;
    }
    result[61] = 64;
    result
}
const DIGITS: [u8; 128] = digits();
fn digit(unit: u16) -> u8 {
    DIGITS.get(unit as usize).copied().unwrap_or(255)
}
fn decode(input: &[u16], output: Output<'_>) -> Result<usize, u32> {
    if input.len() > MAX_ENCODED {
        return Err(5);
    }
    let mut writer = Utf8Writer {
        output,
        scalar: 0,
        minimum: 0,
        remaining: 0,
        invalid: false,
    };
    let (mut quartet, mut used, mut padded) = ([0u8; 4], 0, false);
    let mut at = 0;
    while at < input.len() {
        if used == 0 && !padded && at + 4 <= input.len() {
            let a = digit(input[at]);
            let b = digit(input[at + 1]);
            let c = digit(input[at + 2]);
            let d = digit(input[at + 3]);
            if (a | b | c | d) < 64 {
                writer.triple((a << 2) | (b >> 4), (b << 4) | (c >> 2), (c << 6) | d);
                at += 4;
                continue;
            }
        }
        let unit = input[at];
        at += 1;
        if matches!(unit, 9 | 10 | 13 | 32) {
            continue;
        }
        if padded {
            return Err(3);
        }
        let value = digit(unit);
        if value == 255 {
            return Err(3);
        }
        quartet[used] = value;
        used += 1;
        if used == 4 {
            if quartet[0] >= 64 || quartet[1] >= 64 {
                return Err(3);
            }
            writer.byte((quartet[0] << 2) | (quartet[1] >> 4));
            if quartet[2] == 64 {
                if quartet[3] != 64 {
                    return Err(3);
                }
                padded = true;
            } else {
                writer.byte((quartet[1] << 4) | (quartet[2] >> 2));
                if quartet[3] == 64 {
                    padded = true;
                } else {
                    writer.byte((quartet[2] << 6) | quartet[3]);
                }
            }
            used = 0;
        }
    }
    if used != 0 {
        return Err(3);
    }
    // FromBase64String validates the complete payload before strict UTF-8.
    // Keep scanning Base64 after a UTF-8 failure so a later syntax error wins.
    if writer.invalid || writer.remaining != 0 {
        return Err(2);
    }
    if writer.output.overflow {
        return Err(0);
    }
    Ok(writer.output.length)
}

unsafe fn transform(
    source: *const u16,
    length: i32,
    output: *mut u16,
    capacity: i32,
    written: *mut i32,
    is_encode: bool,
) -> u32 {
    if !valid_buffer(source, length)
        || !valid_buffer(output, capacity)
        || written.is_null()
        || !written.is_aligned()
        || output.is_null() && capacity != 0
    {
        return 0;
    }
    let input = if length == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(source, length as usize) }
    };
    let output = Output {
        buffer: if output.is_null() {
            None
        } else {
            Some(if capacity == 0 {
                &mut []
            } else {
                unsafe { slice::from_raw_parts_mut(output, capacity as usize) }
            })
        },
        length: 0,
        overflow: false,
    };
    let result = if is_encode {
        encode(input, output)
    } else {
        decode(input, output)
    };
    match result {
        Ok(length) => {
            unsafe {
                written.write(length as i32);
            }
            1
        }
        Err(status) => status,
    }
}
/// Direct UTF-16 to UTF-8 Base64. Query with null output/capacity zero, then
/// fill the final managed string. 1 success, 2 invalid UTF-16, 4 >128 KiB,
/// 0 invalid ABI/insufficient output. No allocation, retained pointers or I/O.
/// # Safety
/// Pinned source/output/written buffers cover declared lengths, do not alias
/// and stay alive for this call. Output is consumed only after status 1; a
/// rejected write may have filled a prefix. Query succeeds before C# allocates.
#[no_mangle]
pub unsafe extern "C" fn lom_state_encode_utf16_v1(
    source: *const u16,
    length: i32,
    output: *mut u16,
    capacity: i32,
    written: *mut i32,
) -> u32 {
    unsafe { transform(source, length, output, capacity, written, true) }
}
/// Direct Base64 to strict UTF-8 decoded UTF-16. 1 success, 2 invalid UTF-8,
/// 3 invalid Base64, 5 >180000 input units, 0 invalid ABI/output capacity.
/// The historical load limit is independent from the 128 KiB write limit.
/// # Safety
/// Same buffers/lifetime/query-before-write contract as the encode entry.
#[no_mangle]
pub unsafe extern "C" fn lom_state_decode_utf16_v1(
    source: *const u16,
    length: i32,
    output: *mut u16,
    capacity: i32,
    written: *mut i32,
) -> u32 {
    unsafe { transform(source, length, output, capacity, written, false) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(value: &str, encode: bool) -> Result<String, u32> {
        let source: Vec<u16> = value.encode_utf16().collect();
        let mut length = 0;
        let entry = if encode {
            lom_state_encode_utf16_v1
        } else {
            lom_state_decode_utf16_v1
        };
        let result = unsafe {
            entry(
                source.as_ptr(),
                source.len() as i32,
                std::ptr::null_mut(),
                0,
                &mut length,
            )
        };
        if result != 1 {
            return Err(result);
        }
        let mut output = vec![0u16; length as usize];
        assert_eq!(
            unsafe {
                entry(
                    source.as_ptr(),
                    source.len() as i32,
                    output.as_mut_ptr(),
                    length,
                    &mut length,
                )
            },
            1
        );
        Ok(String::from_utf16(&output).unwrap())
    }
    #[test]
    fn base64_old_whitespace_pad_bits_and_strict_utf8() {
        assert_eq!(run("正文😀\0", true).unwrap(), "5q2j5paH8J+YgAA=");
        assert_eq!(run(" 5q2j\r\n5paH8J+YgAA=\t", false).unwrap(), "正文😀\0");
        assert_eq!(run("Z h =\t=\r\n", false).unwrap(), "f");
        assert_eq!(run("Zm9=", false).unwrap(), "fo");
        assert_eq!(run("Zh==x", false), Err(3));
        assert_eq!(run("Zg==\u{a0}", false), Err(3));
        assert_eq!(run("wK8=", false), Err(2));
        assert_eq!(run("/w==?", false), Err(3));
        assert_eq!(run(&"x".repeat(MAX_BYTES + 1), true), Err(4));
        assert_eq!(run(&" ".repeat(MAX_ENCODED + 1), false), Err(5));
    }
}
