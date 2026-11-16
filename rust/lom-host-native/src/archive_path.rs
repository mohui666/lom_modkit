//! Exact lexical ZIP-path rules from the Host. Duplicate/case-folded ownership,
//! entry limits and file/directory conflicts remain with the managed archive.
use std::slice;

fn upper(c: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&c) {
        c - 32
    } else {
        c
    }
}

fn device(segment: &[u16]) -> bool {
    let stem = &segment[..segment
        .iter()
        .position(|&c| c == b'.' as u16)
        .unwrap_or(segment.len())];
    let same =
        |s: &[u8]| stem.len() == s.len() && stem.iter().zip(s).all(|(&a, &b)| upper(a) == b as u16);
    if same(b"CON") || same(b"PRN") || same(b"AUX") || same(b"NUL") {
        return true;
    }
    stem.len() == 4
        && (b'1' as u16..=b'9' as u16).contains(&stem[3])
        && [b"COM", b"LPT"].iter().any(|prefix| {
            stem[..3]
                .iter()
                .zip(prefix.iter())
                .all(|(&a, &b)| upper(a) == b as u16)
        })
}

pub(crate) fn validate(path: &[u16]) -> u32 {
    if path.is_empty() || path.contains(&0) {
        return 1;
    }
    if path.contains(&(b'\\' as u16)) {
        return 2;
    }
    if path[0] == b'/' as u16
        || path.len() >= 2
            && upper(path[0]) >= b'A' as u16
            && upper(path[0]) <= b'Z' as u16
            && path[1] == b':' as u16
    {
        return 3;
    }
    let body = if path.last() == Some(&(b'/' as u16)) {
        &path[..path.len() - 1]
    } else {
        path
    };
    if body.is_empty() {
        return 4;
    }
    for part in body.split(|&c| c == b'/' as u16) {
        if part.is_empty() || part == [b'.' as u16] || part == [b'.' as u16, b'.' as u16] {
            return 5;
        }
        if part.contains(&(b':' as u16)) {
            return 6;
        }
        if matches!(part.last(), Some(&c) if c == b'.' as u16 || c == b' ' as u16) {
            return 7;
        }
        if device(part) {
            return 8;
        }
    }
    0
}

/// Returns 0 for an accepted path, 1..8 for the original ordered path errors,
/// or u32::MAX for invalid ABI arguments. Does not allocate or retain pointers.
/// # Safety
/// Nonempty input addresses `length` readable UTF-16 units for this call.
#[no_mangle]
pub unsafe extern "C" fn lom_archive_path_v1(path: *const u16, length: i32) -> u32 {
    if length < 0
        || length as usize > isize::MAX as usize / 2
        || (length > 0 && (path.is_null() || !path.is_aligned()))
    {
        return u32::MAX;
    }
    let units = if length == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(path, length as usize) }
    };
    validate(units)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_error_order_and_accepted_unicode() {
        for (s, want) in [
            ("", 1),
            ("a\0\\", 1),
            ("/a\\b", 2),
            ("C:/a", 3),
            ("a//b", 5),
            ("a/../b", 5),
            ("a/a:b", 6),
            ("a./x", 7),
            ("a/CoM1.lua", 8),
            ("目录/𝄞.json", 0),
            ("a/", 0),
            ("COM0", 0),
            ("COM¹", 0),
            ("a/cOn.txt", 8),
        ] {
            assert_eq!(
                validate(&s.encode_utf16().collect::<Vec<_>>()),
                want,
                "{s:?}"
            );
        }
    }
}
