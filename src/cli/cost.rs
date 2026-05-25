//! `flaketide cost` — translate flake debt into CI-runner dollars.
//!
//! The flake tax is invisible to engineering leadership until you put a
//! dollar sign in front of it. This subcommand walks the history and
//! multiplies wasted CI minutes (failed-run duration + repeat-run cost)
//! by a configurable $/minute rate.

use std::path::Path;

use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use serde::Serialize;

use crate::config::load_config;
use crate::error::Result;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct CostArgs {
    /// Days of history to consider.
    #[arg(long, default_value_t = 30)]
    pub days: u32,
    /// Cost per minute of CI runner time in USD (GitHub Actions Linux = $0.008,
    /// Windows/macOS larger; tune for your provider).
    #[arg(long, default_value_t = 0.008)]
    pub rate_per_minute_usd: f64,
    /// Average extra runs each flake triggers (CI re-runs, developer retries).
    #[arg(long, default_value_t = 2.0)]
    pub avg_retries_per_flake: f64,
}

#[derive(Debug, Serialize)]
pub struct CostReport {
    pub days_considered: u32,
    pub runs_considered: u64,
    pub total_test_minutes_wasted: f64,
    pub estimated_monthly_cost_usd: f64,
    pub top_offenders: Vec<TestCost>,
}

#[derive(Debug, Serialize)]
pub struct TestCost {
    pub test_id: String,
    pub failure_count: u32,
    pub avg_duration_ms: u64,
    pub monthly_cost_usd: f64,
}

pub async fn run(args: CostArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let runs = store.history(args.days, None).await?;
    let runs_considered = runs.len() as u64;

    use std::collections::HashMap;
    let mut by_test: HashMap<String, (u32, u128, u128)> = HashMap::new(); // (fail_count, total_failed_ms, total_ms)
    for run in &runs {
        for r in &run.results {
            let entry = by_test.entry(r.id.as_str().to_string()).or_insert((0, 0, 0));
            entry.2 += r.duration.as_millis();
            if r.status.is_failure() {
                entry.0 += 1;
                entry.1 += r.duration.as_millis();
            }
        }
    }

    // Cost model: wasted_ms = failed_duration_ms × (1 + avg_retries) (every retry pays again)
    let scale = 1.0 + args.avg_retries_per_flake;
    let mut top: Vec<TestCost> = by_test
        .into_iter()
        .filter(|(_, (failures, _, _))| *failures > 0)
        .map(|(test_id, (failures, total_failed_ms, _))| {
            let wasted_ms = total_failed_ms as f64 * scale;
            let cost = (wasted_ms / 60_000.0) * args.rate_per_minute_usd;
            let monthly = cost * (30.0 / args.days as f64);
            TestCost {
                test_id,
                failure_count: failures,
                avg_duration_ms: if failures > 0 {
                    (total_failed_ms / failures as u128) as u64
                } else {
                    0
                },
                monthly_cost_usd: monthly,
            }
        })
        .collect();
    top.sort_by(|a, b| b.monthly_cost_usd.partial_cmp(&a.monthly_cost_usd).unwrap_or(std::cmp::Ordering::Equal));
    let total_test_minutes_wasted: f64 = top.iter().map(|t| t.monthly_cost_usd / args.rate_per_minute_usd).sum();
    let monthly_total: f64 = top.iter().map(|t| t.monthly_cost_usd).sum();

    let report = CostReport {
        days_considered: args.days,
        runs_considered,
        total_test_minutes_wasted,
        estimated_monthly_cost_usd: monthly_total,
        top_offenders: top.iter().take(20).cloned().collect(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!();
        println!("flaketide cost — over the last {} days", args.days);
        println!("======================================");
        println!("Runs considered:           {:>10}", report.runs_considered);
        println!("Wasted test-minutes/mo:    {:>10.1}", report.total_test_minutes_wasted);
        println!("Estimated monthly cost:   ${:>10.2}", report.estimated_monthly_cost_usd);
        println!("Rate used:                ${:.4}/minute   (--rate-per-minute-usd to tune)", args.rate_per_minute_usd);
        println!("Retry multiplier:          {:.1}x          (--avg-retries-per-flake to tune)", scale);
        println!();
        if report.top_offenders.is_empty() {
            println!("No failing tests in the window — congratulations, you owe zero dollars.");
        } else {
            println!("Top offenders by $/month:");
            let mut table = Table::new();
            table.load_preset(UTF8_FULL).set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["$/mo", "Fails", "Avg dur", "Test"]);
            for t in &report.top_offenders {
                table.add_row(vec![
                    Cell::new(format!("${:.2}", t.monthly_cost_usd)),
                    Cell::new(t.failure_count.to_string()),
                    Cell::new(format!("{} ms", t.avg_duration_ms)),
                    Cell::new(truncate(&t.test_id, 80)),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(0)
}

impl Clone for TestCost {
    fn clone(&self) -> Self {
        Self {
            test_id: self.test_id.clone(),
            failure_count: self.failure_count,
            avg_duration_ms: self.avg_duration_ms,
            monthly_cost_usd: self.monthly_cost_usd,
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}