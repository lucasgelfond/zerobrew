use std::path::PathBuf;
use std::process::{Command, Output};

struct TestEnv {
    root: tempfile::TempDir,
}

impl TestEnv {
    fn new() -> Self {
        Self {
            root: tempfile::TempDir::new().expect("failed to create temp dir"),
        }
    }

    fn zb(&self, args: &[&str]) -> Output {
        let zb = env!("CARGO_BIN_EXE_zb");
        Command::new(zb)
            .env("ZEROBREW_ROOT", self.root.path())
            .args(args)
            .output()
            .unwrap_or_else(|_| panic!("failed to execute {zb} command"))
    }

    fn bin_dir(&self) -> PathBuf {
        self.root.path().join("prefix").join("bin")
    }

    fn count_store_entries(&self) -> usize {
        assert!(self.root.path().join("store").is_dir());
        std::fs::read_dir(self.root.path().join("store"))
            .map(|r| r.count())
            .expect("failed to read store directory")
    }

    fn run_binary(&self, name: &str, args: &[&str]) -> Output {
        let bin_path = self.bin_dir().join(name);
        Command::new(&bin_path)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.bin_dir().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to execute {}: {e}", bin_path.display()))
    }
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{} failed:\nstdout: {}\nstderr: {}",
        context,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_stdout_contains(output: &Output, needle: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(needle),
        "expected stdout to contain {needle:?}, got: {stdout}"
    );
}

fn assert_no_installed_symlinks(dir: &std::path::Path) {
    if !dir.exists() {
        return;
    }
    let cellar = dir.join("Cellar");
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.expect("failed to read directory entry");
        if entry.path().starts_with(&cellar) {
            continue;
        }
        assert!(
            !entry.path_is_symlink(),
            "unexpected symlink: {}",
            entry.path().display()
        );
    }
}

#[test]
#[ignore = "integration test"]
#[cfg(target_os = "macos")] // GitHub Actions linux runner needs additional X11/XCB deps
fn test_ffmpeg_formula() {
    let t = TestEnv::new();

    assert_success(&t.zb(&["install", "ffmpeg"]), "zb install ffmpeg");

    // From the upstream formula test:
    // https://github.com/Homebrew/homebrew-core/blob/3076627c980d101ff02a720060c508433c44f293/Formula/f/ffmpeg.rb#L114
    let mp4out = t.root.path().join("video.mp4");
    assert_success(
        &t.run_binary(
            "ffmpeg",
            &[
                "-filter_complex",
                "testsrc=rate=1:duration=5",
                mp4out.to_str().unwrap(),
            ],
        ),
        "ffmpeg create test video",
    );
    assert!(mp4out.exists());
}

#[test]
#[ignore = "integration test"]
fn test_curl_simple() {
    let t = TestEnv::new();

    assert_success(&t.zb(&["install", "curl"]), "zb install curl");

    let output = t.run_binary("curl", &["https://www.githubstatus.com"]);
    assert_success(&output, "curl https://www.githubstatus.com");
    assert_stdout_contains(&output, "GitHub");
}

#[test]
#[ignore = "integration test"]
fn test_install_uninstall_and_reinstall() {
    let t = TestEnv::new();

    assert_success(&t.zb(&["install", "jq"]), "zb install jq");

    let test_json = t.root.path().join("test.json");
    std::fs::write(&test_json, r#"{"foo":1, "bar":2}"#).expect("failed to write test.json");

    let output = t.run_binary("jq", &[".bar", test_json.to_str().unwrap()]);
    assert_success(&output, "jq .bar test.json");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");

    assert_success(&t.zb(&["uninstall", "jq"]), "zb uninstall jq");
    assert_success(&t.zb(&["uninstall", "oniguruma"]), "zb uninstall oniguruma");
    assert!(!t.bin_dir().join("jq").exists());
    assert_no_installed_symlinks(&t.root.path().join("prefix"));

    assert_success(&t.zb(&["install", "jq"]), "zb install jq (reinstall)");
    assert_success(
        &t.run_binary("jq", &["--version"]),
        "jq --version after reinstall",
    );
}

#[test]
#[ignore = "integration test"]
fn test_list_installed_formulas() {
    let t = TestEnv::new();

    let output = t.zb(&["list"]);
    assert_success(&output, "zb list (empty)");
    assert_stdout_contains(&output, "No formulas installed");

    assert_success(&t.zb(&["install", "jq"]), "zb install jq");

    let output = t.zb(&["list"]);
    assert_success(&output, "zb list");
    assert_stdout_contains(&output, "jq");

    assert_success(&t.zb(&["uninstall", "jq"]), "zb uninstall jq");
    assert_success(&t.zb(&["uninstall", "oniguruma"]), "zb uninstall oniguruma");

    let output = t.zb(&["list"]);
    assert_success(&output, "zb list (empty)");
    assert_stdout_contains(&output, "No formulas installed");
}

#[test]
#[ignore = "integration test"]
fn test_info_finds_installed_formula() {
    let t = TestEnv::new();

    let output = t.zb(&["info", "jq"]);
    assert_success(&output, "zb info jq (not installed)");
    assert_stdout_contains(&output, "not installed");

    assert_success(&t.zb(&["install", "jq"]), "zb install jq");

    let output = t.zb(&["info", "jq"]);
    assert_success(&output, "zb info jq");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Name:") && !stdout.contains("not installed"),
        "stdout: {stdout}"
    );
}

#[test]
#[ignore = "integration test"]
fn test_gc_removes_unused_store_entries() {
    let t = TestEnv::new();

    assert_success(&t.zb(&["gc"]), "zb gc (empty)");
    assert_eq!(t.count_store_entries(), 0);

    assert_success(&t.zb(&["install", "jq"]), "zb install jq");
    let entries_before = t.count_store_entries();
    assert!(entries_before > 0);

    assert_success(&t.zb(&["uninstall", "jq"]), "zb uninstall jq");
    assert_success(&t.zb(&["uninstall", "oniguruma"]), "zb uninstall oniguruma");
    assert_eq!(t.count_store_entries(), entries_before);

    assert_success(&t.zb(&["gc"]), "zb gc");
    assert_eq!(t.count_store_entries(), 0);
}
