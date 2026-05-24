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
use crate::parser::{detect, parser_for};

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
    tracing::info!(run = run_id + 1, of = total, command = ?options.command, "starting run");

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

    let temp_dir = if options.isolated {
        let dir = std::env::temp_dir().join(format!("flaketide-{}-{}", std::process::id(), run_id));
        std::fs::create_dir_all(&dir).ok();
        cmd.env("TMPDIR", &dir);
        cmd.env("TEMP", &dir);
        cmd.env("TMP", &dir);
        Some(dir)
    } else { None };

    let mut child = cmd.spawn().map_err(|e| FlaketideError::Runner(format!("spawn: {e}")))?;

    let stdout = child.stdout.take().ok_or_else(|| FlaketideError::Runner("no stdout".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| FlaketideError::Runner("no stderr".into()))?;

    let tee = options.tee;
    let stdout_task = tokio::spawn(capture::collect_stream(stdout, tee, false));
    let stderr_task = tokio::spawn(capture::collect_stream(stderr, tee, true));

    let exit_status = match options.timeout {
        Some(t) => {
            match tokio::time::timeout(t, child.wait()).await {
                Ok(r) => r.map_err(|e| FlaketideError::Runner(format!("wait: {e}")))?,
                Err(_) => {
                    let _ = child.kill().await;
                    return Err(FlaketideError::Timeout(t));
                }
            }
        }
        None => child.wait().await.map_err(|e| FlaketideError::Runner(format!("wait: {e}")))?,
    };

    let stdout_bytes = stdout_task.await
        .map_err(|e| FlaketideError::Runner(format!("stdout task: {e}")))??;
    let stderr_bytes = stderr_task.await
        .map_err(|e| FlaketideError::Runner(format!("stderr task: {e}")))??;
    let finished_at = Utc::now();

    if let Some(dir) = temp_dir {
        let _ = std::fs::remove_dir_all(dir);
    }

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
