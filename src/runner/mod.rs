//! Repeat-runner: execute the test command N times, capture output, parse, return TestRuns.
//!
//! Cross-platform subprocess handling. Best-effort tree-kill via tokio's
//! `kill_on_drop(true)` + explicit `kill().await`. On Windows, child processes
//! that spawn their own workers may leak — this is documented as a known
//! limitation; users on Windows should prefer single-process test commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::domain::{Framework, TestRun};
use crate::error::{FlaketideError, Result};
use crate::parser::{detect::detect, parser_for};

pub mod capture;
pub mod process;

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub runs: u32,
    pub parallel: u8,
    pub isolated: bool,
    pub timeout: Option<Duration>,
    pub tee: bool,
    pub framework: Option<Framework>,
    pub command: Vec<String>,
    pub working_dir: PathBuf,
}

pub struct RepeatRunner {
    options: RunOptions,
}

#[derive(Debug)]
pub struct RunnerOutcome {
    pub runs: Vec<TestRun>,
}

impl RepeatRunner {
    pub fn new(options: RunOptions) -> Self { Self { options } }

    pub async fn execute(&self) -> Result<RunnerOutcome> {
        let parallel = self.options.parallel.max(1) as usize;
        let sem = Arc::new(Semaphore::new(parallel));
        let mut handles = Vec::with_capacity(self.options.runs as usize);
        let total = self.options.runs;

        for run_id in 0..self.options.runs {
            let permit = sem.clone().acquire_owned().await.unwrap();
            let opts = self.options.clone();
            let h = tokio::spawn(async move {
                let _p = permit;
                run_once(run_id, total, &opts).await
            });
            handles.push(h);
        }

        let mut runs = Vec::with_capacity(self.options.runs as usize);
        for h in handles {
            match h.await {
                Ok(Ok(run)) => runs.push(run),
                Ok(Err(e)) => return Err(e),
                Err(join_err) => return Err(FlaketideError::Runner(format!("task: {join_err}"))),
            }
        }
        Ok(RunnerOutcome { runs })
    }
}

async fn run_once(run_id: u32, total: u32, options: &RunOptions) -> Result<TestRun> {
    let started_at = Utc::now();
    // Log only the program name — argv may contain secrets (e.g.
    // `cargo test -- --env API_KEY=...`). The full command is recorded
    // in the SQLite history, but not in tracing output.
    let program_name = options.command.first().map(|s| s.as_str()).unwrap_or("?");
    tracing::info!(
        run = run_id + 1, of = total, program = program_name,
        "starting run"
    );

    let (program, args) = options.command.split_first()
        .ok_or_else(|| FlaketideError::Runner("empty command".into()))?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.current_dir(&options.working_dir);
    cmd.env("FLAKETIDE_RUN_ID", run_id.to_string());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    cmd.kill_on_drop(true);

    // Use tempfile for an unpredictable, O_EXCL-created directory; the
    // previous `std::env::temp_dir().join(format!("flaketide-{pid}-{id}"))`
    // path was predictable and TOCTOU-attackable for `--isolated` runs.
    let temp_guard = if options.isolated {
        let td = tempfile::Builder::new()
            .prefix("flaketide-")
            .tempdir()
            .map_err(|e| FlaketideError::Runner(format!("tempdir: {e}")))?;
        cmd.env("TMPDIR", td.path());
        cmd.env("TEMP", td.path());
        cmd.env("TMP", td.path());
        Some(td)
    } else { None };

    let mut child = cmd.spawn().map_err(|e| FlaketideError::Runner(format!("spawn: {e}")))?;

    let stdout = child.stdout.take().ok_or_else(|| FlaketideError::Runner("no stdout".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| FlaketideError::Runner("no stderr".into()))?;

    let tee = options.tee;
    let stdout_task = tokio::spawn(capture::collect_stream(stdout, tee, false));
    let stderr_task = tokio::spawn(capture::collect_stream(stderr, tee, true));

    // Default safety net: even when the user hasn't set an explicit timeout,
    // a 30-minute cap prevents a hanging test command from blocking all runs
    // indefinitely (which would burn unlimited CI minutes). Set the config's
    // `timeout = "0s"` to opt out and accept unbounded waits.
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
    let effective_timeout = match options.timeout {
        Some(t) if t.as_secs() == 0 => None,
        Some(t) => Some(t),
        None => Some(DEFAULT_TIMEOUT),
    };
    let exit_status = match effective_timeout {
        Some(t) => match tokio::time::timeout(t, child.wait()).await {
            Ok(r) => r.map_err(|e| FlaketideError::Runner(format!("wait: {e}")))?,
            Err(_) => {
                let _ = child.kill().await;
                return Err(FlaketideError::Timeout(t));
            }
        },
        None => child.wait().await.map_err(|e| FlaketideError::Runner(format!("wait: {e}")))?,
    };

    let stdout_bytes = stdout_task.await
        .map_err(|e| FlaketideError::Runner(format!("stdout task: {e}")))??;
    let stderr_bytes = stderr_task.await
        .map_err(|e| FlaketideError::Runner(format!("stderr task: {e}")))??;
    let finished_at = Utc::now();

    // Drop the TempDir guard explicitly so cleanup runs in the same scope.
    drop(temp_guard);

    let framework = options.framework
        .or_else(|| detect(&stdout_bytes))
        .or_else(|| detect(&stderr_bytes))
        .ok_or(FlaketideError::FrameworkUndetected)?;

    let parser = parser_for(framework);
    let primary = if !stdout_bytes.is_empty() { &stdout_bytes } else { &stderr_bytes };
    let results = parser.parse(primary)?;

    Ok(TestRun {
        run_id: 0,
        started_at,
        finished_at,
        command: options.command.join(" "),
        exit_code: exit_status.code().unwrap_or(-1),
        framework,
        git_sha: read_git_sha(&options.working_dir),
        results,
    })
}

fn read_git_sha(working_dir: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(working_dir)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[allow(dead_code)]
fn _placeholder(_: HashMap<String, String>) {}
