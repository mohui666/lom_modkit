use super::*;
use std::{alloc::System, cell::Cell, ptr};

// Expected values were produced by the original C# DisclosureIntegrity.cs,
// with the original fingerprint predicate, using .NET SHA256/HMACSHA256.
// Generator and complete UTF-16 inputs: out/host-native-check/managed-vectors.json
// (2026-11-15). These are reference outputs, not Rust-generated expected values.
const MOD: &[u16] = &[0x64, 0x65, 0x6d, 0x6f, 0x2e, 0x6d, 0x6f, 0x64];
const BASE_SEAL: &str = "86ec770d243b258a2858b38bbde15ca9bc5c53d69dd52d803ff34ffd4ac22c2d";

#[derive(Clone, Copy)]
struct Fixed {
    stamp: [u8; 16],
    digest: [u8; 32],
    a: [u8; 16],
    b: [u8; 16],
}

const FIXED: Fixed = Fixed {
    stamp: [
        23, 21, 30, 122, 117, 122, 15, 20, 21, 28, 28, 19, 25, 19, 27, 22,
    ],
    digest: [
        0xF7, 0x20, 0x81, 0xB2, 0x9C, 0xFB, 0x0A, 0x8C, 0x53, 0x80, 0x36, 0xE9, 0x8B, 0x7D, 0xE1,
        0x1E, 0x68, 0x5A, 0xEA, 0x05, 0xEC, 0x9A, 0x4F, 0xE2, 0xBF, 0xA2, 0x00, 0x60, 0x11, 0x0C,
        0xA1, 0xFF,
    ],
    a: [
        0x93, 0x21, 0xD7, 0x4B, 0x55, 0xA8, 0x0C, 0xFE, 0x11, 0x69, 0xB2, 0x38, 0xC4, 0x7D, 0x02,
        0xEA,
    ],
    b: [
        0x2D, 0xF0, 0x41, 0x98, 0xC3, 0x16, 0x77, 0x5A, 0xE1, 0x0B, 0x64, 0xAF, 0x39, 0xD2, 0x8C,
        0x05,
    ],
};

fn hex(text: &str) -> [u8; 32] {
    assert_eq!(text.len(), 64);
    let mut output = [0; 32];
    for (i, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
    }
    output
}

