use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::process::Command;
use zb_core::{BuildPlan, Error};

use super::environment::build_env;
use super::source::download_and_extract_source;

const SHIM_RUBY: &str = include_str!("shim.rb");

pub struct BuildExecutor {
    prefix: PathBuf,
    work_root: PathBuf,
}

impl BuildExecutor {
    pub fn new(prefix: PathBuf) -> Self {
        let work_root = prefix.join("tmp").join("build");
        Self { prefix, work_root }
    }

    pub async fn execute(
        &self,
        plan: &BuildPlan,
        formula_rb_path: &Path,
        installed_deps: &HashMap<String, DepInfo>,
    ) -> Result<(), Error> {
        let work_dir = self.work_root.join(&plan.formula_name);
        self.prepare_work_dir(&work_dir).await?;

        let source_root = download_and_extract_source(
            &plan.source_url,
            plan.source_checksum.as_deref(),
            &work_dir,
        )
        .await?;

        let shim_path = work_dir.join("zerobrew_shim.rb");
        fs::write(&shim_path, SHIM_RUBY)
            .await
            .map_err(|e| Error::FileError {
                message: format!("failed to write ruby shim: {e}"),
            })?;

        fs::create_dir_all(&plan.cellar_path)
            .await
            .map_err(|e| Error::FileError {
                message: format!("failed to create cellar directory: {e}"),
            })?;

        let mut env = build_env(plan, &self.prefix);
        env.insert(
            "ZEROBREW_FORMULA_FILE".into(),
            formula_rb_path.display().to_string(),
        );

        let deps_json = serde_json::to_string(installed_deps).unwrap_or_else(|_| "{}".into());
        env.insert("ZEROBREW_INSTALLED_DEPS".into(), deps_json);

        let ruby = find_ruby().await?;
        run_build(&ruby, &shim_path, &source_root, &env).await?;

        self.cleanup_work_dir(&work_dir).await;
        Ok(())
    }

    async fn prepare_work_dir(&self, work_dir: &Path) -> Result<(), Error> {
        if work_dir.exists() {
            let _ = fs::remove_dir_all(work_dir).await;
        }
        fs::create_dir_all(work_dir)
            .await
            .map_err(|e| Error::FileError {
                message: format!("failed to create work directory: {e}"),
            })
    }

    async fn cleanup_work_dir(&self, work_dir: &Path) {
        let _ = fs::remove_dir_all(work_dir).await;
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DepInfo {
    pub cellar_path: String,
}

async fn find_ruby() -> Result<PathBuf, Error> {
    for candidate in ["ruby", "/usr/bin/ruby"] {
        let result = Command::new(candidate).arg("--version").output().await;

        if let Ok(output) = result
            && output.status.success()
        {
            return Ok(PathBuf::from(candidate));
        }
    }

    Err(Error::ExecutionError {
        message: "ruby not found — required for building from source".into(),
    })
}

async fn run_build(
    ruby: &Path,
    shim_path: &Path,
    source_root: &Path,
    env: &HashMap<String, String>,
) -> Result<(), Error> {
    let output = Command::new(ruby)
        .arg(shim_path)
        .current_dir(source_root)
        .envs(env)
        .output()
        .await
        .map_err(|e| Error::ExecutionError {
            message: format!("failed to execute ruby shim: {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        for line in stdout.lines() {
            eprintln!("  {line}");
        }
    }

    if !output.status.success() {
        let mut msg = format!(
            "source build failed (exit code: {:?})",
            output.status.code()
        );
        if !stderr.is_empty() {
            msg.push_str(&format!("\n{stderr}"));
        }
        return Err(Error::ExecutionError { message: msg });
    }

    Ok(())
}
