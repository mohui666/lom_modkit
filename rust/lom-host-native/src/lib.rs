//! Pure batch-computation kernels used by the managed game Host.
//!
//! Game objects, file I/O and playback lifetime stay in C#. Batch entries borrow
//! pinned buffers only for synchronous calls; the Ogg decoder owns its input and
//! stream state behind an explicitly released handle. The integrity entry
//! does not cache decisions or change the original verification frequency.

#![deny(unsafe_op_in_unsafe_fn)]

use hmac::{digest::CtOutput, Hmac, Mac};
use sha2::{Digest, Sha256};
use std::{char::REPLACEMENT_CHARACTER, mem::size_of, slice};

pub mod archive_path;
pub mod audio_mix;
mod gif_decode;
#[allow(dead_code)]
#[path = "../../lom-core/src/gif_image.rs"]
mod gif_image;
pub mod json;
pub mod json_write;
pub mod ogg_decode;
pub mod pcm;
pub mod scenario_plan;
pub mod sha256;
pub mod source_compare;
pub mod state_codec;
pub mod story_patch;
pub mod text_scan;

type HmacSha256 = Hmac<Sha256>;
const PROTOCOL: &[u8] = b"lom-disclosure-integrity-v2\n";

/// Session identity is invalid; the fixed-stamp digest was not checked.
pub const SESSION_INVALID: u32 = 0;
/// Session identity is valid, but the fixed-stamp digest is invalid.
pub const STAMP_INVALID: u32 = 1;
/// Session identity and the fixed-stamp digest are both valid.
pub const INTEGRITY_VALID: u32 = 3;

/// Match Encoding.UTF8's replacement fallback without allocating a String.
fn update_dotnet_utf8(mac: &mut HmacSha256, units: &[u16]) {
    let mut bytes = [0u8; 256];
    let mut used = 0;
    for scalar in char::decode_utf16(units.iter().copied()) {
        let scalar = scalar.unwrap_or(REPLACEMENT_CHARACTER);
        let length = scalar.len_utf8();
        if bytes.len() - used < length {
            mac.update(&bytes[..used]);
            used = 0;
        }
        scalar.encode_utf8(&mut bytes[used..used + length]);
        used += length;
    }
    if used != 0 {
        mac.update(&bytes[..used]);
    }
}

/// Verify the session seal, then (only on success) the fixed-stamp SHA-256.
///
/// C ABI: `uint32_t lom_integrity_verify_v1(const uint16_t*, int32_t,
/// const uint16_t*, int32_t, const uint8_t*, int32_t, const uint8_t*,
/// const uint8_t*, const uint8_t*, const uint8_t*)` using the C calling convention.
/// The first three lengths count UTF-16 code units, UTF-16 code units, and bytes.
/// Fixed arrays have lengths **16, 32, 16, 16** respectively.
///
/// Empty mod IDs, non-64-character ASCII-hex fingerprints, non-32-byte seals,
/// negative lengths, null pointers, and misaligned UTF-16 pointers return 0.
/// Fingerprint case is immaterial. Embedded NULs and all nonempty UTF-16 mod IDs
/// are accepted; each unpaired surrogate is encoded as UTF-8 U+FFFD. Stamp bytes
/// are XORed with 0x5A, then bytes above 0x7F become `?`, matching .NET ASCII.
/// The payload is protocol + LF + mod UTF-8 + LF + uppercase fingerprint + LF +
/// decoded stamp. The protocol is unchanged; fixed arrays remain Host-owned.
///
/// Returns only 0, 1, or 3 (see the constants above). A rejection is a completed
/// verification, never an indication that native code is unavailable. Normal
/// verification uses bounded stack storage and no heap allocation or I/O.
///
/// # Safety
/// Pointers which pass the checks above must address initialized, readable
/// memory for their declared/fixed lengths in one allocation, without address
/// wraparound. These buffers must remain alive and must
/// not be mutated concurrently until this synchronous call returns. The caller
/// must supply all four fixed array lengths exactly; the ABI carries no length
/// fields for them. Invalid non-null addresses or undersized fixed arrays cannot
/// be validated by this pure computation entry point. No pointer is retained.
#[no_mangle]
pub unsafe extern "C" fn lom_integrity_verify_v1(
    mod_id: *const u16,
    mod_len: i32,
    fingerprint: *const u16,
    fp_len: i32,
    seal: *const u8,
    seal_len: i32,
    encoded_stamp_16: *const u8,
    expected_stamp_digest_32: *const u8,
    key_part_a_16: *const u8,
    key_part_b_16: *const u8,
) -> u32 {
    if mod_len <= 0
        || fp_len != 64
        || seal_len != 32
        || mod_id.is_null()
        || fingerprint.is_null()
        || seal.is_null()
        || encoded_stamp_16.is_null()
        || expected_stamp_digest_32.is_null()
        || key_part_a_16.is_null()
        || key_part_b_16.is_null()
        || !mod_id.is_aligned()
        || !fingerprint.is_aligned()
        || mod_len as usize > isize::MAX as usize / size_of::<u16>()
    {
        return SESSION_INVALID;
    }

    // All non-null buffers are caller-owned and pinned for this call.
    let mod_id = unsafe { slice::from_raw_parts(mod_id, mod_len as usize) };
    let fingerprint = unsafe { slice::from_raw_parts(fingerprint, 64) };
    let seal = unsafe { slice::from_raw_parts(seal, 32) };
    let mut uppercase = [0u8; 64];
    for (&unit, byte) in fingerprint.iter().zip(&mut uppercase) {
        *byte = match unit {
            0x0030..=0x0039 | 0x0041..=0x0046 => unit as u8,
            0x0061..=0x0066 => unit as u8 - 0x20,
            _ => return SESSION_INVALID,
        };
    }

    let encoded = unsafe { slice::from_raw_parts(encoded_stamp_16, 16) };
    let part_a = unsafe { slice::from_raw_parts(key_part_a_16, 16) };
    let part_b = unsafe { slice::from_raw_parts(key_part_b_16, 16) };
    let mut stamp = [0u8; 16];
    for (&encoded, decoded) in encoded.iter().zip(&mut stamp) {
        let byte = encoded ^ 0x5A;
        *decoded = if byte <= 0x7F { byte } else { b'?' };
    }
    let mut key = [0u8; 32];
    for (i, (&part, byte)) in part_a.iter().zip(&mut key[..16]).enumerate() {
        *byte = part ^ (0x37 + i as u8);
    }
    for (i, (&part, byte)) in part_b.iter().zip(&mut key[16..]).enumerate() {
        *byte = part ^ (0xA1 - i as u8);
    }
    let Ok(mut mac) = HmacSha256::new_from_slice(&key) else {
        return SESSION_INVALID;
    };
    mac.update(PROTOCOL);
    update_dotnet_utf8(&mut mac, mod_id);
    mac.update(b"\n");
    mac.update(&uppercase);
    mac.update(b"\n");
    mac.update(&stamp);
    if mac.verify_slice(seal).is_err() {
        return SESSION_INVALID;
    }

    // Preserve the original && short circuit: don't read/hash the expected
    // fixed payload until the identity seal has passed.
    let expected = unsafe { expected_stamp_digest_32.cast::<[u8; 32]>().read() };
    let digest = CtOutput::<Sha256>::new(Sha256::digest(stamp));
    if digest == CtOutput::<Sha256>::new(expected.into()) {
        INTEGRITY_VALID
    } else {
        STAMP_INVALID
    }
}

#[cfg(test)]
mod tests;
