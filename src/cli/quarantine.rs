//! `flaketide quarantine` — add/list/remove quarantine entries; emit framework-specific skip annotations.

use std::path::Path;

use clap::{Args, Subcommand, ValueEnum};

use crate::cli::FrameworkArg;
use crate::config::load_config;
use crate::domain::{Framework, TestId};
use crate::error::Result;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Subcommand)]
pub enum QuarantineCmd {
    /// Add a test to the quarantine list and persist the debt entry.
    Add(AddArgs),
    /// List current quarantine debt.
    List(ListArgs),
    /// Emit the framework-specific skip annotation for a test (stdout).
    Emit(EmitArgs),
    /// Remove a quarantine entry.
    Remove(RemoveArgs),
}

#[derive(Debug, Args)]
pub struct AddArgs {
    pub test_id: String,
    #[arg(long, value_enum)]
    pub framework: FrameworkArg,
    #[arg(long)]
    pub reason: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ListFormat { Table, Json, Markdown }

#[derive(Debug, Args)]
pub struct EmitArgs {
    pub test_id: String,
    #[arg(long, value_enum)]
    pub framework: FrameworkArg,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    pub test_id: String,
}

pub async fn run(cmd: QuarantineCmd, config: Option<&Path>, json: bool) -> Result<i32> {
    // `Emit` is pure — it just prints a snippet — so we skip the DB entirely.
    // Opening the store eagerly would cause SQLite-lock races when several
    // `quarantine emit` commands run in parallel (e.g. in a test suite).
    if let QuarantineCmd::Emit(a) = &cmd {
        let fw: Framework = a.framework.into();
        let id = TestId::from_raw(a.test_id.clone())?;
        let snippet = crate::quarantine::emit::emit_skip(fw, &id, "flaketide-quarantined");
        println!("{snippet}");
        return Ok(0);
    }

    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    match cmd {
        QuarantineCmd::Add(a) => {
            let fw: Framework = a.framework.into();
            let id = TestId::from_raw(a.test_id)?;
            let prob = store.flake_prob_for(&id).await?.unwrap_or(0.0);
            let author = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok();
            let entry = crate::domain::QuarantineEntry {
                id,
                framework: fw,
                reason: a.reason,
                created_at: chrono::Utc::now(),
                flake_prob_at_quarantine: prob,
                author,
                linked_issue_url: None,
            };
            store.add_quarantine(&entry).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&entry)?);
            } else {
                println!("quarantined {} ({})", entry.id, entry.framework);
            }
            Ok(0)
        }
        QuarantineCmd::List(a) => {
            let entries = store.list_quarantine().await?;
            match (a.format, json) {
                (ListFormat::Json, _) | (_, true) => {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                }
                (ListFormat::Markdown, _) => {
                    println!("{}", crate::report::quarantine_markdown(&entries));
                }
                (ListFormat::Table, _) => {
                    crate::report::print_quarantine_table(&entries);
                }
            }
            Ok(0)
        }
        QuarantineCmd::Emit(_) => unreachable!("handled above before opening the DB"),
        QuarantineCmd::Remove(a) => {
            let id = TestId::from_raw(a.test_id)?;
            let removed = store.remove_quarantine(&id).await?;
            if removed {
                println!("removed {}", id);
                Ok(0)
            } else {
                println!("no entry for {}", id);
                Ok(0)
            }
        }
    }
}
