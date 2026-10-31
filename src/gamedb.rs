//! The game database's second layer: `gamedbRC.slt` is a large-block [container](crate::container) whose decrypted
//! blocks are XORed with a keystream; removing it gives an SQLite database image.
//!
//! The keystream runs in 4-byte groups from the start of the decrypted data. Group `q` (bytes `4q..4q+4`) is XORed
//! with the little-endian bytes of the IEEE CRC-32 of the four little-endian bytes of `q + C × (q + 1)` (wrapping
//! 32-bit arithmetic), each byte in `A..=Z` first folded to lower case. `C` is a constant you supply
//! ([`crate::keys::Keys::gamedb_scramble`]). The XOR is its own inverse.
//!
//! This is the layer as a community tool that decrypts the file describes it; it has not been checked against the
//! game's own code. A database that decodes to `SQLite format 3` and passes SQLite's integrity check is the check
//! `tests/real_data.rs` makes when you have an install and its keys.
use crate::container::Container;
use crate::keys::{TransportKeys, UsageKeys};
use primase_foundations::hash::crc32;

/// XORs `data`, taken from the start of the decrypted database, with the keystream of constant `scramble`. Applying
/// it to decrypted blocks gives the SQLite image; applying it to an image gives the blocks back.
pub fn descramble(data: &mut [u8], scramble: u32) {
    for (q, group) in (0u32..).zip(data.chunks_mut(4)) {
        let seed = q.wrapping_add(scramble.wrapping_mul(q.wrapping_add(1)));
        let folded = seed.to_le_bytes().map(|b| b.to_ascii_lowercase());
        for (byte, key) in group.iter_mut().zip(crc32(&folded).to_le_bytes()) {
            *byte ^= key;
        }
    }
}

/// The game database's SQLite image from its container: every block decrypted ([`Container::decrypt`], the padded
/// length; SQLite reads its own page count from the image's header), then descrambled.
pub fn decrypt(
    container: &Container<'_>,
    transport: &TransportKeys,
    keys: &UsageKeys,
    scramble: u32,
) -> Vec<u8> {
    let mut image = container.decrypt(transport, keys);
    descramble(&mut image, scramble);
    image
}

/// Whether `image` starts with SQLite's header string.
pub fn is_sqlite(image: &[u8]) -> bool {
    image.starts_with(b"SQLite format 3\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keystreams worked by hand with an independent CRC-32 (Python's `zlib.crc32`) from the description above; both
    /// constants put a byte in `A..=Z` into a seed, so the case folding is exercised.
    #[test]
    fn keystream_matches_hand_worked_values() {
        for (constant, expected) in [
            (
                0x41,
                [
                    0x7a, 0x4f, 0xde, 0xa2, 0xc9, 0xc6, 0xa8, 0xde, 0x28, 0xc1, 0xd7, 0x60,
                ],
            ),
            (
                0x1234_5678,
                [
                    0x32, 0xc1, 0x20, 0x97, 0xa4, 0x0f, 0xdb, 0x5e, 0xec, 0x17, 0xcc, 0x7c,
                ],
            ),
        ] {
            let mut zeros = [0u8; 12];
            descramble(&mut zeros, constant);
            assert_eq!(zeros, expected, "constant {constant:#x}");
        }
    }

    #[test]
    fn descramble_is_its_own_inverse_and_handles_a_short_tail() {
        let original: Vec<u8> = (0..=254).collect(); // 255 bytes: the last group is 3 long
        let mut data = original.clone();
        descramble(&mut data, 0xdead_beef);
        assert_ne!(data, original);
        descramble(&mut data, 0xdead_beef);
        assert_eq!(data, original);
    }
}
