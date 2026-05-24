//! `flaketide history` — show recent runs.

use std::path::Path;

use clap::Args;

use crate::config::load_config;
use crate::error::Result;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Days back to include.
    #[arg(long, default_value_t = 30)]
    pub days: u32,
    /// Restrict to a single test id.
    pub test_id: Option<String>,
}

pub async fn run(args: HistoryArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;
    let runs = store.history(args.days, args.test_id.as_deref()).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
    } else {
        for r in &runs {
            println!(
                "run #{:>5}  {}  exit={}  results={}",
                r.run_id, r.started_at, r.exit_code, r.results.len()
            );
        }
    }
    Ok(0)
}
