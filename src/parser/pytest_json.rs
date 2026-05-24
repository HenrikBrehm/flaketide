//! Parser for `pytest-json-report` plugin output.
//!
//! Schema:
//! ```text
//! { "created":..., "duration":..., "summary":{...},
//!   "tests":[ {"nodeid":"path::Class::test_name","outcome":"passed"|"failed"|"skipped"|"error",
//!              "duration":0.01, "call":{"crash":{"message":"..."}, "longrepr":"..."}} ] }
//! ```

use std::time::Duration;

use serde::Deserialize;

use crate::domain::{Framework, TestId, TestResult, TestStatus};
use crate::error::{FlaketideError, Result};
use crate::parser::TestParser;
use crate::util::json::truncate_chars;

#[derive(Debug, Deserialize)]
struct PytestRoot {
    #[serde(default)]
    tests: Vec<PytestEntry>,
}

#[derive(Debug, Deserialize)]
struct PytestEntry {
    #[serde(default)]
    nodeid: String,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    call: Option<PytestCall>,
    #[serde(default)]
    setup: Option<PytestCall>,
    #[serde(default)]
    teardown: Option<PytestCall>,
}

#[derive(Debug, Deserialize)]
struct PytestCall {
    #[serde(default)]
    longrepr: Option<String>,
    #[serde(default)]
    crash: Option<PytestCrash>,
}

#[derive(Debug, Deserialize)]
struct PytestCrash {
    #[serde(default)]
    message: Option<String>,
}

pub struct PytestJsonParser;

impl TestParser for PytestJsonParser {
    fn framework(&self) -> Framework { Framework::PytestJson }

    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>> {
        let root: PytestRoot = serde_json::from_slice(raw)
            .map_err(|e| FlaketideError::Parse(format!("pytest-json: {e}")))?;
        let mut out = Vec::new();
        for t in root.tests {
            let (suite, name) = split_nodeid(&t.nodeid);
            let id = TestId::new(&suite, &name)?;
            let status = match t.outcome.as_str() {
                "passed" => TestStatus::Passed,
                "failed" => TestStatus::Failed,
                "skipped" => TestStatus::Skipped,
                "error" => TestStatus::Errored,
                _ => TestStatus::Errored,
            };
            let mut message: Option<String> = None;
            let mut log: Option<String> = None;
            for c in [t.call.as_ref(), t.setup.as_ref(), t.teardown.as_ref()].into_iter().flatten() {
                if let Some(m) = c.crash.as_ref().and_then(|c| c.message.clone()) {
                    message.get_or_insert(m);
                }
                if let Some(lr) = c.longrepr.clone() {
                    log.get_or_insert(lr);
                }
            }
            let log_excerpt = log.map(|s| truncate_chars(&s, 4096));
            out.push(TestResult {
                id,
                suite,
                name,
                status,
                duration: Duration::from_secs_f64(t.duration.max(0.0)),
                message,
                framework: Framework::PytestJson,
                log_excerpt,
            });
        }
        Ok(out)
    }
}

fn split_nodeid(nodeid: &str) -> (String, String) {
    if let Some(pos) = nodeid.rfind("::") {
        (nodeid[..pos].to_string(), nodeid[pos + 2..].to_string())
    } else {
        (String::new(), nodeid.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let raw = br#"{
            "created":1716514680,"summary":{"total":2,"passed":1,"failed":1},
            "tests":[
                {"nodeid":"tests/test_x.py::TestA::test_pass","outcome":"passed","duration":0.01},
                {"nodeid":"tests/test_x.py::TestA::test_fail","outcome":"failed","duration":0.02,
                 "call":{"crash":{"message":"AssertionError: nope"},"longrepr":"trace..."}}
            ]
        }"#;
        let p = PytestJsonParser;
        let out = p.parse(raw).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].status, TestStatus::Passed);
        assert_eq!(out[1].status, TestStatus::Failed);
        assert_eq!(out[1].message.as_deref(), Some("AssertionError: nope"));
    }
}
