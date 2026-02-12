use sha2::{Digest, Sha256};
use zb_core::Error;

/// Verify the SHA-256 checksum of a byte slice.
///
/// When `expected_sha256` is `None` the check is skipped (caller opted out).
/// When `Some`, the value must be a 64-character hex string; mismatches and
/// malformed expectations are returned as typed errors.
pub fn verify_sha256_bytes(bytes: &[u8], expected_sha256: Option<&str>) -> Result<(), Error> {
    let Some(expected_sha256) = expected_sha256 else {
        return Ok(());
    };

    let expected = normalize_sha256(expected_sha256)?;

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = format!("{:x}", hasher.finalize());

    if actual != expected {
        return Err(Error::ChecksumMismatch { expected, actual });
    }

    Ok(())
}

fn normalize_sha256(input: &str) -> Result<String, Error> {
    let normalized = input.trim().to_lowercase();

    if normalized.len() != 64 {
        return Err(Error::InvalidArgument {
            message: format!(
                "invalid sha256 checksum: expected 64 hex chars, got {}",
                normalized.len()
            ),
        });
    }

    if !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidArgument {
            message: "invalid sha256 checksum: must contain only hex characters".to_string(),
        });
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_verification_when_none() {
        assert!(verify_sha256_bytes(b"anything", None).is_ok());
    }

    #[test]
    fn accepts_valid_checksum() {
        // SHA-256 of b"hello"
        let expected = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_sha256_bytes(b"hello", Some(expected)).is_ok());
    }

    #[test]
    fn accepts_uppercase_and_whitespace() {
        let expected = " 2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824 ";
        assert!(verify_sha256_bytes(b"hello", Some(expected)).is_ok());
    }

    #[test]
    fn rejects_invalid_length() {
        let err = verify_sha256_bytes(b"hello", Some("abc")).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument { .. }));
    }

    #[test]
    fn rejects_non_hex() {
        let bad = format!("{}{}", "a".repeat(63), "z");
        let err = verify_sha256_bytes(b"hello", Some(&bad)).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument { .. }));
    }

    #[test]
    fn rejects_mismatch() {
        let err = verify_sha256_bytes(b"hello", Some(&"0".repeat(64))).unwrap_err();
        assert!(matches!(err, Error::ChecksumMismatch { .. }));
    }
}
