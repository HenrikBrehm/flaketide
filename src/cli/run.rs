//! `flaketide run` — execute the test command N times, persist, report flakes.

use std::path::Path;
use std::time::Duration;

use clap::Args;

use crate::cli::FrameworkArg;
use crate::config::load_config;
use crate::domain::Framework;
use crate::error::{FlaketideError, Result};
use crate::runner::{RepeatRunner, RunOptions};
use crate::stats;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Number of times to repeat the suite.
    #[arg(short = 'n', long)]
    pub runs: Option<u32>,
    /// Maximum concurrent runs (default 1 — serial).
    #[arg(long)]
    pub parallel: Option<u8>,
    /// Allocate a unique TMPDIR per run.
    #[arg(long)]
    pub isolated: bool,
    /// Wall-clock per-run timeout (e.g. 2m, 30s).
    #[arg(long, value_parser = crate::cli::parse_duration)]
    pub timeout: Option<Duration>,
    /// Mirror child stdout/stderr to our stdout.
    #[arg(long)]
    pub tee: bool,
    /// Test framework. Default: from config or auto-detect.
    #[arg(long, value_enum)]
    pub framework: Option<FrameworkArg>,
    /// The test command, after `--`.
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(args: RunArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;

    let command_parts = if !args.command.is_empty() {
        args.command.clone()
    } else if let Some(c) = &cfg.command {
        shell_words::split(c).map_err(|e| FlaketideError::Config(format!("command parse: {e}")))?
    } else {
        return Err(FlaketideError::Config(
            "no test command — pass after `--` or set `command =` in flaketide.toml".into(),
        ));
    };
    if command_parts.is_empty() {
        return Err(FlaketideError::Config("empty test command".into()));
    }

    let framework: Option<Framework> = args.framework.map(Into::into).or(cfg.framework);

    let runs = args.runs.unwrap_or(cfg.runs);
    let parallel = args.parallel.unwrap_or(cfg.parallel).max(1);
    let timeout = args.timeout.or(cfg.timeout);

    let root = find_repo_root(&cwd).unwrap_or(cwd.clone());
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let options = RunOptions {
        runs,
        parallel,
        isolated: args.isolated,
        timeout,
        tee: args.tee,
        framework,
        command: command_parts.clone(),
        working_dir: cwd.clone(),
    };

    let runner = RepeatRunner::new(options);
    let outcome = runner.execute().await?;

    let recorded_runs = outcome.runs.len();
    let mut record_ids = Vec::with_capacity(outcome.runs.len());
    for r in &outcome.runs {
        let id = store.record_run(r).await?;
        record_ids.push(id);
    }

    let verdicts = stats::summarize(&store, &cfg.thresholds).await?;
    store.upsert_flake_verdicts(&verdicts).await?;

    if json {
        let out = serde_json::json!({
            "runs_completed": recorded_runs,
            "flaky_tests": verdicts.len(),
            "verdicts": verdicts,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("flaketide: completed {recorded_runs} runs, {} flaky tests detected", verdicts.len());
        crate::report::print_flake_table(&verdicts);
    }

    Ok(0)
}
