//! SHA-256 content fingerprints.
//!
//! The optimistic-concurrency model of the patch pipeline compares the
//! fingerprint of a file *on disk* with the fingerprint that was recorded when
//! the diff was produced (`CR-SPEC-13 §4`). Both sides must agree on the
//! encoding, so every hash in the project is a lowercase hexadecimal SHA-256
//! digest of the UTF-8 bytes of the file content.

use sha2::{Digest, Sha256};

/// Compute the lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Compute the lowercase hexadecimal SHA-256 digest of the UTF-8 bytes of
/// `content`.
pub fn sha256_hex(content: &str) -> String {
    sha256_hex_bytes(content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{sha256_hex, sha256_hex_bytes};

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn empty_string_matches_the_known_vector() {
        assert_eq!(sha256_hex(""), EMPTY);
        assert_eq!(sha256_hex_bytes(b""), EMPTY);
    }

    #[test]
    fn abc_matches_the_known_vector() {
        assert_eq!(sha256_hex("abc"), ABC);
        assert_eq!(sha256_hex_bytes(b"abc"), ABC);
    }

    #[test]
    fn digest_is_lowercase_hex_of_the_expected_length() {
        let digest = sha256_hex("hello world");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!digest.chars().any(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn digest_is_stable_and_content_dependent() {
        assert_eq!(sha256_hex("same"), sha256_hex("same"));
        assert_ne!(sha256_hex("same"), sha256_hex("other"));
    }
}
