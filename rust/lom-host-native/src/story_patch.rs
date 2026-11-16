//! Anchored edits over the untouched UTF-16 source. No game or ownership state.
use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
    slice,
};

#[repr(C)]
pub struct Edit {
    pub find: *const u16,
    pub find_len: i32,
    pub occurrence: i32,
    pub replacement: *const u16,
    pub replacement_len: i32,
    pub validation: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Span {
    pub start: i32,
    pub end: i32,
    pub edit: i32,
}

fn valid_text(pointer: *const u16, length: i32) -> bool {
    length >= 0
        && length as usize <= isize::MAX as usize / 2
        && (length == 0 || (!pointer.is_null() && pointer.is_aligned()))
}
unsafe fn text<'a>(pointer: *const u16, length: i32) -> &'a [u16] {
    if length == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(pointer, length as usize) }
    }
}

/// Returns 1 success, 2 invalid edit shape, 3 invalid anchor/occurrence range,
/// 4 missing nth anchor, 5 overlapping ranges, 6 output too large, 0 bad ABI.
/// Individual validation codes from C# are consumed in original edit order;
/// missing anchors precede later edit errors, and overlap is checked last.
///
/// # Safety
/// Pointers must address pinned, initialized, nonoverlapping buffers for their
/// declared counts. Edit strings remain alive through plan and write. The plan
/// has room for edit_count spans. No pointer is retained between calls.
#[no_mangle]
pub unsafe extern "C" fn lom_story_patch_plan_v1(
    source: *const u16,
    source_len: i32,
    edits: *const Edit,
    edit_count: i32,
    plan: *mut Span,
    output_len: *mut i64,
) -> u32 {
    if !valid_text(source, source_len)
        || !(0..=4096).contains(&edit_count)
        || (edit_count > 0
            && (edits.is_null() || !edits.is_aligned() || plan.is_null() || !plan.is_aligned()))
        || output_len.is_null()
        || !output_len.is_aligned()
    {
        return 0;
    }
    let source = unsafe { text(source, source_len) };
    let edits = if edit_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(edits, edit_count as usize) }
    };
    for e in edits {
        if !(0..=2).contains(&e.validation) {
            return 0;
        }
        if e.validation == 0
            && (!valid_text(e.find, e.find_len) || !valid_text(e.replacement, e.replacement_len))
        {
            return 0;
        }
    }
    let plan = if edit_count == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(plan, edit_count as usize) }
    };
    // Store only requested occurrence positions, never every match in a large
    // source. Repeated anchors share a scan, bounded by <=4096 requested edits.
    let mut groups: HashMap<&[u16], Vec<(i32, usize)>> = HashMap::new();
    for (i, e) in edits.iter().enumerate() {
        if e.validation == 0
            && e.find_len > 0
            && e.find_len <= 65536
            && e.replacement_len <= 262144
            && (1..=1_000_000).contains(&e.occurrence)
        {
            groups
                .entry(unsafe { text(e.find, e.find_len) })
                .or_default()
                .push((e.occurrence, i));
        }
    }
    let mut searched = HashSet::new();
    let mut positions = vec![None; edits.len()];
    let mut prefix = Vec::new();
    for (i, e) in edits.iter().enumerate() {
        if e.validation == 1 {
            return 2;
        }
        if e.validation == 2
            || e.find_len <= 0
            || e.find_len > 65536
            || e.replacement_len > 262144
            || !(1..=1_000_000).contains(&e.occurrence)
        {
            return 3;
        }
        let find = unsafe { text(e.find, e.find_len) };
        if searched.insert(find) {
            let queries = groups.get_mut(find).expect("validated edit group");
            queries.sort_unstable_by_key(|q| q.0);
            prefix.clear();
            prefix.resize(find.len(), 0usize);
            let mut matched = 0;
            for at in 1..find.len() {
                while matched > 0 && find[at] != find[matched] {
                    matched = prefix[matched - 1];
                }
                if find[at] == find[matched] {
                    matched += 1;
                }
                prefix[at] = matched;
            }
            matched = 0;
            let (mut occurrence, mut query) = (0, 0);
            for (at, &unit) in source.iter().enumerate() {
                while matched > 0 && unit != find[matched] {
                    matched = prefix[matched - 1];
                }
                if unit == find[matched] {
                    matched += 1;
                }
                if matched == find.len() {
                    occurrence += 1;
                    while query < queries.len() && queries[query].0 == occurrence {
                        positions[queries[query].1] = Some(at + 1 - find.len());
                        query += 1;
                    }
                    if query == queries.len() {
                        break;
                    }
                    matched = 0; // original IndexOf starts after the full match
                }
            }
        }
        let Some(start) = positions[i] else {
            return 4;
        };
        plan[i] = Span {
            start: start as i32,
            end: start as i32 + e.find_len,
            edit: i as i32,
        };
    }
    plan.sort_unstable_by_key(|span| span.start);
    let (mut end, mut length) = (0, source_len as i64);
    for span in plan.iter() {
        if span.start < end {
            return 5;
        }
        end = span.end;
        length += edits[span.edit as usize].replacement_len as i64 - (span.end - span.start) as i64;
    }
    if length > i32::MAX as i64 {
        return 6;
    }
    unsafe {
        output_len.write(length);
    }
    1
}

