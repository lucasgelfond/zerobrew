use std::collections::HashMap;
use std::path::Path;

use zb_core::BuildPlan;

pub fn build_env(plan: &BuildPlan, prefix: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();

    let bin_dir = prefix.join("bin");
    let lib_dir = prefix.join("lib");
    let include_dir = prefix.join("include");
    let pkgconfig_dir = lib_dir.join("pkgconfig");

    let system_path = std::env::var("PATH").unwrap_or_default();
    env.insert(
        "PATH".into(),
        format!("{}:{system_path}", bin_dir.display()),
    );

    let system_pkg = std::env::var("PKG_CONFIG_PATH").unwrap_or_default();
    env.insert(
        "PKG_CONFIG_PATH".into(),
        format!("{}:{system_pkg}", pkgconfig_dir.display()),
    );

    let system_cflags = std::env::var("CFLAGS").unwrap_or_default();
    let system_cppflags = std::env::var("CPPFLAGS").unwrap_or_default();
    let system_ldflags = std::env::var("LDFLAGS").unwrap_or_default();

    env.insert(
        "CFLAGS".into(),
        format!("-I{} {system_cflags}", include_dir.display())
            .trim()
            .to_string(),
    );
    env.insert(
        "CPPFLAGS".into(),
        format!("-I{} {system_cppflags}", include_dir.display())
            .trim()
            .to_string(),
    );
    env.insert(
        "LDFLAGS".into(),
        format!("-L{} {system_ldflags}", lib_dir.display())
            .trim()
            .to_string(),
    );

    env.insert("HOMEBREW_PREFIX".into(), prefix.display().to_string());
    env.insert(
        "HOMEBREW_CELLAR".into(),
        prefix.join("Cellar").display().to_string(),
    );

    env.insert("ZEROBREW_PREFIX".into(), prefix.display().to_string());
    env.insert(
        "ZEROBREW_CELLAR".into(),
        prefix.join("Cellar").display().to_string(),
    );
    env.insert("ZEROBREW_FORMULA_NAME".into(), plan.formula_name.clone());
    env.insert("ZEROBREW_FORMULA_VERSION".into(), plan.version.clone());

    env.insert("MAKEFLAGS".into(), format!("-j{}", num_cpus()));

    #[cfg(target_os = "macos")]
    if !env.contains_key("MACOSX_DEPLOYMENT_TARGET") {
        let target = std::env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| {
            zb_core::macos_major_version()
                .map(|v| format!("{v}.0"))
                .unwrap_or_else(|| "15.0".to_string())
        });
        env.insert("MACOSX_DEPLOYMENT_TARGET".into(), target);
    }

    env
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zb_core::{BuildPlan, BuildSystem};

    fn test_plan() -> BuildPlan {
        BuildPlan {
            formula_name: "test".to_string(),
            version: "1.0.0".to_string(),
            source_url: "https://example.com/test.tar.gz".to_string(),
            source_checksum: None,
            ruby_source_path: None,
            build_dependencies: Vec::new(),
            runtime_dependencies: Vec::new(),
            detected_system: BuildSystem::Autoconf,
            prefix: PathBuf::from("/opt/zerobrew/prefix"),
            cellar_path: PathBuf::from("/opt/zerobrew/cellar/test/1.0.0"),
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn build_env_includes_macosx_deployment_target() {
        let plan = test_plan();
        let env = build_env(&plan, &PathBuf::from("/opt/zerobrew/prefix"));
        assert!(env.contains_key("MACOSX_DEPLOYMENT_TARGET"));
        let target = &env["MACOSX_DEPLOYMENT_TARGET"];
        assert!(
            target.contains('.'),
            "expected version format like '15.0', got '{target}'"
        );
    }

    #[test]
    fn build_env_includes_standard_vars() {
        let plan = test_plan();
        let env = build_env(&plan, &PathBuf::from("/opt/zerobrew/prefix"));
        assert!(env.contains_key("ZEROBREW_PREFIX"));
        assert!(env.contains_key("ZEROBREW_FORMULA_NAME"));
        assert!(env.contains_key("MAKEFLAGS"));
    }
}
