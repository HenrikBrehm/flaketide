//! `flaketide bisect` — find the commit that introduced a flake.
//!
//! Strategy: walk the commit range (default: HEAD ↤ HEAD~50) bisection-style.
//! For each candidate commit: `git stash → git checkout <sha> → run the test
//! N times → compute flake probability → restore`. Narrow the range to the
//! first commit where the posterior mean exceeds the user-defined threshold.
//!
//! The user supplies the test command (the same one they normally hand to
//! `flaketide run`) and the test-id to track.

use std::path::Path;
use std::process::Command as StdCommand;
use std::time::Duration;

use clap::Args;

use crate::cli::FrameworkArg;
use crate::config::load_config;
use crate::domain::{Framework, TestId};
use crate::error::{FlaketideError, Result};
use crate::runner::{RepeatRunner, RunOptions};
use crate::stats::flake_posterior;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct BisectArgs {
    /// Test id to track (suite::name).
    pub test_id: String,
    /// Inclusive oldest commit (default: HEAD~50).
    #[arg(long, default_value = "HEAD~50")]
    pub from: String,
    /// Inclusive newest commit (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    pub to: String,
    /// Runs per candidate commit. More = tighter posterior, more time.
    #[arg(short = 'n', long, default_value_t = 5)]
    pub runs: u32,
    /// Per-run timeout.
    #[arg(long, default_value = "5m", value_parser = crate::cli::parse_duration)]
    pub timeout: Duration,
    /// Framework override (default: from config / auto-detect).
    #[arg(long, value_enum)]
    pub framework: Option<FrameworkArg>,
    /// Posterior-mean threshold above which a commit is considered "bad".
    #[arg(long, default_value_t = 0.20)]
    pub bad_threshold: f64,
    /// The test command, after `--`.
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(args: BisectArgs, config: Option<&Path>) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd.clone());

    let command: Vec<String> = if !args.command.is_empty() {
        args.command.clone()
    } else if let Some(c) = &cfg.command {
        shell_words::split(c).map_err(|e| FlaketideError::Config(format!("command parse: {e}")))?
    } else {
        return Err(FlaketideError::Config(
            "no test command — pass after `--` or set `command =` in flaketide.toml".into(),
        ));
    };
    let framework: Option<Framework> = args.framework.map(Into::into).or(cfg.framework);
    let test_id = TestId::from_raw(args.test_id.clone())?;

    eprintln!("flaketide bisect — tracking `{}` from {} to {}",
        test_id, args.from, args.to);
    eprintln!("Each candidate commit will run the test {} times.", args.runs);
    eprintln!();

    let stash_token = stash_save(&root)?;
    let original_ref = git_current_ref(&root)?;
    let result = bisect_loop(
        &root,
        &args.from,
        &args.to,
        &test_id,
        &command,
        framework,
        args.runs,
        args.timeout,
        args.bad_threshold,
    )
    .await;
    // Restore working state regardless of result.
    let _ = git_checkout(&root, &original_ref);
    if stash_token {
        let _ = stash_pop(&root);
    }
    let outcome = result?;

    match outcome {
        BisectOutcome::FoundBad(sha) => {
            println!();
            println!("flaketide bisect found a likely first-bad commit:");
            println!("  {}", sha);
            println!();
            println!("Inspect with: git show {}", sha);
            Ok(0)
        }
        BisectOutcome::NoneFlaky => {
            println!();
            println!("No commit in the range exceeded the {} flake-probability threshold.",
                args.bad_threshold);
            println!("(Try widening --from or lowering --bad-threshold.)");
            Ok(0)
        }
    }
}

enum BisectOutcome {
    FoundBad(String),
    NoneFlaky,
}

