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

    env.insert("CFLAGS".into(), format!("-I{}", include_dir.display()));
    env.insert("CPPFLAGS".into(), format!("-I{}", include_dir.display()));
    env.insert("LDFLAGS".into(), format!("-L{}", lib_dir.display()));

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

    env
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
