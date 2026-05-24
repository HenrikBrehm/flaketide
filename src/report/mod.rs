//! Output renderers: stdout tables, Markdown, JSON, JUnit XML, GitHub issue sync.

use std::path::Path;

use comfy_table::{Cell, ContentArrangement, Table, presets::UTF8_FULL};

use crate::domain::{FlakeReport, QuarantineEntry, Thresholds};
use crate::error::Result;

pub mod github;
pub mod json_out;
pub mod junit_out;
pub mod markdown;

pub fn print_flake_table(verdicts: &[FlakeReport]) {
    if verdicts.is_empty() {
        println!("(no flaky tests)");
        return;
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL).set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Severity", "Prob", "95% CI", "Runs (f/n)", "Test"]);
    for v in verdicts {
        table.add_row(vec![
            Cell::new(format!("{:.2}", v.severity)),
            Cell::new(format!("{:.2}", v.flake_prob)),
            Cell::new(format!("[{:.2}, {:.2}]", v.hdi_low, v.hdi_high)),
            Cell::new(format!("{}/{}", v.failures, v.runs)),
            Cell::new(truncate(&v.id.0, 80)),
        ]);
    }
    println!("{table}");
}

pub fn print_quarantine_table(entries: &[QuarantineEntry]) {
    if entries.is_empty() {
        println!("(no quarantine entries)");
        return;
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL).set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Test", "Framework", "Reason", "Created", "Author"]);
    for e in entries {
        table.add_row(vec![
            Cell::new(truncate(&e.id.0, 60)),
            Cell::new(e.framework.as_str()),
            Cell::new(truncate(&e.reason, 60)),
            Cell::new(e.created_at.format("%Y-%m-%d").to_string()),
            Cell::new(e.author.clone().unwrap_or_default()),
        ]);
    }
    println!("{table}");
}

pub fn quarantine_markdown(entries: &[QuarantineEntry]) -> String {
    let mut s = String::from("| Test | Framework | Reason | Created | Author |\n|---|---|---|---|---|\n");
    for e in entries {
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            e.id.as_str(),
            e.framework,
            e.reason.replace('|', "\\|"),
            e.created_at.format("%Y-%m-%d"),
            e.author.clone().unwrap_or_default(),
        ));
    }
    s
}

/// Compare current verdicts to a JSON baseline file and return regression count.
/// A regression = test whose severity exceeds `severity_alert` and was not present in baseline,
/// OR severity increased by more than 50% relative to baseline.
pub fn ci_regression_count(
    verdicts: &[FlakeReport],
    baseline_path: &Path,
    thr: &Thresholds,
) -> Result<u32> {
    let raw = std::fs::read_to_string(baseline_path)?;
    let baseline: Vec<FlakeReport> = serde_json::from_str(&raw)?;
    let baseline_map: std::collections::HashMap<&str, &FlakeReport> =
        baseline.iter().map(|v| (v.id.as_str(), v)).collect();

    let mut regressions = 0u32;
    for v in verdicts {
        if v.severity < thr.severity_alert {
            continue;
        }
        match baseline_map.get(v.id.as_str()) {
            None => regressions += 1,
            Some(prev) if v.severity > prev.severity * 1.5 => regressions += 1,
            _ => {}
        }
    }
    Ok(regressions)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}
