//! Allocation-free RichTags filtering and the installed Mono StringInfo rules.
//! Unicode/category and invariant-regex case classes come from the owning BCL;
//! this module never substitutes a newer Unicode segmentation database.
use std::slice;

const COMBINING: u8 = 1;
const BASE: u8 = 2;
const WHITE: u8 = 4;
const NAMES: &[&[u8]] = &[
    b"b",
    b"i",
    b"size",
    b"color",
    b"material",
    b"quad",
    b"sprite",
    b"font",
    b"alpha",
    b"u",
    b"s",
    b"mark",
    b"link",
    b"align",
    b"voffset",
    b"space",
    b"cspace",
    b"mspace",
    b"nobr",
];

struct Tables<'a> {
    bmp: &'a [u8],
    letters: &'a [u8],
    ends: &'a [u32],
    supplementary: &'a [u8],
}

impl Tables<'_> {
    fn flags(&self, scalar: u32) -> u8 {
        if scalar <= 0xffff {
            self.bmp[scalar as usize]
        } else {
            let at = self.ends.partition_point(|&end| end < scalar);
            self.supplementary[at]
        }
    }
    fn tag_end(&self, source: &[u16], start: usize) -> Option<usize> {
        let mut at = start + 1;
        if source.get(at) == Some(&(b'/' as u16)) {
            at += 1;
        }
        for name in NAMES {
            if name.len() > source.len().saturating_sub(at) {
                continue;
            }
            if !source[at..at + name.len()]
                .iter()
                .zip(name.iter())
                .all(|(&unit, &letter)| self.letters[unit as usize] == letter)
            {
                continue;
            }
            let mut tail = at + name.len();
            match source.get(tail).copied() {
                Some(unit) if unit == b'>' as u16 => return Some(tail + 1),
                Some(unit) if unit == b'=' as u16 || self.bmp[unit as usize] & WHITE != 0 => {
                    tail += 1;
                    while let Some(&unit) = source.get(tail) {
                        if unit == b'>' as u16 {
                            return Some(tail + 1);
                        }
                        if unit == b'<' as u16 {
                            break;
                        }
                        tail += 1;
                    }
                }
                _ => {}
            }
        }
        None
    }
}

struct Filter<'a> {
    source: &'a [u16],
    tables: &'a Tables<'a>,
    at: usize,
    pending: Option<u16>,
}
impl Filter<'_> {
    fn next_unit(&mut self) -> Option<u16> {
        if let Some(unit) = self.pending.take() {
            return Some(unit);
        }
        while let Some(&unit) = self.source.get(self.at) {
            if unit == b'<' as u16 {
                if let Some(end) = self.tables.tag_end(self.source, self.at) {
                    self.at = end;
                    continue;
                }
            }
            self.at += 1;
            return Some(unit);
        }
        None
    }
    fn next_flags(&mut self) -> Option<u8> {
        let first = self.next_unit()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            match self.next_unit() {
                Some(second) if (0xdc00..=0xdfff).contains(&second) => {
                    0x10000 + ((first as u32 - 0xd800) << 10) + second as u32 - 0xdc00
                }
                other => {
                    self.pending = other;
                    first as u32
                }
            }
        } else {
            first as u32
        };
        Some(self.tables.flags(scalar))
    }
}

fn count(source: &[u16], tables: &Tables<'_>) -> i32 {
    let mut input = Filter {
        source,
        tables,
        at: 0,
        pending: None,
    };
    let mut current = input.next_flags();
    let mut count = 0;
    while let Some(flags) = current {
        let mut visible = flags & WHITE == 0;
        current = input.next_flags();
        // This is Mono's pre-UAX29 StringInfo grouping: combining characters
        // join only a noncombining, assigned, non-control/format/surrogate base.
        if flags & BASE != 0 {
            while current.is_some_and(|next| next & COMBINING != 0) {
                visible = true; // A mark makes an otherwise whitespace group visible.
                current = input.next_flags();
            }
        }
        if visible {
            count += 1;
        }
    }
    count
}

/// Counts visible text elements after the Host's exact, nonrecursive RichTags
/// replacement. Returns -1 for invalid ABI arguments; no pointers are retained.
/// Zero length accepts null pointers and returns 0. All lengths count elements.
///
/// # Safety
/// Nonempty text addresses `length` initialized UTF-16 units. BMP and letter
/// tables each address 65,536 bytes. The supplementary arrays address `ranges`
/// u32 ends / u8 flags: increasing inclusive ends cover U+10000..U+10FFFF.
/// These caller-owned tables are generated from its BCL once and remain fixed
/// for the call. Buffers must be readable, correctly aligned, live, and not
/// concurrently mutated. Non-null invalid addresses cannot be validated here.
#[no_mangle]
pub unsafe extern "C" fn lom_text_count_v1(
    text: *const u16,
    length: i32,
    bmp: *const u8,
    letters: *const u8,
    range_ends: *const u32,
    range_flags: *const u8,
    ranges: i32,
) -> i32 {
    if length < 0 || length as usize > isize::MAX as usize / 2 {
        return -1;
    }
    if length == 0 {
        return 0;
    }
    if text.is_null()
        || !text.is_aligned()
        || bmp.is_null()
        || letters.is_null()
        || range_ends.is_null()
        || !range_ends.is_aligned()
        || range_flags.is_null()
        || ranges < 1
        || ranges > 0x100000
    {
        return -1;
    }
    let ends = unsafe { slice::from_raw_parts(range_ends, ranges as usize) };
    if ends[ends.len() - 1] != 0x10ffff {
        return -1;
    }
    let tables = Tables {
        bmp: unsafe { slice::from_raw_parts(bmp, 65536) },
        letters: unsafe { slice::from_raw_parts(letters, 65536) },
        ends,
        supplementary: unsafe { slice::from_raw_parts(range_flags, ranges as usize) },
    };
    count(
        unsafe { slice::from_raw_parts(text, length as usize) },
        &tables,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(text: &str) -> i32 {
        let mut bmp = vec![BASE; 65536];
        for unit in [b' ' as usize, b'\n' as usize] {
            bmp[unit] |= WHITE;
        }
        bmp[0x301] = COMBINING;
        for unit in 0xd800..=0xdfff {
            bmp[unit] = 0;
        }
        let mut letters = vec![0u8; 65536];
        for unit in b'a'..=b'z' {
            letters[unit as usize] = unit;
            letters[(unit - 32) as usize] = unit;
        }
        let tables = Tables {
            bmp: &bmp,
            letters: &letters,
            ends: &[0x10ffff],
            supplementary: &[BASE],
        };
        count(&text.encode_utf16().collect::<Vec<_>>(), &tables)
    }
    #[test]
    fn tags_and_combining_sequences_keep_nonrecursive_filtering() {
        assert_eq!(run("<size=40>你</size> <COLOR=#fff>好</COLOR>\n😀"), 3);
        assert_eq!(run("e<b>\u{301}</b>"), 1);
        assert_eq!(run(" <b>\u{301}</b>"), 1);
        assert_eq!(run("\u{301}\u{301}"), 2);
        assert_eq!(run("<b<color>></color>"), 3);
        assert_eq!(run("<unknown>x</unknown>"), 20);
    }
    #[test]
    fn abi_rejects_invalid_arguments() {
        unsafe {
            assert_eq!(
                lom_text_count_v1(
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0
                ),
                0
            );
            assert_eq!(
                lom_text_count_v1(
                    std::ptr::null(),
                    -1,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0
                ),
                -1
            );
            assert_eq!(
                lom_text_count_v1(
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0
                ),
                -1
            );
        }
    }
}
