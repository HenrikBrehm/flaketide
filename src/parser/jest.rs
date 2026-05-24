//! Jest / Vitest JSON-reporter parser.
//!
//! Schema (Jest --json):
//! ```text
//! { "testResults": [
//!     { "name": "/path/to/file.test.js",
//!       "assertionResults": [
//!         { "ancestorTitles": ["Suite"], "title": "should X", "status": "passed"|"failed"|"skipped",
//!           "duration": 12, "failureMessages": ["..."] }, ... ] }, ... ] }
//! ```
//! Vitest emits a compatible shape under `--reporter=json`.

use std::time::Duration;

use serde::Deserialize;

use crate::domain::{Framework, TestResult, TestStatus, TestId};
use crate::error::{FlaketideError, Result};
use crate::parser::TestParser;
use crate::util::json::truncate_chars;

#[derive(Debug, Deserialize)]
struct JestRoot {
    #[serde(rename = "testResults", default)]
    test_results: Vec<JestFile>,
}

#[derive(Debug, Deserialize)]
struct JestFile {
    #[serde(default)]
    name: String,
    #[serde(rename = "assertionResults", default)]
    assertion_results: Vec<JestAssertion>,
}

#[derive(Debug, Deserialize)]
struct JestAssertion {
    #[serde(rename = "ancestorTitles", default)]
    ancestor_titles: Vec<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(rename = "failureMessages", default)]
    failure_messages: Vec<String>,
}

pub struct JestParser {
    fw: Framework,
}

impl JestParser {
    pub fn new(fw: Framework) -> Self { Self { fw } }
}

impl TestParser for JestParser {
    fn framework(&self) -> Framework { self.fw }

    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>> {
        let root: JestRoot = serde_json::from_slice(raw)
            .map_err(|e| FlaketideError::Parse(format!("jest json: {e}")))?;
        let mut out = Vec::new();
        for file in root.test_results {
            for a in file.assertion_results {
                let suite_parts = if a.ancestor_titles.is_empty() {
                    vec![file.name.clone()]
                } else {
                    a.ancestor_titles.clone()
                };
                let suite = suite_parts.join(" > ");
                let id = TestId::new(&suite, &a.title)?;
                let status = match a.status.as_str() {
                    "passed" => TestStatus::Passed,
                    "failed" => TestStatus::Failed,
                    "skipped" | "pending" | "todo" => TestStatus::Skipped,
                    _ => TestStatus::Errored,
                };
                let message = if a.failure_messages.is_empty() {
                    None
                } else {
                    Some(truncate_chars(&a.failure_messages.join("\n"), 4096))
                };
                let log_excerpt = message.clone();
                out.push(TestResult {
                    id,
                    suite,
                    name: a.title,
                    status,
                    duration: Duration::from_millis(a.duration.unwrap_or(0.0).max(0.0) as u64),
                    message,
                    framework: self.fw,
                    log_excerpt,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_jest() {
        let raw = br#"{
            "testResults":[{
                "name":"foo.test.js",
                "assertionResults":[
                  {"ancestorTitles":["S"],"title":"t1","status":"passed","duration":2},
                  {"ancestorTitles":["S"],"title":"t2","status":"failed","failureMessages":["boom"]}
                ]
            }]
        }"#;
        let p = JestParser::new(Framework::Jest);
        let r = p.parse(raw).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].status, TestStatus::Passed);
        assert_eq!(r[1].status, TestStatus::Failed);
        assert_eq!(r[1].message.as_deref(), Some("boom"));
    }
}
