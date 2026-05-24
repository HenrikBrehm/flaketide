//! `flaketide ci` — CI-friendly pipeline: run + summarize + emit JUnit/JSON + exit non-zero on regressions.

use std::path::{Path, PathBuf};

use clap::Args;

use crate::config::load_config;
use crate::error::Result;
use crate::stats;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct CiArgs {
    /// Path to a previous run's JSON baseline. Regressions are tests that became flaky relative to it.
    #[arg(long)]
    pub baseline: Option<PathBuf>,
    /// Write a JUnit-compatible XML report.
    #[arg(long)]
    pub junit_out: Option<PathBuf>,
    /// Write a JSON report.
    #[arg(long)]
    pub json_out: Option<PathBuf>,
}

pub async fn run(args: CiArgs, config: Option<&Path>) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let verdicts = stats::summarize(&store, &cfg.thresholds).await?;

    if let Some(out) = &args.junit_out {
        let xml = crate::report::junit_out::render(&verdicts, &cfg.thresholds);
        std::fs::write(out, xml)?;
    }
    if let Some(out) = &args.json_out {
        let txt = serde_json::to_string_pretty(&verdicts)?;
        std::fs::write(out, txt)?;
    }

    let regression = match args.baseline.as_ref() {
        Some(base) => crate::report::ci_regression_count(&verdicts, base, &cfg.thresholds)?,
        None => 0,
    };

    eprintln!(
        "flaketide ci: {} flaky test(s); {} regression(s) vs baseline",
        verdicts.len(),
        regression
    );

    if regression > 0 {
        return Ok(4);
    }
    Ok(0)
}
