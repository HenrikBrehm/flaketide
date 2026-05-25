//! `flaketide tag` — heuristic flake-pattern detection without any LLM.
//!
//! Walks the local history and tags tests with heuristic patterns based
//! purely on observed correlations:
//!
//!   - `scheduling`  — failures cluster around a particular hour-of-day
//!                     (e.g. only fails at midnight UTC when crons run)
//!   - `ordering`    — failures consistently co-occur with another test
//!                     failing in the same run (suggests global state)
//!   - `flapping`    — failures alternate frequently with passes inside
//!                     the same run-window (high variance, low memory)
//!   - `worsening`   — flake rate has trended up over the window
//!
//! Output is a per-test tag-list. Combine with `flaketide analyze` for
//! deeper LLM-based classification; this command is the AI-free fallback.

use std::collections::HashMap;
use std::path::Path;

use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use serde::Serialize;

use crate::config::load_config;
use crate::error::Result;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct TagArgs {
    /// History window in days.
    #[arg(long, default_value_t = 30)]
    pub days: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct TestTags {
    pub test_id: String,
    pub tags: Vec<String>,
    pub evidence: Vec<String>,
}

pub async fn run(args: TagArgs, config: Option<&Path>, json: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;

    let runs = store.history(args.days, None).await?;
    if runs.is_empty() {
        println!("(no history in the last {} days)", args.days);
        return Ok(0);
    }

    let mut tagged: Vec<TestTags> = Vec::new();
    let observations = analyze_runs(&runs);
    for (test_id, obs) in observations {
        let mut tags = Vec::new();
        let mut ev = Vec::new();

        if let Some((hour, ratio)) = scheduling_signal(&obs.failure_hours) {
            tags.push("scheduling".into());
            ev.push(format!("{:.0}% of failures clustered at hour {} UTC", ratio * 100.0, hour));
        }
        if let Some((co_test, count)) = ordering_signal(&obs.co_failures) {
            tags.push("ordering".into());
            ev.push(format!("co-failed with `{}` in {} runs", co_test, count));
        }
        if flapping_signal(&obs.history_pattern) {
            tags.push("flapping".into());
            ev.push("alternates pass/fail rapidly (≥6 transitions in window)".into());
        }
        if let Some(delta) = worsening_signal(&obs.history_pattern) {
            tags.push("worsening".into());
            ev.push(format!(
                "flake rate up {:+.0}% comparing first vs second half of window",
                delta * 100.0
            ));
        }

        if !tags.is_empty() {
            tagged.push(TestTags {
                test_id,
                tags,
                evidence: ev,
            });
        }
    }
    tagged.sort_by(|a, b| b.tags.len().cmp(&a.tags.len()));

    if json {
        println!("{}", serde_json::to_string_pretty(&tagged)?);
    } else if tagged.is_empty() {
        println!("(no patterns detected in the last {} days)", args.days);
    } else {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL).set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Test", "Tags", "Evidence"]);
        for t in &tagged {
            table.add_row(vec![
                Cell::new(truncate(&t.test_id, 60)),
                Cell::new(t.tags.join(", ")),
                Cell::new(t.evidence.join("\n")),
            ]);
        }
        println!("{table}");
    }
    Ok(0)
}

struct Observation {
    failure_hours: Vec<u32>,             // UTC hours when failures occurred
    co_failures: HashMap<String, u32>,   // test_id → count
    history_pattern: Vec<bool>,          // chronological pass(false)/fail(true)
}

fn analyze_runs(runs: &[crate::domain::TestRun]) -> Vec<(String, Observation)> {
    let mut by_test: HashMap<String, Observation> = HashMap::new();
    // Sort runs chronologically.
    let mut sorted: Vec<&crate::domain::TestRun> = runs.iter().collect();
    sorted.sort_by_key(|r| r.started_at);
    for run in &sorted {
        let failing_ids: Vec<String> = run
            .results
            .iter()
            .filter(|r| r.status.is_failure())
            .map(|r| r.id.as_str().to_string())
            .collect();
        let hour = run.started_at.format("%H").to_string().parse::<u32>().unwrap_or(0);
        for r in &run.results {
            let entry = by_test.entry(r.id.as_str().to_string()).or_insert(Observation {
                failure_hours: Vec::new(),
                co_failures: HashMap::new(),
                history_pattern: Vec::new(),
            });
            entry.history_pattern.push(r.status.is_failure());
            if r.status.is_failure() {
                entry.failure_hours.push(hour);
                for other in &failing_ids {
                    if other != r.id.as_str() {
                        *entry.co_failures.entry(other.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    by_test.into_iter().collect()
}

fn scheduling_signal(hours: &[u32]) -> Option<(u32, f64)> {
    if hours.len() < 5 {
        return None;
    }
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for h in hours {
        *counts.entry(*h).or_insert(0) += 1;
    }
    let total = hours.len() as f64;
    let (peak_h, peak_c) = counts.into_iter().max_by_key(|(_, c)| *c)?;
    let ratio = peak_c as f64 / total;
    if ratio >= 0.5 {
        Some((peak_h, ratio))
    } else {
        None
    }
}

fn ordering_signal(co: &HashMap<String, u32>) -> Option<(String, u32)> {
    co.iter()
        .filter(|(_, &c)| c >= 3)
        .max_by_key(|(_, c)| **c)
        .map(|(t, c)| (t.clone(), *c))
}

fn flapping_signal(pattern: &[bool]) -> bool {
    if pattern.len() < 6 {
        return false;
    }
    let transitions = pattern.windows(2).filter(|w| w[0] != w[1]).count();
    transitions >= 6
}

fn worsening_signal(pattern: &[bool]) -> Option<f64> {
    if pattern.len() < 8 {
        return None;
    }
    let mid = pattern.len() / 2;
    let (first, second) = pattern.split_at(mid);
    let rate = |slice: &[bool]| -> f64 {
        slice.iter().filter(|x| **x).count() as f64 / slice.len() as f64
    };
    let delta = rate(second) - rate(first);
    if delta >= 0.20 { Some(delta) } else { None }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduling_detects_hour_cluster() {
        let hours = vec![3, 3, 3, 3, 3, 11];
        let (h, r) = scheduling_signal(&hours).unwrap();
        assert_eq!(h, 3);
        assert!(r > 0.8);
    }

    #[test]
    fn flapping_detects_high_transitions() {
        let pat = vec![true, false, true, false, true, false, true, false];
        assert!(flapping_signal(&pat));
    }

    #[test]
    fn flapping_does_not_fire_on_stable() {
        let pat = vec![false, false, false, false, true, true, true, true];
        assert!(!flapping_signal(&pat));
    }

    #[test]
    fn worsening_detects_rising_failure_rate() {
        let mut pat: Vec<bool> = vec![false; 5];
        pat.extend(std::iter::repeat(true).take(5));
        let delta = worsening_signal(&pat).unwrap();
        assert!(delta >= 0.5);
    }
}