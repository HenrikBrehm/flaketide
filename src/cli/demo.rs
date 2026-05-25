//! `flaketide demo` — self-contained walkthrough on a synthetic flaky suite.
//!
//! Spawns a tiny shell script that fails roughly 1/3 of the time, runs the
//! flaketide pipeline against it 30 times, prints the resulting verdicts.
//! Useful for: screenshots, conference demos, README GIFs, smoke-testing
//! a fresh install.

use std::path::Path;
use std::time::Duration;

use clap::Args;

use crate::domain::{Framework, Thresholds};
use crate::error::{FlaketideError, Result};
use crate::runner::{RepeatRunner, RunOptions};
use crate::stats;
use crate::store::Store;

#[derive(Debug, Args)]
pub struct DemoArgs {
    /// How many runs to perform.
    #[arg(short = 'n', long, default_value_t = 30)]
    pub runs: u32,
    /// Keep the demo SQLite database around for inspection.
    #[arg(long)]
    pub keep_db: bool,
}

pub async fn run(args: DemoArgs, _config: Option<&Path>) -> Result<i32> {
    let temp_dir = tempfile::Builder::new()
        .prefix("flaketide-demo-")
        .tempdir()
        .map_err(|e| FlaketideError::Runner(format!("tempdir: {e}")))?;
    let workdir = temp_dir.path();

    // Write a fixture flaky script in the temp working directory.
    let (script_path, command) = write_fixture_script(workdir)?;
    println!("flaketide demo — synthetic flaky test");
    println!("====================================");
    println!("Wrote a tiny flaky script to: {}", script_path.display());
    println!("It deterministically fails on every 3rd run.");
    println!();
    println!("Running the test command {} times…", args.runs);

    let db_path = workdir.join(".flaketide").join("history.db");
    std::fs::create_dir_all(db_path.parent().unwrap())?;
    let store = Store::open(&db_path).await?;

    let runner = RepeatRunner::new(RunOptions {
        runs: args.runs,
        parallel: 1,
        isolated: false,
        timeout: Some(Duration::from_secs(10)),
        tee: false,
        framework: Some(Framework::Cargo),
        command,
        working_dir: workdir.to_path_buf(),
    });
    let outcome = runner.execute().await?;
    println!("Done — captured {} runs.", outcome.runs.len());
    println!();

    for r in &outcome.runs {
        store.record_run(r).await?;
    }

    let thresholds = Thresholds::default();
    let verdicts = stats::summarize(&store, &thresholds).await?;
    store.upsert_flake_verdicts(&verdicts).await?;

    println!("Flake verdicts (sorted by severity):");
    crate::report::print_flake_table(&verdicts);

    println!();
    println!("Try next:");
    println!("  flaketide --config {} stats", workdir.join("flaketide.toml").display());
    println!("  flaketide --config {} tui      # interactive explorer", workdir.join("flaketide.toml").display());
    println!("  flaketide --config {} tag --days 1   # heuristic pattern detection", workdir.join("flaketide.toml").display());
    if args.keep_db {
        // Forget the TempDir guard so the cleanup is skipped — leaks the dir intentionally.
        let kept = temp_dir.keep();
        println!();
        println!("Demo data kept at: {}", kept.display());
    } else {
        println!();
        println!("(temp dir auto-cleaned; pass --keep-db to inspect)");
    }

    Ok(0)
}

#[cfg(unix)]
fn write_fixture_script(dir: &Path) -> Result<(std::path::PathBuf, Vec<String>)> {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("flaky.sh");
    let body = r#"#!/usr/bin/env bash
RUN_ID="${FLAKETIDE_RUN_ID:-0}"
echo "{ \"type\": \"suite\", \"event\": \"started\", \"test_count\": 1 }"
echo "{ \"type\": \"test\", \"event\": \"started\", \"name\": \"demo::synthetic_flake\" }"
if [ $((RUN_ID % 3)) -eq 0 ]; then
  echo "{ \"type\": \"test\", \"name\": \"demo::synthetic_flake\", \"event\": \"failed\", \"exec_time\": 0.01, \"stdout\": \"assertion failed: race condition reproduced\" }"
  echo "{ \"type\": \"suite\", \"event\": \"failed\", \"passed\": 0, \"failed\": 1 }"
  exit 101
else
  echo "{ \"type\": \"test\", \"name\": \"demo::synthetic_flake\", \"event\": \"ok\", \"exec_time\": 0.01 }"
  echo "{ \"type\": \"suite\", \"event\": \"ok\", \"passed\": 1, \"failed\": 0 }"
fi
"#;
    std::fs::write(&path, body)?;
    let mut perm = std::fs::metadata(&path)?.permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&path, perm)?;
    let cmd = vec!["bash".to_string(), path.display().to_string()];
    Ok((path, cmd))
}

#[cfg(windows)]
fn write_fixture_script(dir: &Path) -> Result<(std::path::PathBuf, Vec<String>)> {
    let path = dir.join("flaky.ps1");
    let body = r#"$runId = if ($env:FLAKETIDE_RUN_ID) { [int]$env:FLAKETIDE_RUN_ID } else { 0 }
Write-Output '{ "type": "suite", "event": "started", "test_count": 1 }'
Write-Output '{ "type": "test", "event": "started", "name": "demo::synthetic_flake" }'
if ($runId % 3 -eq 0) {
    Write-Output '{ "type": "test", "name": "demo::synthetic_flake", "event": "failed", "exec_time": 0.01, "stdout": "assertion failed: race condition reproduced" }'
    Write-Output '{ "type": "suite", "event": "failed", "passed": 0, "failed": 1 }'
    exit 101
} else {
    Write-Output '{ "type": "test", "name": "demo::synthetic_flake", "event": "ok", "exec_time": 0.01 }'
    Write-Output '{ "type": "suite", "event": "ok", "passed": 1, "failed": 0 }'
}
"#;
    std::fs::write(&path, body)?;
    let cmd = vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        path.display().to_string(),
    ];
    Ok((path, cmd))
}