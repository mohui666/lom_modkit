//! UTF-16 JSON string escaping used by ScenarioState's existing serializer.
//! Dictionary ordering, scalar formatting and object ownership stay managed.
use std::{ptr, slice};

fn quoted_length(text: &[u16]) -> Option<i32> {
    let mut length = 2usize;
    for &c in text {
        length = length.checked_add(if c < 32 {
            6
        } else if c == 34 || c == 92 {
            2
        } else {
            1
        })?;
    }
    if length > (i32::MAX as usize).min(isize::MAX as usize / 2) {
        None
    } else {
        Some(length as i32)
    }
}

unsafe fn input<'a>(text: *const u16, length: i32) -> Option<&'a [u16]> {
    if length < 0
        || length as usize > isize::MAX as usize / 2
        || length > 0 && (text.is_null() || !text.is_aligned())
    {
        return None;
    }
    Some(if length == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(text, length as usize) }
    })
}

/// Exact quoted output length, or -1 for invalid arguments/overflow.
/// # Safety
/// Nonempty text must address length readable UTF-16 units during the call.
#[no_mangle]
pub unsafe extern "C" fn lom_json_quote_length_v1(text: *const u16, length: i32) -> i32 {
    unsafe { input(text, length) }
        .and_then(quoted_length)
        .unwrap_or(-1)
}

/// Quote strings exactly like the historical Host: lower-case \u00xx for
/// control units, escape quote/backslash, keep all other UTF-16 units verbatim.
/// Returns the length, or -1 without writing on rejected lengths/capacity.
/// # Safety
/// Source/output address disjoint readable/writable spans of their declared
/// lengths and remain alive exclusively for this call. No pointer is retained.
#[no_mangle]
pub unsafe extern "C" fn lom_json_quote_write_v1(
    text: *const u16,
    length: i32,
    output: *mut u16,
    capacity: i32,
) -> i32 {
    let Some(source) = (unsafe { input(text, length) }) else {
        return -1;
    };
    let Some(needed) = quoted_length(source) else {
        return -1;
    };
    if output.is_null() || !output.is_aligned() || capacity < needed {
        return -1;
    }
    let mut at = 0usize;
    let mut put = |c| {
        unsafe {
            ptr::write(output.add(at), c);
        }
        at += 1;
    };
    put(34);
    const HEX: &[u8] = b"0123456789abcdef";
    for &c in source {
        if c < 32 {
            for c in [
                92,
                117,
                48,
                48,
                HEX[(c / 16) as usize] as u16,
                HEX[(c % 16) as usize] as u16,
            ] {
                put(c);
            }
        } else {
            if c == 34 || c == 92 {
                put(92);
            }
            put(c);
        }
    }
    put(34);
    needed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_utf16_and_lowercase_hex_with_atomic_rejection() {
        let text = [0u16, 31, 34, 92, 0xd800, 0xdc00, 65];
        let mut output = [0xAAAA; 32];
        assert_eq!(unsafe { lom_json_quote_length_v1(text.as_ptr(), 7) }, 21);
        assert_eq!(
            unsafe { lom_json_quote_write_v1(text.as_ptr(), 7, output.as_mut_ptr(), 20) },
            -1
        );
        assert_eq!(output, [0xAAAA; 32]);
        assert_eq!(
            unsafe { lom_json_quote_write_v1(text.as_ptr(), 7, output.as_mut_ptr(), 32) },
            21
        );
        assert_eq!(
            &output[..21],
            &[
                34, 92, 117, 48, 48, 48, 48, 92, 117, 48, 48, 49, 102, 92, 34, 92, 92, 0xd800,
                0xdc00, 65, 34
            ]
        );
    }
}
