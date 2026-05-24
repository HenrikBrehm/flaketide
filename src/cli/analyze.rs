//! `flaketide analyze` — classify a flaky test's root cause via Claude API.

use std::path::Path;

use clap::Args;

use crate::ai::{Classifier, ClassifyInput};
use crate::config::load_config;
use crate::error::{FlaketideError, Result};
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct AnalyzeArgs {
    /// Specific test id ("suite::name"). Default: highest-severity test.
    pub test_id: Option<String>,
    /// Override the AI model.
    #[arg(long)]
    pub model: Option<String>,
    /// Bypass the cache and re-call the API.
    #[arg(long)]
    pub no_cache: bool,
}

pub async fn run(args: AnalyzeArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let target = match args.test_id {
        Some(s) => crate::domain::TestId::from_raw(s)?,
        None => store
            .top_flake()
            .await?
            .ok_or_else(|| FlaketideError::NotFound("no flaky tests in history".into()))?
            .id,
    };

    let input = ClassifyInput::from_store(&store, &target, cfg.ai.max_log_excerpt_chars).await?;
    let mut ai_cfg = cfg.ai.clone();
    if let Some(m) = args.model {
        ai_cfg.model = m;
    }
    let classifier = Classifier::from_env(ai_cfg)?;
    let verdict = classifier
        .classify(&store, &input, !args.no_cache)
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&verdict)?);
    } else {
        println!("{}", verdict.render_markdown());
    }
    Ok(0)
}
