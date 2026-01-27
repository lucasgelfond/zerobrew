# Historical Homebrew Vulnerabilities Analysis

> **Created**: January 2026
> **Purpose**: Document historical Homebrew security issues and verify Zerobrew doesn't repeat them

This document catalogs actual security vulnerabilities from Homebrew's history and shows how Zerobrew addresses or avoids each one.

## Research Sources

- **2023 Security Audit** (Trail of Bits, funded by Open Technology Fund)
  - URL: https://brew.sh/2024/07/30/homebrew-security-audit/
  - Comprehensive audit identifying 20+ security issues

- **2021 review-cask-pr Incident**
  - URL: https://brew.sh/2021/04/21/security-incident-disclosure/
  - Critical GitHub Actions vulnerability allowing auto-merge of malicious code

- **CVE-2024-3094** (XZ Utils Supply Chain Attack)
  - Homebrew proactively responded but was not directly compromised

## Vulnerabilities from 2023 Security Audit

### 1. Special Characters in Package Names/Versions

**Status in Homebrew**: Acknowledged but not fully resolved  
**Severity**: High

**Description**: Package names and versions can contain shell metacharacters and path separators, potentially enabling:
- Command injection if names used in shell contexts
- Path traversal if used in filesystem operations

**Zerobrew Status**: 
- ⚠️ **Same vulnerability exists** - no validation on formula names
- Tests added: `historical_special_chars_in_package_name`, `historical_special_chars_in_version`
- **Mitigation**: API serves as filter (won't find malicious names)
- **Recommendation**: Add explicit validation

**Test Results**: Tests pass (document the issue)

### 2. Path Traversal During File Caching

**Status in Homebrew**: Fixed  
**Severity**: High

**Description**: Downloaded files cached with attacker-controlled names could write outside cache directory.

**Zerobrew Status**: 
- ✅ **Not vulnerable** - cache uses SHA256 keys, not URL-derived names
- Path: `/opt/zerobrew/cache/blobs/{sha256}.tar.gz`
- No user/API control over cache filenames

**Test Added**: `historical_cache_path_traversal` (in zb_core)

### 3. Path Traversal During Bottling

**Status in Homebrew**: Fixed  
**Severity**: High

**Description**: Bottle creation could traverse paths outside intended directory.

**Zerobrew Status**: 
- ✅ **Not applicable** - Zerobrew doesn't create bottles (bottles-only consumer)
- Only downloads and extracts pre-built bottles

### 4. Sandbox Escape Via String Injection

**Status in Homebrew**: Fixed  
**Severity**: Critical

**Description**: String injection could escape sandbox restrictions.

**Zerobrew Status**: 
- ✅ **Different architecture** - no formula-level sandbox (bottles-only)
- No arbitrary code execution during installation
- Services run in launchd sandbox

**Test Added**: `historical_newline_injection_in_formula_name`

### 5. Formula Privilege Escalation Through Sudo

**Status in Homebrew**: Fixed  
**Severity**: Critical

**Description**: Formulas could request sudo during installation, potentially executing malicious code as root.

**Zerobrew Status**: 
- ✅ **Not vulnerable** - bottles-only architecture
- No source builds = no arbitrary code execution
- Sudo only used for directory creation (init/reset)
- Services with `require_root` not yet implemented

**Test Added**: `historical_formula_requests_sudo`

### 6. Bottles Beginning with "-" Breaking rm

**Status in Homebrew**: Fixed  
**Severity**: Medium

**Description**: Bottle filenames starting with `-` treated as options by rm: `rm -rf -filename` becomes `rm -rf` with `-filename` as option.

**Zerobrew Status**: 
- ✅ **Likely safe** - uses `std::fs::remove_file()` (Rust), not shell
- Rust file operations don't parse - as options

**Test Added**: `historical_dash_prefix_bottle_cleanup`

**Recommendation**: Audit all file deletion code to confirm no shell usage

### 7. Use of ldd on Untrusted Inputs

**Status in Homebrew**: Fixed  
**Severity**: High

**Description**: `ldd` executes code in binary's .init section, allowing RCE on untrusted binaries.

**Zerobrew Status**: 
- ✅ **Not vulnerable** - doesn't use ldd
- No dependency analysis currently

**Test Added**: `historical_ldd_on_untrusted_binary`

**Recommendation**: If adding dependency analysis, use `objdump -p` or `readelf -d`, never `ldd`

### 8. Use of Marshal (Ruby Deserialization)

**Status in Homebrew**: Fixed  
**Severity**: Critical

**Description**: Ruby's `Marshal.load()` on untrusted data allows arbitrary object deserialization → RCE.

**Zerobrew Status**: 
- ✅ **Not vulnerable** - uses JSON (serde), not Ruby Marshal
- Rust's type system prevents arbitrary deserialization
- All deserialization is type-safe

**Test Added**: `historical_marshal_deserialization`

### 9. Weak Cryptographic Digest in Formulary Namespaces

**Status in Homebrew**: Fixed  
**Severity**: Low

**Description**: Used weak hash for namespace generation.

**Zerobrew Status**: 
- ✅ **Better than Homebrew** - uses SHA256 for content-addressable store
- Store keys are bottle SHA256 hashes

**Test Added**: `historical_weak_hash_for_store_keys`

### 10. Sandbox Permissions for Important Directories

**Status in Homebrew**: Acknowledged  
**Severity**: Medium

**Description**: Sandbox doesn't restrict access to sensitive directories.

**Zerobrew Status**: 
- ⚠️ **No sandbox** - bottles are trusted (from Homebrew CDN)
- Extraction happens to controlled directory (`/opt/zerobrew/store/{sha}`)
- Relies on tar crate for path traversal prevention

**Recommendation**: Consider sandboxing extraction process

### 11. Extraction Not Sandboxed

**Status in Homebrew**: Acknowledged  
**Severity**: Medium

**Description**: Tarball extraction could overwrite system files if tarball malicious.

**Zerobrew Status**: 
- ⚠️ **Same limitation** - extraction not sandboxed
- Defense: Extraction to `/opt/zerobrew/store/{sha}/` (controlled location)
- Relies on `tar` crate to prevent `../../` paths in tarball

**Test Added**: `historical_uncontained_extraction`

**Recommendation**: Audit tar extraction code, consider explicit path validation

## Vulnerabilities from 2021 review-cask-pr Incident

### Git Diff Parser Spoofing

**Status in Homebrew**: Fixed (Actions disabled)  
**Severity**: Critical

**Description**: Automated review system could be tricked into approving malicious code through spoofed git diffs.

**Zerobrew Status**: 
- ✅ **Not applicable** - no automated merge system
- No git diff parsing

**Analogous Attack**: Brewfile that appears benign but installs malware

**Zerobrew Defenses:**
1. Import dry-run shows ALL entries before installing
2. Package names must exist in Homebrew API (can't be arbitrary)
3. User must review and confirm installations

**Test Added**: `historical_diff_parser_spoofing_analogy`, `historical_hidden_entry_via_whitespace`

## Test Coverage Summary

**14 new historical vulnerability tests:**

| Crate | Tests | Coverage |
|-------|-------|----------|
| zb_core | 8 | Special chars, path traversal, sudo, hashing, ldd, Marshal |
| zb_brewfile | 6 | Hidden entries, obfuscation, Unicode, diff spoofing |

**All 174 tests passing** (160 original + 14 historical)

## Architectural Advantages

Zerobrew's architecture inherently avoids several classes of Homebrew vulnerabilities:

1. **Bottles-Only** → No source builds → No arbitrary code execution
2. **Rust + serde** → No Marshal deserialization → No object injection
3. **JSON API** → No Ruby DSL execution → No code in package definitions
4. **SHA256 everywhere** → Strong cryptographic integrity
5. **Type-safe deserialization** → Prevents many injection attacks

## Remaining Concerns

**Should Fix:**
1. Formula name validation (special characters, path separators)
2. Version string validation
3. Dependency name validation
4. Audit tar extraction for path traversal prevention

**Future Considerations:**
5. Sandbox extraction process
6. Disk space limits
7. Rate limiting
8. Bottle signature verification (when Homebrew adds it)

## References

- Homebrew 2023 Security Audit: https://brew.sh/2024/07/30/homebrew-security-audit/
- 2021 Security Incident: https://brew.sh/2021/04/21/security-incident-disclosure/
- Trail of Bits Report: https://github.com/Homebrew/brew/blob/HEAD/docs/Homebrew-2023-Security-Audit.pdf