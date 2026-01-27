//! Tests based on historical Homebrew Brewfile/cask vulnerabilities.
//!
//! References:
//! - 2021 review-cask-pr incident: Spoofable git diff parser allowed auto-merge of malicious code

#[cfg(test)]
mod historical_vulnerabilities {
    use crate::parser::BrewfileParser;

    // ============================================================================
    // 2021 REVIEW-CASK-PR VULNERABILITY ADAPTED TO BREWFILES
    // ============================================================================
    // Original: Git diff parser could be spoofed to show benign changes while
    // actually introducing malicious code that bypassed automated review.
    //
    // Zerobrew adaptation: Ensure Brewfile parser cannot be confused by
    // clever formatting to hide malicious entries

    #[test]
    fn historical_hidden_entry_via_whitespace() {
        // ATTACK: Use whitespace/formatting to hide entries
        let brewfile = r#"
brew "jq"
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                brew "hidden-malware"
brew "wget"
"#;

        let parsed = BrewfileParser::parse(brewfile).unwrap();
        let entries = parsed.brew_entries();

        // Parser should find all entries regardless of whitespace
        assert_eq!(entries.len(), 3);
        
        // Entries should include the "hidden" one
        assert!(entries.iter().any(|e| e.name == "hidden-malware"));
        
        // This is CORRECT behavior - whitespace shouldn't hide entries
        // The defense is: all entries are visible in the plan
    }

    #[test]
    fn historical_entry_obfuscation_via_comments() {
        // ATTACK: Try to make malicious entry look like comment
        let brewfile = r#"
# Legitimate packages
brew "jq"
#brew "looks-like-comment-but-isnt"
brew "wget"
"#;

        let parsed = BrewfileParser::parse(brewfile).unwrap();
        let entries = parsed.brew_entries();

        // Second line IS a comment - should only find jq and wget
        assert_eq!(entries.len(), 2);
        assert!(!entries.iter().any(|e| e.name == "looks-like-comment-but-isnt"));
        
        // SECURE: Comments are properly stripped
    }

    #[test]
    fn historical_entry_confusion_via_similar_names() {
        // ATTACK: Use Unicode lookalikes or similar names to confuse user
        let brewfile = r#"
brew "jq"
brew "јq"
"#;  // Second is Cyrillic 'ј', not Latin 'j'

        let parsed = BrewfileParser::parse(brewfile).unwrap();
        let entries = parsed.brew_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "jq");
        assert_eq!(entries[1].name, "јq");  // Different character
        
        // These are treated as different packages
        // API won't find Cyrillic version
        // User could be tricked into thinking they're installing "jq"
        
        // Defense: Brewfile import shows ALL entries in plan before installing
        // User must review and confirm
    }

    // ============================================================================
    // HISTORICAL: String Injection in Sandbox Escape
    // ============================================================================
    // From 2023 Audit: "Sandbox escape via string injection"
    // Fixed in Homebrew

    #[test]
    fn historical_newline_injection_in_formula_name() {
        // ATTACK: Newline characters to break parsing or escape contexts
        let brewfile = "brew \"package\nmalicious\ncommand\"";

        let result = BrewfileParser::parse(brewfile);
        
        // Should either:
        // 1. Parse correctly with newlines in name (API won't find it)
        // 2. Reject as invalid
        
        if let Ok(parsed) = result {
            let entries = parsed.brew_entries();
            if let Some(entry) = entries.first() {
                println!("Parsed name with newlines: {:?}", entry.name);
                
                // If this gets to API request or path construction with newlines...
                // Could cause issues
            }
        }
        
        // TODO: Sanitize names to reject control characters including \n, \r, \0
    }

    #[test]
    fn historical_quote_escaping_bypass() {
        // ATTACK: Escaped quotes to break out of string context
        let brewfile = r#"brew "package\" malicious"; brew "other""#;

        let result = BrewfileParser::parse(brewfile);
        
        // Parser should handle escaped quotes correctly
        // Should NOT parse "malicious" as a separate entry
        
        if let Ok(parsed) = result {
            let entries = parsed.brew_entries();
            // Should find entries, but verify escaping didn't break parsing
            println!("Parsed with escaped quotes: {} entries", entries.len());
        }
    }

    // ============================================================================
    // 2021 review-cask-pr: Git Diff Parser Spoofing
    // ============================================================================
    // Not directly applicable to Zerobrew (no git diff parsing)
    // But documents the attack pattern

    #[test]
    fn historical_diff_parser_spoofing_analogy() {
        // HISTORICAL ATTACK: Showed benign diff, committed malicious code
        
        // In Zerobrew context, analogous attack would be:
        // Brewfile that looks benign when displayed but installs malware
        
        let brewfile = r#"
# Productivity tools
brew "jq"
brew "wget"
# Developer utilities  
brew "totally-legit-dev-tool"
"#;

        let parsed = BrewfileParser::parse(brewfile).unwrap();
        let entries = parsed.brew_entries();

        // All 3 entries visible in parsing
        assert_eq!(entries.len(), 3);
        
        // Defense in Zerobrew:
        // 1. Import dry-run shows ALL entries before installing
        // 2. Package names must exist in Homebrew API (can't be arbitrary)
        // 3. No hidden/obfuscated entries possible
        
        // Attack requires: Getting malicious formula into Homebrew API
        // Then distributing Brewfile with that formula
    }
}