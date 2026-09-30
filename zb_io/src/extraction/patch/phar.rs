//! Keep PHP archives valid after their contents are patched.
//!
//! A phar ends with a hash of everything before it, followed by a 4-byte
//! little-endian signature type and the magic `GBMB`. Homebrew bottles can
//! ship phars with `@@HOMEBREW_PREFIX@@` placeholders inside (composer does),
//! so the hash only matches once the placeholders are replaced with the exact
//! prefix the phar was built with. Any other prefix breaks the signature and
//! PHP refuses to load the archive.

use sha2::{Digest, Sha256, Sha512};

const MAGIC: &[u8] = b"GBMB";
const SHA256: u32 = 0x0003;
const SHA512: u32 = 0x0004;

/// If `data` is a phar signed with SHA-256 or SHA-512 whose hash no longer
/// matches its contents, return a copy with the hash recomputed. Returns
/// `None` for anything else, including phars that are already valid and
/// phars signed with algorithms we can't recompute (MD5, SHA-1, OpenSSL).
pub(crate) fn resign(data: &[u8]) -> Option<Vec<u8>> {
    let trailer = data.len().checked_sub(8)?;
    if &data[trailer + 4..] != MAGIC {
        return None;
    }
    let kind = u32::from_le_bytes(data[trailer..trailer + 4].try_into().ok()?);
    let hash_len = match kind {
        SHA256 => 32,
        SHA512 => 64,
        _ => return None,
    };
    let body_len = trailer.checked_sub(hash_len)?;
    let body = &data[..body_len];
    if !body.windows(18).any(|w| w == b"__HALT_COMPILER();") {
        return None;
    }

    let hash = match kind {
        SHA256 => Sha256::digest(body).to_vec(),
        _ => Sha512::digest(body).to_vec(),
    };
    if data[body_len..trailer] == hash[..] {
        return None;
    }

    let mut fixed = data.to_vec();
    fixed[body_len..trailer].copy_from_slice(&hash);
    Some(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phar(body: &[u8], kind: u32) -> Vec<u8> {
        let hash = match kind {
            SHA256 => Sha256::digest(body).to_vec(),
            _ => Sha512::digest(body).to_vec(),
        };
        let mut data = body.to_vec();
        data.extend_from_slice(&hash);
        data.extend_from_slice(&kind.to_le_bytes());
        data.extend_from_slice(MAGIC);
        data
    }

    const BODY: &[u8] = b"#!/usr/bin/env php\n<?php __HALT_COMPILER(); ?>\0\x01'@@HOMEBREW_PREFIX@@/etc/openssl@3/cert.pem'";

    fn patched(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let (needle, replacement) = (&b"@@HOMEBREW_PREFIX@@"[..], &b"/opt/zerobrew"[..]);
        let mut i = 0;
        while i < data.len() {
            if data[i..].starts_with(needle) {
                out.extend_from_slice(replacement);
                i += needle.len();
            } else {
                out.push(data[i]);
                i += 1;
            }
        }
        out
    }

    fn signature_is_valid(data: &[u8]) -> bool {
        let trailer = data.len() - 8;
        let kind = u32::from_le_bytes(data[trailer..trailer + 4].try_into().unwrap());
        let hash_len = if kind == SHA256 { 32 } else { 64 };
        let body = &data[..trailer - hash_len];
        let hash = if kind == SHA256 {
            Sha256::digest(body).to_vec()
        } else {
            Sha512::digest(body).to_vec()
        };
        data[trailer - hash_len..trailer] == hash[..]
    }

    #[test]
    fn recomputes_sha512_signature_after_patching() {
        let edited = patched(&phar(BODY, SHA512));
        assert!(!signature_is_valid(&edited));

        let fixed = resign(&edited).expect("patched phar should be re-signed");
        assert!(signature_is_valid(&fixed));
        assert_eq!(fixed.len(), edited.len());
    }

    #[test]
    fn recomputes_sha256_signature_after_patching() {
        let fixed = resign(&patched(&phar(BODY, SHA256))).unwrap();
        assert!(signature_is_valid(&fixed));
    }

    #[test]
    fn leaves_valid_phars_alone() {
        assert_eq!(resign(&phar(BODY, SHA512)), None);
    }

    #[test]
    fn leaves_unsupported_signature_types_alone() {
        let mut data = BODY.to_vec();
        data.extend_from_slice(&[0u8; 20]);
        data.extend_from_slice(&0x0002u32.to_le_bytes());
        data.extend_from_slice(MAGIC);
        assert_eq!(resign(&data), None);
    }

    #[test]
    fn ignores_files_that_are_not_phars() {
        assert_eq!(resign(b"#!/bin/sh\necho hello\n"), None);
        assert_eq!(resign(b"GBMB"), None);
        let mut not_phar = vec![b'x'; 100];
        not_phar.extend_from_slice(&SHA512.to_le_bytes());
        not_phar.extend_from_slice(MAGIC);
        assert_eq!(resign(&not_phar), None);
    }
}
