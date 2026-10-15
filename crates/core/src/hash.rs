// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Hashes. FNV-1a (64-bit) keys blocks and atoms for matching inside one run of
//! the engine; it is fast and fully specified, so results are the same on every
//! platform. SHA-256 names anything that is stored: change ids, blobs.

use sha2::{Digest, Sha256};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A streaming FNV-1a 64 hasher.
#[derive(Clone, Copy, Debug)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Fnv {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&mut self, data: &[u8]) -> &mut Self {
        for &byte in data {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
        self
    }

    /// Adds a string followed by a separator, so ("ab", "c") and ("a", "bc") differ.
    pub fn str(&mut self, text: &str) -> &mut Self {
        self.bytes(text.as_bytes()).bytes(&[0xFF])
    }

    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    #[must_use]
    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// FNV-1a 64 of a string.
#[must_use]
pub fn fnv(text: &str) -> u64 {
    Fnv::new().bytes(text.as_bytes()).finish()
}

/// SHA-256 as 64 lower-case hex characters.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0F)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_known_values() {
        // Reference values for FNV-1a 64.
        assert_eq!(fnv(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn separators_keep_fields_apart() {
        assert_ne!(
            Fnv::new().str("ab").str("c").finish(),
            Fnv::new().str("a").str("bc").finish()
        );
    }

    #[test]
    fn sha256_test_vectors() {
        // FIPS 180-2 examples.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