fn fingerprint() -> [u16; 64] {
    let mut output = [0; 64];
    for (unit, byte) in output
        .iter_mut()
        .zip(b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    {
        *unit = *byte as u16;
    }
    output
}

fn verify(mod_id: &[u16], fp: &[u16], seal: &[u8], fixed: &Fixed) -> u32 {
    unsafe {
        lom_integrity_verify_v1(
            mod_id.as_ptr(),
            mod_id.len() as i32,
            fp.as_ptr(),
            fp.len() as i32,
            seal.as_ptr(),
            seal.len() as i32,
            fixed.stamp.as_ptr(),
            fixed.digest.as_ptr(),
            fixed.a.as_ptr(),
            fixed.b.as_ptr(),
        )
    }
}

#[test]
fn managed_reference_vectors_preserve_utf16_and_fingerprint_case() {
    let fp = fingerprint();
    let cases: &[(&[u16], &str)] = &[
        (MOD, BASE_SEAL),
        (
            &[0x20, 0xa, 0, 0x69, 0x64, 0xd, 0xa],
            "a1b3a9a92742c5ea9945d1a9daf867efe52c881921fa627651bc92ee10208565",
        ),
        (
            &[
                0x5510, 0x5e03, 0x8863, 0x2e, 0x6728, 0x6954, 0x5b50, 0x7c, 0x65, 0x301,
            ],
            "e2a8aa5ced442481c178a36858072098b89bfcd34e5d65f22368615d20562f7f",
        ),
        (
            &[0x41, 0xd83d, 0xde03, 0x5a],
            "5c2c3160052882897a1971e8cb51012931d6f52957da707c24d79fd6ef3af3eb",
        ),
        (
            &[0x78, 0xd800, 0x79, 0xdc00, 0x7a, 0xd800, 0xd800],
            "2b588920440e4a0f1d754a7387b52cf22736e4fd32e85b1574b9ac08464431ee",
        ),
        (
            &[0xdc00, 0xd800, 0xdc00, 0xd800],
            "f50a158cc2d12d3633e92a4c4f8d11c56d0b5b2a2b4552a8b4a7fad8499262dc",
        ),
    ];
    for &(units, expected) in cases {
        assert_eq!(verify(units, &fp, &hex(expected), &FIXED), INTEGRITY_VALID);
    }
    let uppercase = fp.map(|unit| (unit as u8).to_ascii_uppercase() as u16);
    assert_eq!(
        verify(MOD, &uppercase, &hex(BASE_SEAL), &FIXED),
        INTEGRITY_VALID
    );
}

#[test]
fn utf8_stack_chunks_preserve_a_surrogate_pair_and_replacement_at_boundaries() {
    let mut units = vec![0x61; 255];
    units.extend_from_slice(&[0xd83d, 0xde03]);
    units.extend_from_slice(&[0x6f22; 100]);
    units.push(0xd800);
    let seal = hex("ab47e6e4a3c6d671d44d4cd664c966289fcebcd8f1e0e67ecc0c2634a73dba0d");
    assert_eq!(
        verify(&units, &fingerprint(), &seal, &FIXED),
        INTEGRITY_VALID
    );
}

#[test]
fn identity_seal_and_each_fixed_field_remain_bound() {
    let fp = fingerprint();
    let seal = hex(BASE_SEAL);
    let mut other_mod = MOD.to_vec();
    other_mod.push(0);
    assert_eq!(verify(&other_mod, &fp, &seal, &FIXED), SESSION_INVALID);
    let mut other_fp = fp;
    other_fp[0] = b'1' as u16;
    assert_eq!(verify(MOD, &other_fp, &seal, &FIXED), SESSION_INVALID);
    for i in 0..32 {
        let mut changed = seal;
        changed[i] ^= 1;
        assert_eq!(verify(MOD, &fp, &changed, &FIXED), SESSION_INVALID);
        let mut fixed = FIXED;
        fixed.digest[i] ^= 1;
        assert_eq!(verify(MOD, &fp, &seal, &fixed), STAMP_INVALID);
        // Bad identity must still return 0 when the stamp digest is bad too.
        assert_eq!(verify(MOD, &fp, &changed, &fixed), SESSION_INVALID);
    }
    for i in 0..16 {
        for field in 0..3 {
            let mut fixed = FIXED;
            match field {
                0 => fixed.stamp[i] ^= 1,
                1 => fixed.a[i] ^= 1,
                _ => fixed.b[i] ^= 1,
            }
            assert_eq!(verify(MOD, &fp, &seal, &fixed), SESSION_INVALID);
        }
    }
}

#[test]
fn custom_key_parts_and_ascii_fallback_match_managed_reference() {
    let fp = fingerprint();
    let mut fixed = FIXED;
    fixed.a[7] ^= 0x80;
    let seal = hex("1cd1f4c21e9b31ee3423b14fe443b342ea0f335879acbab0b434584b8ae82f7f");
    assert_eq!(verify(MOD, &fp, &seal, &fixed), INTEGRITY_VALID);
    fixed = FIXED;
    fixed.b[15] ^= 0x55;
    let seal = hex("791d85fb01b3d305b1d95b008e27625d8a2cfd457e96036337de17dbfcd67d50");
    assert_eq!(verify(MOD, &fp, &seal, &fixed), INTEGRITY_VALID);
    fixed = FIXED;
    fixed.stamp[0] = 0xda; // XOR 0x5A gives 0x80, which .NET ASCII maps to '?'.
    let seal = hex("1e529629f6abc41b9762765c6b6690ecb42cdd70ac38a07cf6952d88ec88c1fa");
    assert_eq!(verify(MOD, &fp, &seal, &fixed), STAMP_INVALID);
    assert_eq!(verify(MOD, &fp, &hex(BASE_SEAL), &fixed), SESSION_INVALID);
    fixed.digest = hex("ff64da5c3375fe657c7a11ae6f1704e5f22497000e3ff0bb4597886065c71cc2");
    assert_eq!(verify(MOD, &fp, &seal, &fixed), INTEGRITY_VALID);
}

#[test]
fn rejects_wrong_lengths_ascii_nonhex_and_null_or_misaligned_inputs() {
    let fp = fingerprint();
    let seal = hex(BASE_SEAL);
    let call = |m, ml, f, fl, s, sl| unsafe {
        lom_integrity_verify_v1(
            m,
            ml,
            f,
            fl,
            s,
            sl,
            FIXED.stamp.as_ptr(),
            FIXED.digest.as_ptr(),
            FIXED.a.as_ptr(),
            FIXED.b.as_ptr(),
        )
    };
    for length in [i32::MIN, -1, 0] {
        assert_eq!(
            call(MOD.as_ptr(), length, fp.as_ptr(), 64, seal.as_ptr(), 32),
            SESSION_INVALID
        );
    }
    for length in [-1, 0, 63, 65] {
        assert_eq!(
            call(MOD.as_ptr(), 8, fp.as_ptr(), length, seal.as_ptr(), 32),
            SESSION_INVALID
        );
    }
    for length in [-1, 0, 31, 33] {
        assert_eq!(
            call(MOD.as_ptr(), 8, fp.as_ptr(), 64, seal.as_ptr(), length),
            SESSION_INVALID
        );
    }
    assert_eq!(
        call(ptr::null(), 8, fp.as_ptr(), 64, seal.as_ptr(), 32),
        SESSION_INVALID
    );
    assert_eq!(
        call(MOD.as_ptr(), 8, ptr::null(), 64, seal.as_ptr(), 32),
        SESSION_INVALID
    );
    assert_eq!(
        call(MOD.as_ptr(), 8, fp.as_ptr(), 64, ptr::null(), 32),
        SESSION_INVALID
    );
    let misaligned = (MOD.as_ptr() as usize | 1) as *const u16;
    assert_eq!(
        call(misaligned, 8, fp.as_ptr(), 64, seal.as_ptr(), 32),
        SESSION_INVALID
    );
    let misaligned = (fp.as_ptr() as usize | 1) as *const u16;
    assert_eq!(
        call(MOD.as_ptr(), 8, misaligned, 64, seal.as_ptr(), 32),
        SESSION_INVALID
    );
    for invalid in [0, b'g' as u16, b'G' as u16, 0xe9, 0xff21, 0x0660, 0xd800] {
        let mut changed = fp;
        changed[63] = invalid;
        assert_eq!(verify(MOD, &changed, &seal, &FIXED), SESSION_INVALID);
    }
    for i in 0..4 {
        let mut pointers = [
            FIXED.stamp.as_ptr(),
            FIXED.digest.as_ptr(),
            FIXED.a.as_ptr(),
            FIXED.b.as_ptr(),
        ];
        pointers[i] = ptr::null();
        assert_eq!(
            unsafe {
                lom_integrity_verify_v1(
                    MOD.as_ptr(),
                    8,
                    fp.as_ptr(),
                    64,
                    seal.as_ptr(),
                    32,
                    pointers[0],
                    pointers[1],
                    pointers[2],
                    pointers[3],
                )
            },
            SESSION_INVALID
        );
    }
}

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        count_allocation();
        unsafe { std::alloc::GlobalAlloc::alloc(&System, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        count_allocation();
        unsafe { std::alloc::GlobalAlloc::alloc_zeroed(&System, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { std::alloc::GlobalAlloc::realloc(&System, p, layout, size) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::GlobalAlloc::dealloc(&System, p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn public_verification_does_not_allocate_on_the_calling_thread() {
    let fp = fingerprint();
    let seal = hex(BASE_SEAL);
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let result = verify(MOD, &fp, &seal, &FIXED);
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(result, INTEGRITY_VALID);
    assert_eq!(allocations, 0);
}
