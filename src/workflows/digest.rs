// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowercase hexadecimal SHA-256 digests shared by workflow, agent and file identities.

use sha2::{Digest as _, Sha256};

/// Lowercase hexadecimal text of digest bytes.
pub fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Lowercase hexadecimal SHA-256 of `bytes`, without a prefix.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes))
}

/// Whether `value` is a lowercase hexadecimal SHA-256 digest, with or without `sha256:`.
pub fn is_sha256_digest(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_lowercase_hex_and_validate_with_or_without_prefix() {
        let digest = sha256_hex(b"abc");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_sha256_digest(&digest));
        assert!(is_sha256_digest(&format!("sha256:{digest}")));
        assert!(!is_sha256_digest(&digest.to_uppercase()));
        assert!(!is_sha256_digest(&digest[1..]));
        assert_eq!(hex_digest([0x0a, 0xff]), "0aff");
    }
}
