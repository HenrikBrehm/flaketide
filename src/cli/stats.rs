//! `flaketide stats` — print current flake verdicts.

use std::path::Path;

use clap::Args;

use crate::config::load_config;
use crate::error::Result;
use crate::stats;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct StatsArgs {
    pub test_id: Option<String>,
}

pub async fn run(args: StatsArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let verdicts = stats::summarize(&store, &cfg.thresholds).await?;
    let filtered: Vec<_> = match args.test_id {
        Some(id) => verdicts.into_iter().filter(|v| v.id.as_str() == id).collect(),
        None => verdicts,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else {
        crate::report::print_flake_table(&filtered);
    }
    Ok(0)
}
