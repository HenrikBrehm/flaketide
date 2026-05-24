//! Auto-detect the test framework from raw output bytes.

use crate::domain::Framework;

/// Best-effort sniff based on the first ~8 KiB. Returns None if no strong signal.
pub fn detect(raw: &[u8]) -> Option<Framework> {
    let sample = &raw[..raw.len().min(8192)];

    // XML: JUnit XML (Maven/Gradle/pytest --junit-xml all share schema).
    if let Some(trimmed) = trim_leading_whitespace(sample) {
        if trimmed.starts_with(b"<?xml") || trimmed.starts_with(b"<testsuites") || trimmed.starts_with(b"<testsuite") {
            return Some(Framework::JunitXml);
        }
    }

    // JSON Lines: cargo libtest, nextest, go test.
    if first_nonblank_line_starts_with_brace(sample) {
        let mut is_lines = false;
        let mut nextest = false;
        let mut go_test = false;
        let mut libtest = false;
        for line in sample.split(|&b| b == b'\n').take(20) {
            if line.is_empty() {
                continue;
            }
            is_lines = true;
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(line) {
                if v.get("Action").is_some() && v.get("Package").is_some() {
                    go_test = true;
                }
                if v.get("type").and_then(|s| s.as_str()) == Some("suite")
                    || v.get("type").and_then(|s| s.as_str()) == Some("test")
                {
                    libtest = true;
                }
                if v.get("nextest").is_some()
                    || v.get("type").and_then(|s| s.as_str()) == Some("nextest")
                {
                    nextest = true;
                }
            }
        }
        if is_lines {
            if nextest {
                return Some(Framework::Nextest);
            }
            if go_test {
                return Some(Framework::GoTest);
            }
            if libtest {
                return Some(Framework::Cargo);
            }
        }
    }

    // JSON object: Jest / Vitest / pytest-json-report.
    if let Some(trimmed) = trim_leading_whitespace(sample) {
        if trimmed.starts_with(b"{") {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(sample) {
                if v.get("testResults").is_some() || v.get("numTotalTestSuites").is_some() {
                    return Some(Framework::Jest);
                }
                if v.get("created").is_some() && v.get("summary").is_some() && v.get("tests").is_some() {
                    return Some(Framework::PytestJson);
                }
            }
        }
    }

    None
}

fn trim_leading_whitespace(raw: &[u8]) -> Option<&[u8]> {
    let pos = raw.iter().position(|b| !b.is_ascii_whitespace())?;
    Some(&raw[pos..])
}

fn first_nonblank_line_starts_with_brace(raw: &[u8]) -> bool {
    raw.split(|&b| b == b'\n')
        .find(|l| !l.iter().all(|b| b.is_ascii_whitespace()))
        .and_then(|l| l.iter().position(|b| !b.is_ascii_whitespace()).map(|i| l[i]))
        .map(|c| c == b'{')
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_junit_xml() {
        assert_eq!(detect(b"<?xml version=\"1.0\"?>\n<testsuites/>"), Some(Framework::JunitXml));
        assert_eq!(detect(b"  <testsuite name=\"x\"/>"), Some(Framework::JunitXml));
    }

    #[test]
    fn detects_jest_object() {
        let raw = br#"{"numTotalTestSuites":1,"testResults":[]}"#;
        assert_eq!(detect(raw), Some(Framework::Jest));
    }

    #[test]
    fn detects_libtest_jsonl() {
        let raw = b"{\"type\":\"suite\",\"event\":\"started\",\"test_count\":2}\n{\"type\":\"test\",\"event\":\"started\",\"name\":\"x\"}\n";
        assert_eq!(detect(raw), Some(Framework::Cargo));
    }

    #[test]
    fn detects_gotest_jsonl() {
        let raw = b"{\"Time\":\"2026-05-24T01:58:00Z\",\"Action\":\"run\",\"Package\":\"pkg\",\"Test\":\"TestX\"}\n";
        assert_eq!(detect(raw), Some(Framework::GoTest));
    }

    #[test]
    fn detects_pytest_json() {
        let raw = br#"{"created":1716514680,"summary":{"total":1,"passed":1},"tests":[]}"#;
        assert_eq!(detect(raw), Some(Framework::PytestJson));
    }

    #[test]
    fn returns_none_for_empty() {
        assert_eq!(detect(b""), None);
        assert_eq!(detect(b"random text\n"), None);
    }
}
