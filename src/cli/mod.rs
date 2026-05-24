//! Command-line interface — top-level [`Cli`] parser and [`dispatch`] entry point.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use crate::domain::Framework;
use crate::error::Result;

pub mod analyze;
pub mod ci;
pub mod history;
pub mod init;
pub mod quarantine;
pub mod report;
pub mod run;
pub mod stats;
pub mod tui;

#[derive(Debug, Parser)]
#[command(
    name = "flaketide",
    version,
    about = "Cross-framework flaky-test intelligence",
    long_about = "flaketide runs your test command N times, learns which tests are flaky \
                  via a Beta-Binomial model, persists results in local SQLite, and \
                  optionally classifies root causes via the Claude API."
)]
pub struct Cli {
    /// Path to a flaketide.toml. Default: walk up from cwd to the repo root.
    #[arg(long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,

    /// Emit machine-readable JSON instead of human-friendly output where applicable.
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase log verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Initialize flaketide in the current repo (writes flaketide.toml).
    Init(init::InitArgs),
    /// Run the test suite N times, parse results, persist, and report flakes.
    Run(run::RunArgs),
    /// Classify failure root cause with the Claude API.
    Analyze(analyze::AnalyzeArgs),
    /// Open the interactive TUI explorer.
    Tui(tui::TuiArgs),
    /// Manage quarantine entries.
    #[command(subcommand)]
    Quarantine(quarantine::QuarantineCmd),
    /// CI mode — runs the pipeline, exits non-zero on regression.
    Ci(ci::CiArgs),
    /// Render a report (markdown/json) or sync a GitHub issue.
    Report(report::ReportArgs),
    /// Show historical run data.
    History(history::HistoryArgs),
    /// Show current flake statistics.
    Stats(stats::StatsArgs),
    /// Generate shell completions and print to stdout.
    Completions {
        /// Target shell.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Generate manpages into the given directory.
    Man {
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum FrameworkArg {
    Jest,
    Vitest,
    PytestJunit,
    PytestJson,
    Gotest,
    Cargo,
    Nextest,
    JunitXml,
}

impl From<FrameworkArg> for Framework {
    fn from(a: FrameworkArg) -> Self {
        match a {
            FrameworkArg::Jest => Framework::Jest,
            FrameworkArg::Vitest => Framework::Vitest,
            FrameworkArg::PytestJunit => Framework::PytestJunit,
            FrameworkArg::PytestJson => Framework::PytestJson,
            FrameworkArg::Gotest => Framework::GoTest,
            FrameworkArg::Cargo => Framework::Cargo,
            FrameworkArg::Nextest => Framework::Nextest,
            FrameworkArg::JunitXml => Framework::JunitXml,
        }
    }
}

/// Parse `humantime`-style duration from CLI flag value.
pub fn parse_duration(s: &str) -> std::result::Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| e.to_string())
}

/// Dispatch the parsed CLI to its subcommand handler. Returns process exit code.
pub async fn dispatch(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Cmd::Init(a) => init::run(a, cli.config.as_deref()).await,
        Cmd::Run(a) => run::run(a, cli.config.as_deref(), cli.json).await,
        Cmd::Analyze(a) => analyze::run(a, cli.config.as_deref(), cli.json).await,
        Cmd::Tui(a) => tui::run(a, cli.config.as_deref()).await,
        Cmd::Quarantine(a) => quarantine::run(a, cli.config.as_deref(), cli.json).await,
        Cmd::Ci(a) => ci::run(a, cli.config.as_deref()).await,
        Cmd::Report(a) => report::run(a, cli.config.as_deref(), cli.json).await,
        Cmd::History(a) => history::run(a, cli.config.as_deref(), cli.json).await,
        Cmd::Stats(a) => stats::run(a, cli.config.as_deref(), cli.json).await,
        Cmd::Completions { shell } => completions_cmd(shell),
        Cmd::Man { out_dir } => man_cmd(&out_dir),
    }
}

fn completions_cmd(shell: clap_complete::Shell) -> Result<i32> {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "flaketide", &mut std::io::stdout());
    Ok(0)
}

fn man_cmd(out_dir: &std::path::Path) -> Result<i32> {
    use clap::CommandFactory;
    std::fs::create_dir_all(out_dir)?;
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd.clone());
    let mut buf = Vec::new();
    man.render(&mut buf).map_err(|e| crate::FlaketideError::Other(e.into()))?;
    let main_path = out_dir.join("flaketide.1");
    std::fs::write(&main_path, buf)?;
    // Sub-commands
    for sub in cmd.get_subcommands() {
        let name = format!("flaketide-{}", sub.get_name());
        let sub_cmd = sub.clone().name(name.clone());
        let man = clap_mangen::Man::new(sub_cmd);
        let mut buf = Vec::new();
        man.render(&mut buf).map_err(|e| crate::FlaketideError::Other(e.into()))?;
        let p = out_dir.join(format!("{name}.1"));
        std::fs::write(&p, buf)?;
    }
    eprintln!("wrote manpages to {}", out_dir.display());
    Ok(0)
}