#[allow(clippy::too_many_arguments)]
async fn bisect_loop(
    root: &Path,
    from: &str,
    to: &str,
    test_id: &TestId,
    command: &[String],
    framework: Option<Framework>,
    runs_per_commit: u32,
    timeout: Duration,
    bad_threshold: f64,
) -> Result<BisectOutcome> {
    let commits = git_rev_list(root, from, to)?;
    if commits.is_empty() {
        return Err(FlaketideError::Runner("rev-list returned no commits".into()));
    }
    eprintln!("rev-list returned {} commit(s); bisecting…", commits.len());

    let mut lo = 0usize;
    let mut hi = commits.len() - 1;
    let mut last_bad: Option<String> = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let sha = commits[mid].clone();
        eprintln!("  checking {} ({} of remaining range)…", &sha[..sha.len().min(10)], mid);
        git_checkout(root, &sha)?;
        let runner = RepeatRunner::new(RunOptions {
            runs: runs_per_commit,
            parallel: 1,
            isolated: false,
            timeout: Some(timeout),
            tee: false,
            framework,
            command: command.to_vec(),
            working_dir: root.to_path_buf(),
        });
        let outcome = runner.execute().await?;
        let (runs_n, fails) = count_target(&outcome, test_id);
        let posterior = flake_posterior(fails, runs_n);
        eprintln!(
            "    flake_prob={:.2}  ci=[{:.2},{:.2}]  fails={}/{}",
            posterior.mean, posterior.low, posterior.high, fails, runs_n
        );
        if posterior.mean >= bad_threshold {
            last_bad = Some(sha);
            if mid == 0 { break; }
            hi = mid - 1;
        } else if mid == commits.len() - 1 {
            break;
        } else {
            lo = mid + 1;
        }
    }
    Ok(match last_bad {
        Some(sha) => BisectOutcome::FoundBad(sha),
        None => BisectOutcome::NoneFlaky,
    })
}

fn count_target(
    outcome: &crate::runner::RunnerOutcome,
    test_id: &TestId,
) -> (u32, u32) {
    let mut runs = 0u32;
    let mut fails = 0u32;
    for r in &outcome.runs {
        if let Some(t) = r.results.iter().find(|t| t.id.as_str() == test_id.as_str()) {
            runs += 1;
            if t.status.is_failure() {
                fails += 1;
            }
        } else {
            // Test didn't appear in this run; treat as "not measured".
        }
    }
    (runs, fails)
}

fn git_rev_list(root: &Path, from: &str, to: &str) -> Result<Vec<String>> {
    let out = StdCommand::new("git")
        .args(["rev-list", "--reverse", &format!("{from}..{to}")])
        .current_dir(root)
        .output()
        .map_err(|e| FlaketideError::Runner(format!("git rev-list: {e}")))?;
    if !out.status.success() {
        return Err(FlaketideError::Runner(format!(
            "git rev-list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn git_checkout(root: &Path, sha: &str) -> Result<()> {
    let out = StdCommand::new("git")
        .args(["checkout", "--quiet", sha])
        .current_dir(root)
        .output()
        .map_err(|e| FlaketideError::Runner(format!("git checkout: {e}")))?;
    if !out.status.success() {
        return Err(FlaketideError::Runner(format!(
            "git checkout {sha} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

fn git_current_ref(root: &Path) -> Result<String> {
    let out = StdCommand::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| FlaketideError::Runner(format!("git rev-parse: {e}")))?;
    if !out.status.success() {
        return Err(FlaketideError::Runner("git rev-parse failed".into()));
    }
    let r = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if r == "HEAD" {
        // Detached. Fall back to SHA.
        let sha = StdCommand::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()
            .map_err(|e| FlaketideError::Runner(format!("git rev-parse HEAD: {e}")))?;
        Ok(String::from_utf8_lossy(&sha.stdout).trim().to_string())
    } else {
        Ok(r)
    }
}

fn stash_save(root: &Path) -> Result<bool> {
    let out = StdCommand::new("git")
        .args(["stash", "push", "-u", "-m", "flaketide-bisect-autostash"])
        .current_dir(root)
        .output()
        .map_err(|e| FlaketideError::Runner(format!("git stash: {e}")))?;
    let txt = String::from_utf8_lossy(&out.stdout);
    Ok(!txt.contains("No local changes to save"))
}

fn stash_pop(root: &Path) -> Result<()> {
    let _ = StdCommand::new("git")
        .args(["stash", "pop"])
        .current_dir(root)
        .output();
    Ok(())
}