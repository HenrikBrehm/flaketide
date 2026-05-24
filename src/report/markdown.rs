//! Markdown report rendering.

use crate::domain::{FlakeReport, QuarantineEntry};

pub fn render(verdicts: &[FlakeReport], quarantine: &[QuarantineEntry]) -> String {
    let mut s = String::new();
    s.push_str("# flaketide report\n\n");
    s.push_str(&format!("Generated: `{}`\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

    s.push_str("## Top flaky tests\n\n");
    if verdicts.is_empty() {
        s.push_str("_No flaky tests detected._\n\n");
    } else {
        s.push_str("| Severity | Prob | 95% CI | Runs (f/n) | Test |\n|---|---|---|---|---|\n");
        for v in verdicts.iter().take(50) {
            s.push_str(&format!(
                "| {:.2} | {:.2} | [{:.2}, {:.2}] | {}/{} | `{}` |\n",
                v.severity, v.flake_prob, v.hdi_low, v.hdi_high,
                v.failures, v.runs, v.id.as_str()
            ));
        }
        s.push('\n');
        s.push_str("### Recent failure messages (top 10 tests)\n\n");
        for v in verdicts.iter().take(10) {
            if v.recent_messages.is_empty() { continue; }
            s.push_str(&format!("**`{}`**\n", v.id.as_str()));
            for m in &v.recent_messages {
                let one_line: String = m.lines().next().unwrap_or("").chars().take(200).collect();
                s.push_str(&format!("- {}\n", one_line));
            }
            s.push('\n');
        }
    }

    s.push_str("## Quarantine debt\n\n");
    if quarantine.is_empty() {
        s.push_str("_No quarantine entries._\n");
    } else {
        s.push_str(&super::quarantine_markdown(quarantine));
    }
    s
}
