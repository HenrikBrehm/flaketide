//! Emit a JUnit-compatible XML summary so flaketide regressions can be surfaced
//! by any CI consumer that already parses JUnit.

use crate::domain::{FlakeReport, Thresholds};

pub fn render(verdicts: &[FlakeReport], thr: &Thresholds) -> String {
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    s.push('\n');
    let failures = verdicts.iter().filter(|v| v.severity >= thr.severity_alert).count();
    s.push_str(&format!(
        r#"<testsuites name="flaketide" tests="{}" failures="{}" errors="0">"#,
        verdicts.len(), failures
    ));
    s.push('\n');
    s.push_str(r#"  <testsuite name="flake-verdicts" tests=""#);
    s.push_str(&verdicts.len().to_string());
    s.push_str(r#"" failures=""#);
    s.push_str(&failures.to_string());
    s.push_str(r#"">"#);
    s.push('\n');
    for v in verdicts {
        let testname = escape_xml(v.id.as_str());
        if v.severity >= thr.severity_alert {
            s.push_str(&format!(
                r#"    <testcase classname="flaketide" name="{}" time="0">"#,
                testname
            ));
            s.push('\n');
            s.push_str(&format!(
                r#"      <failure message="flaky">severity={:.2} prob={:.2} runs={}/{}</failure>"#,
                v.severity, v.flake_prob, v.failures, v.runs
            ));
            s.push('\n');
            s.push_str("    </testcase>\n");
        } else {
            s.push_str(&format!(
                r#"    <testcase classname="flaketide" name="{}" time="0"/>"#,
                testname
            ));
            s.push('\n');
        }
    }
    s.push_str("  </testsuite>\n");
    s.push_str("</testsuites>\n");
    s
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TestId;

    fn vd(sev: f64) -> FlakeReport {
        let now = chrono::Utc::now();
        FlakeReport::new(
            TestId::from_raw("s::t").unwrap(),
            10, 3, 0.3, 0.1, 0.5, sev, now, now, vec![],
        ).unwrap()
    }

    #[test]
    fn marks_above_threshold_as_failure() {
        let xml = render(&[vd(0.9)], &Thresholds::default());
        assert!(xml.contains("<failure"));
        assert!(xml.contains("severity=0.90"));
    }

    #[test]
    fn no_failure_below_threshold() {
        let xml = render(&[vd(0.1)], &Thresholds::default());
        assert!(!xml.contains("<failure"));
    }
}
