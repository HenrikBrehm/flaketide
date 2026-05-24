//! `cargo test --format=json` (libtest) and nextest `libtest-json-plus` parser.
//!
//! Event lines:
//! ```text
//! {"type":"suite","event":"started","test_count":N}
//! {"type":"test","event":"started","name":"crate::path::test_name"}
//! {"type":"test","name":"crate::path::test_name","event":"ok","exec_time":0.001}
//! {"type":"test","name":"...","event":"failed","exec_time":0.001,"stdout":"..."}
//! {"type":"test","name":"...","event":"ignored"}
//! {"type":"suite","event":"ok"|"failed","passed":N,"failed":N,...}
//! ```

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use crate::domain::{Framework, TestId, TestResult, TestStatus};
use crate::error::Result;
use crate::parser::TestParser;
use crate::util::json::{jsonl_lines, truncate_chars};

#[derive(Debug, Deserialize)]
struct LibtestEvent {
    #[serde(rename = "type")]
    kind: String,
    event: Option<String>,
    name: Option<String>,
    exec_time: Option<f64>,
    stdout: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

pub struct CargoLibtestParser {
    fw: Framework,
}

impl CargoLibtestParser {
    pub fn new(fw: Framework) -> Self { Self { fw } }
}

struct Partial {
    name: String,
    status: TestStatus,
    elapsed: f64,
    stdout: Option<String>,
}

impl TestParser for CargoLibtestParser {
    fn framework(&self) -> Framework { self.fw }

    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>> {
        let mut tests: HashMap<String, Partial> = HashMap::new();
        for line in jsonl_lines(raw) {
            let ev: LibtestEvent = match serde_json::from_slice(line) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if ev.kind != "test" {
                continue;
            }
            let Some(name) = ev.name.clone() else { continue };
            let entry = tests.entry(name.clone()).or_insert(Partial {
                name,
                status: TestStatus::Passed,
                elapsed: 0.0,
                stdout: None,
            });
            match ev.event.as_deref() {
                Some("started") | None => {}
                Some("ok") => {
                    entry.status = TestStatus::Passed;
                    if let Some(t) = ev.exec_time { entry.elapsed = t; }
                }
                Some("failed") => {
                    entry.status = TestStatus::Failed;
                    if let Some(t) = ev.exec_time { entry.elapsed = t; }
                    if let Some(s) = ev.stdout.or(ev.message) { entry.stdout = Some(s); }
                }
                Some("ignored") => {
                    entry.status = TestStatus::Skipped;
                }
                Some(_) => {}
            }
        }
        let mut out = Vec::with_capacity(tests.len());
        let fw = self.fw;
        for (_, p) in tests {
            let (suite, name) = split_libtest_name(&p.name);
            let id = TestId::new(&suite, &name)?;
            let message = p.stdout.as_ref().and_then(|s| {
                s.lines().find(|l| l.contains("assertion") || l.contains("panic") || l.contains("FAILED"))
                 .map(|l| l.to_string())
            });
            let log = p.stdout.map(|s| truncate_chars(&s, 4096));
            out.push(TestResult {
                id,
                suite,
                name,
                status: p.status,
                duration: Duration::from_secs_f64(p.elapsed.max(0.0)),
                message,
                framework: fw,
                log_excerpt: log,
            });
        }
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(out)
    }
}

fn split_libtest_name(full: &str) -> (String, String) {
    match full.rsplit_once("::") {
        Some((suite, name)) => (suite.to_string(), name.to_string()),
        None => (String::new(), full.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_libtest_jsonl() {
        let raw = b"{\"type\":\"suite\",\"event\":\"started\",\"test_count\":2}\n\
                    {\"type\":\"test\",\"event\":\"started\",\"name\":\"crate::a\"}\n\
                    {\"type\":\"test\",\"name\":\"crate::a\",\"event\":\"ok\",\"exec_time\":0.001}\n\
                    {\"type\":\"test\",\"event\":\"started\",\"name\":\"crate::b\"}\n\
                    {\"type\":\"test\",\"name\":\"crate::b\",\"event\":\"failed\",\"exec_time\":0.002,\"stdout\":\"assertion failed: x\"}\n\
                    {\"type\":\"suite\",\"event\":\"failed\",\"passed\":1,\"failed\":1}\n";
        let out = CargoLibtestParser::new(Framework::Cargo).parse(raw).unwrap();
        assert_eq!(out.len(), 2);
        let a = out.iter().find(|t| t.name == "a").unwrap();
        let b = out.iter().find(|t| t.name == "b").unwrap();
        assert_eq!(a.status, TestStatus::Passed);
        assert_eq!(b.status, TestStatus::Failed);
        assert!(b.message.as_deref().unwrap().contains("assertion"));
    }

    #[test]
    fn name_split() {
        assert_eq!(split_libtest_name("a::b::c"), ("a::b".to_string(), "c".to_string()));
        assert_eq!(split_libtest_name("solo"), ("".to_string(), "solo".to_string()));
    }
}
