//! Streaming SHA-256. Context ownership stays with the synchronous C# wrapper.
use sha2::{Digest, Sha256};
use std::{ptr, slice};

#[no_mangle]
pub extern "C" fn lom_sha256_new_v1() -> *mut Sha256 {
    Box::into_raw(Box::new(Sha256::new()))
}

/// # Safety
/// `context` is a live, exclusively owned context from new_v1. `data` addresses
/// `length` readable bytes and is not retained; zero length permits null data.
#[no_mangle]
pub unsafe extern "C" fn lom_sha256_update_v1(
    context: *mut Sha256,
    data: *const u8,
    length: i32,
) -> i32 {
    if context.is_null() || !context.is_aligned() || length < 0 || (length > 0 && data.is_null()) {
        return -1;
    }
    if length > 0 {
        unsafe {
            (*context).update(slice::from_raw_parts(data, length as usize));
        }
    }
    0
}

/// Feed logical package and current entry digests from one read buffer.
/// Both contexts are checked before either changes. Aliases are rejected.
/// # Safety
/// Contexts must be distinct, live and exclusively owned. Data addresses length
/// readable bytes, disjoint from both contexts, and is not retained.
#[no_mangle]
pub unsafe extern "C" fn lom_sha256_update_pair_v1(
    first: *mut Sha256,
    second: *mut Sha256,
    data: *const u8,
    length: i32,
) -> i32 {
    if first.is_null()
        || second.is_null()
        || first == second
        || !first.is_aligned()
        || !second.is_aligned()
        || length < 0
        || (length > 0 && data.is_null())
    {
        return -1;
    }
    if length > 0 {
        let bytes = unsafe { slice::from_raw_parts(data, length as usize) };
        unsafe {
            (*first).update(bytes);
            (*second).update(bytes);
        }
    }
    0
}

/// # Safety
/// Context must be live and not concurrently accessed. Output addresses 32
/// writable bytes disjoint from the context. Does not release the context.
#[no_mangle]
pub unsafe extern "C" fn lom_sha256_finish_v1(
    context: *const Sha256,
    output: *mut u8,
    capacity: i32,
) -> i32 {
    if context.is_null() || !context.is_aligned() || output.is_null() || capacity < 32 {
        return -1;
    }
    let result = unsafe { (*context).clone() }.finalize();
    unsafe {
        ptr::copy_nonoverlapping(result.as_ptr(), output, 32);
    }
    32
}

/// # Safety
/// A nonnull context must come from new_v1 and be freed exactly once after its
/// last synchronous use. A null pointer is permitted.
#[no_mangle]
pub unsafe extern "C" fn lom_sha256_free_v1(context: *mut Sha256) {
    if !context.is_null() {
        unsafe {
            drop(Box::from_raw(context));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paired_updates_reject_invalid_arguments_before_either_changes() {
        let first = lom_sha256_new_v1();
        let second = lom_sha256_new_v1();
        let mut output = [0; 32];
        unsafe {
            assert_eq!(lom_sha256_update_v1(first, b"prefix".as_ptr(), 6), 0);
            assert_eq!(lom_sha256_update_pair_v1(first, second, ptr::null(), 0), 0);
            for (left, right, data, length) in [
                (first, first, b"x".as_ptr(), 1),
                (first, ptr::null_mut(), b"x".as_ptr(), 1),
                (ptr::null_mut(), second, b"x".as_ptr(), 1),
                (first.cast::<u8>().add(1).cast(), second, b"x".as_ptr(), 1),
                (first, second.cast::<u8>().add(1).cast(), b"x".as_ptr(), 1),
                (first, second, ptr::null(), 1),
                (first, second, b"x".as_ptr(), -1),
            ] {
                assert_eq!(lom_sha256_update_pair_v1(left, right, data, length), -1);
            }
            assert_eq!(lom_sha256_finish_v1(first, output.as_mut_ptr(), 32), 32);
            assert_eq!(&output[..], Sha256::digest(b"prefix").as_slice());
            assert_eq!(lom_sha256_finish_v1(second, output.as_mut_ptr(), 32), 32);
            assert_eq!(&output[..], Sha256::digest(b"").as_slice());
            assert_eq!(
                lom_sha256_update_pair_v1(first, second, b"entry".as_ptr(), 5),
                0
            );
            assert_eq!(lom_sha256_finish_v1(first, output.as_mut_ptr(), 32), 32);
            assert_eq!(&output[..], Sha256::digest(b"prefixentry").as_slice());
            assert_eq!(lom_sha256_finish_v1(second, output.as_mut_ptr(), 32), 32);
            assert_eq!(&output[..], Sha256::digest(b"entry").as_slice());
            lom_sha256_free_v1(first);
            lom_sha256_free_v1(second);
        }
    }
    #[test]
    fn streaming_and_invalid_arguments() {
        let context = lom_sha256_new_v1();
        let mut result = [0xAA; 33];
        unsafe {
            assert_eq!(lom_sha256_update_v1(context, ptr::null(), 0), 0);
            assert_eq!(lom_sha256_update_v1(context, ptr::null(), 1), -1);
            assert_eq!(lom_sha256_update_v1(context, b"abc".as_ptr(), -1), -1);
            assert_eq!(lom_sha256_update_v1(context, b"a".as_ptr(), 1), 0);
            assert_eq!(lom_sha256_update_v1(context, b"bc".as_ptr(), 2), 0);
            assert_eq!(lom_sha256_finish_v1(context, result.as_mut_ptr(), 31), -1);
            assert_eq!(result, [0xAA; 33]);
            assert_eq!(lom_sha256_finish_v1(context, result.as_mut_ptr(), 33), 32);
            assert_eq!(&result[..32], Sha256::digest(b"abc").as_slice());
            assert_eq!(result[32], 0xAA);
            lom_sha256_free_v1(context);
            assert_eq!(
                lom_sha256_finish_v1(ptr::null(), result.as_mut_ptr(), 32),
                -1
            );
            lom_sha256_free_v1(ptr::null_mut());
        }
    }
}