/// Copy a completed plan without repeating anchor scans. Returns 1 or 0 bad ABI.
/// # Safety
/// Same lifetime/ownership requirements as plan; output has output_len UTF-16
/// units and must not alias source or any edit string.
#[no_mangle]
pub unsafe extern "C" fn lom_story_patch_write_v1(
    source: *const u16,
    source_len: i32,
    edits: *const Edit,
    edit_count: i32,
    plan: *const Span,
    output: *mut u16,
    output_len: i32,
) -> u32 {
    if !valid_text(source, source_len)
        || !valid_text(output, output_len)
        || !(0..=4096).contains(&edit_count)
        || (edit_count > 0
            && (edits.is_null() || !edits.is_aligned() || plan.is_null() || !plan.is_aligned()))
        || edit_count as usize > isize::MAX as usize / size_of::<Edit>()
    {
        return 0;
    }
    let source = unsafe { text(source, source_len) };
    let edits = if edit_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(edits, edit_count as usize) }
    };
    let plan = if edit_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(plan, edit_count as usize) }
    };
    let (mut end, mut length) = (0, source_len as i64);
    for span in plan {
        if span.start < end
            || span.end < span.start
            || span.end > source_len
            || span.edit < 0
            || span.edit >= edit_count
        {
            return 0;
        }
        let e = &edits[span.edit as usize];
        if !valid_text(e.replacement, e.replacement_len) || e.replacement_len > 262144 {
            return 0;
        }
        length += e.replacement_len as i64 - (span.end - span.start) as i64;
        end = span.end;
    }
    if length != output_len as i64 {
        return 0;
    }
    let output = if output_len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(output, output_len as usize) }
    };
    let (mut from, mut to) = (0, 0);
    for span in plan {
        let gap = span.start as usize - from;
        output[to..to + gap].copy_from_slice(&source[from..span.start as usize]);
        to += gap;
        let e = &edits[span.edit as usize];
        let value = unsafe { text(e.replacement, e.replacement_len) };
        output[to..to + value.len()].copy_from_slice(value);
        to += value.len();
        from = span.end as usize;
    }
    output[to..].copy_from_slice(&source[from..]);
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_nonoverlapping_occurrences_and_error_precedence() {
        let source: Vec<u16> = "aaaaa".encode_utf16().collect();
        let find: Vec<u16> = "aa".encode_utf16().collect();
        let mut edits = [
            Edit {
                find: find.as_ptr(),
                find_len: 2,
                occurrence: 2,
                replacement: std::ptr::null(),
                replacement_len: 0,
                validation: 0,
            },
            Edit {
                find: std::ptr::null(),
                find_len: 0,
                occurrence: 0,
                replacement: std::ptr::null(),
                replacement_len: 0,
                validation: 1,
            },
        ];
        let mut plan = [Span::default(); 2];
        let mut length = 0;
        let call = |edits: &[Edit], plan: &mut [Span], length: &mut i64| unsafe {
            lom_story_patch_plan_v1(
                source.as_ptr(),
                source.len() as i32,
                edits.as_ptr(),
                edits.len() as i32,
                plan.as_mut_ptr(),
                length,
            )
        };
        assert_eq!(call(&edits[..1], &mut plan[..1], &mut length), 1);
        assert_eq!((plan[0].start, length), (2, 3));
        edits[0].occurrence = 3;
        assert_eq!(call(&edits, &mut plan, &mut length), 4);
        edits[0].occurrence = 2;
        assert_eq!(call(&edits, &mut plan, &mut length), 2);
        assert_eq!(
            size_of::<Edit>(),
            if cfg!(target_pointer_width = "64") {
                32
            } else {
                24
            }
        );
        assert_eq!(size_of::<Span>(), 12);
    }
}
