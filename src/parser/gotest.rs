//! `go test -json` parser.
//!
//! Stream of events keyed by `Package` + `Test`:
//! `{"Time":"...","Action":"run","Package":"pkg","Test":"TestName"}`
//! `{"Action":"output","Package":"pkg","Test":"TestName","Output":"..."}`
//! `{"Action":"pass|fail|skip","Package":"pkg","Test":"TestName","Elapsed":0.01}`
//! Aggregate Test rows; ignore Package-level rollups.

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use crate::domain::{Framework, TestId, TestResult, TestStatus};
use crate::error::{FlaketideError, Result};
use crate::parser::TestParser;
use crate::util::json::{jsonl_lines, truncate_chars};

#[derive(Debug, Deserialize)]
struct GoEvent {
    #[serde(rename = "Action")]
    action: String,
    #[serde(rename = "Package")]
    package: Option<String>,
    #[serde(rename = "Test")]
    test: Option<String>,
    #[serde(rename = "Output")]
    output: Option<String>,
    #[serde(rename = "Elapsed")]
    elapsed: Option<f64>,
}

pub struct GoTestParser;

struct PartialTest {
    suite: String,
    name: String,
    status: TestStatus,
    elapsed: f64,
    output: String,
}

impl TestParser for GoTestParser {
    fn framework(&self) -> Framework { Framework::GoTest }

    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>> {
        let mut tests: HashMap<String, PartialTest> = HashMap::new();
        for line in jsonl_lines(raw) {
            let ev: GoEvent = match serde_json::from_slice(line) {
                Ok(e) => e,
                Err(_) => continue, // tolerate stray non-event lines
            };
            let Some(test) = ev.test.clone() else { continue };
            let pkg = ev.package.clone().unwrap_or_default();
            let key = format!("{pkg}::{test}");
            let entry = tests.entry(key).or_insert(PartialTest {
                suite: pkg,
                name: test,
                status: TestStatus::Passed,
                elapsed: 0.0,
                output: String::new(),
            });
            match ev.action.as_str() {
                "output" => {
                    if let Some(o) = ev.output {
                        entry.output.push_str(&o);
                    }
                }
                "pass" => {
                    entry.status = TestStatus::Passed;
                    if let Some(e) = ev.elapsed { entry.elapsed = e; }
                }
                "fail" => {
                    entry.status = TestStatus::Failed;
                    if let Some(e) = ev.elapsed { entry.elapsed = e; }
                }
                "skip" => {
                    entry.status = TestStatus::Skipped;
                    if let Some(e) = ev.elapsed { entry.elapsed = e; }
                }
                _ => {}
            }
        }
        let mut out = Vec::with_capacity(tests.len());
        for (_, t) in tests {
            let id = TestId::new(&t.suite, &t.name)?;
            let message = first_error_line(&t.output);
            let log = if t.output.is_empty() { None } else { Some(truncate_chars(&t.output, 4096)) };
            out.push(TestResult {
                id,
                suite: t.suite,
                name: t.name,
                status: t.status,
                duration: Duration::from_secs_f64(t.elapsed.max(0.0)),
                message,
                framework: Framework::GoTest,
                log_excerpt: log,
            });
        }
        // Stable ordering for deterministic tests / snapshots.
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(out)
    }
}

fn first_error_line(output: &str) -> Option<String> {
    for raw in output.lines() {
        let l = raw.trim();
        if l.starts_with("--- FAIL")
            || l.contains(".go:") && l.contains("Error")
            || l.contains("panic:")
            || l.contains("FAIL\t")
        {
            return Some(l.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_go_json() {
        let raw = b"{\"Action\":\"run\",\"Package\":\"pkg\",\"Test\":\"TestA\"}\n\
                    {\"Action\":\"output\",\"Package\":\"pkg\",\"Test\":\"TestA\",\"Output\":\"=== RUN TestA\\n\"}\n\
                    {\"Action\":\"pass\",\"Package\":\"pkg\",\"Test\":\"TestA\",\"Elapsed\":0.01}\n\
                    {\"Action\":\"run\",\"Package\":\"pkg\",\"Test\":\"TestB\"}\n\
                    {\"Action\":\"output\",\"Package\":\"pkg\",\"Test\":\"TestB\",\"Output\":\"--- FAIL: TestB (0.02s)\\n\"}\n\
                    {\"Action\":\"fail\",\"Package\":\"pkg\",\"Test\":\"TestB\",\"Elapsed\":0.02}\n";
        let out = GoTestParser.parse(raw).unwrap();
        assert_eq!(out.len(), 2);
        let a = out.iter().find(|t| t.name == "TestA").unwrap();
        let b = out.iter().find(|t| t.name == "TestB").unwrap();
        assert_eq!(a.status, TestStatus::Passed);
        assert_eq!(b.status, TestStatus::Failed);
        assert!(b.message.as_deref().unwrap().contains("FAIL"));
    }
}
