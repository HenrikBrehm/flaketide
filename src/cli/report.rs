//! `flaketide report` — render summaries or sync a GitHub issue.

use std::path::Path;

use clap::Args;

use crate::config::load_config;
use crate::error::Result;
use crate::stats;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Sync the report to a GitHub issue (requires GITHUB_TOKEN).
    #[arg(long)]
    pub github: bool,
    /// Print a Markdown report to stdout.
    #[arg(long, default_value_t = true)]
    pub markdown: bool,
    /// owner/repo override.
    #[arg(long)]
    pub repo: Option<String>,
    /// Custom issue title.
    #[arg(long)]
    pub issue_title: Option<String>,
}

pub async fn run(args: ReportArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let verdicts = stats::summarize(&store, &cfg.thresholds).await?;
    let quarantine = store.list_quarantine().await?;

    if args.github {
        let mut gh_cfg = cfg.github.clone();
        if let Some(r) = args.repo {
            gh_cfg.repo = Some(r);
        }
        if let Some(t) = args.issue_title {
            gh_cfg.issue_title = t;
        }
        let url = crate::report::github::sync_issue(&gh_cfg, &verdicts, &quarantine).await?;
        println!("{url}");
        return Ok(0);
    }

    if json {
        let out = serde_json::json!({
            "verdicts": verdicts,
            "quarantine": quarantine,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else if args.markdown {
        println!("{}", crate::report::markdown::render(&verdicts, &quarantine));
    }
    Ok(0)
}
