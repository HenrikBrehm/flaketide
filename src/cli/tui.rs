//! `flaketide tui` — interactive ratatui explorer.

use std::path::Path;

use clap::Args;

use crate::config::load_config;
use crate::error::Result;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct TuiArgs {}

pub async fn run(_args: TuiArgs, config: Option<&Path>) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;
    crate::tui::run_tui(store, cfg).await?;
    Ok(0)
}
