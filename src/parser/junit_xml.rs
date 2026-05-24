//! Generic JUnit XML parser. Works with Maven Surefire, Gradle, pytest --junit-xml,
//! JUnit5 console launcher, and basically every JUnit-emitting tool.

use std::time::Duration;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::domain::{Framework, TestId, TestResult, TestStatus};
use crate::error::{FlaketideError, Result};
use crate::parser::TestParser;
use crate::util::json::truncate_chars;

pub struct JunitXmlParser;

impl TestParser for JunitXmlParser {
    fn framework(&self) -> Framework { Framework::JunitXml }
    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>> {
        parse_junit(raw, Framework::JunitXml)
    }
}

pub fn parse_junit(raw: &[u8], framework: Framework) -> Result<Vec<TestResult>> {
    let mut reader = Reader::from_reader(raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut suite_stack: Vec<String> = Vec::new();
    let mut current: Option<TestCaseBuilder> = None;
    let mut current_text: String = String::new();
    let mut out: Vec<TestResult> = Vec::new();

    loop {
        let ev = reader.read_event_into(&mut buf).map_err(|e| FlaketideError::Xml(e.to_string()))?;
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let is_empty = matches!(reader.read_event_into(&mut Vec::new()), Ok(Event::End(_)));
                let _ = is_empty;
                let tag = std::str::from_utf8(e.name().as_ref())
                    .map_err(|err| FlaketideError::Xml(err.to_string()))?
                    .to_string();
                match tag.as_str() {
                    "testsuite" => {
                        let name = read_attr(&e, b"name").unwrap_or_default();
                        suite_stack.push(name);
                    }
                    "testcase" => {
                        let name = read_attr(&e, b"name").unwrap_or_default();
                        let classname = read_attr(&e, b"classname").unwrap_or_default();
                        let time = read_attr(&e, b"time").and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
                        let suite_path = if !classname.is_empty() {
                            classname
                        } else {
                            suite_stack.last().cloned().unwrap_or_default()
                        };
                        current = Some(TestCaseBuilder {
                            suite: suite_path,
                            name,
                            time,
                            status: TestStatus::Passed,
                            message: None,
                            log: None,
                        });
                        current_text.clear();
                    }
                    "failure" | "error" | "skipped" => {
                        if let Some(cur) = current.as_mut() {
                            let msg = read_attr(&e, b"message").or_else(|| read_attr(&e, b"type"));
                            cur.status = match tag.as_str() {
                                "failure" => TestStatus::Failed,
                                "error" => TestStatus::Errored,
                                "skipped" => TestStatus::Skipped,
                                _ => cur.status,
                            };
                            if let Some(m) = msg {
                                cur.message.get_or_insert(m);
                            }
                        }
                    }
                    "system-out" | "system-err" => {
                        current_text.clear();
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(|e| FlaketideError::Xml(e.to_string()))?.to_string();
                current_text.push_str(&s);
            }
            Event::CData(c) => {
                current_text.push_str(&String::from_utf8_lossy(&c));
            }
            Event::End(e) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .map_err(|err| FlaketideError::Xml(err.to_string()))?;
                match tag {
                    "testsuite" => { suite_stack.pop(); }
                    "testcase" => {
                        if let Some(cur) = current.take() {
                            let id = TestId::new(&cur.suite, &cur.name)?;
                            let log_excerpt = cur.log.or(cur.message.clone()).map(|s| truncate_chars(&s, 4096));
                            out.push(TestResult {
                                id,
                                suite: cur.suite,
                                name: cur.name,
                                status: cur.status,
                                duration: Duration::from_secs_f64(cur.time.max(0.0)),
                                message: cur.message,
                                framework,
                                log_excerpt,
                            });
                        }
                        current_text.clear();
                    }
                    "failure" | "error" | "skipped" => {
                        if let Some(cur) = current.as_mut() {
                            if !current_text.is_empty() && cur.log.is_none() {
                                cur.log = Some(current_text.clone());
                            }
                        }
                        current_text.clear();
                    }
                    "system-out" | "system-err" => {
                        if let Some(cur) = current.as_mut() {
                            if cur.log.is_none() && !current_text.is_empty() {
                                cur.log = Some(current_text.clone());
                            }
                        }
                        current_text.clear();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(out)
}

struct TestCaseBuilder {
    suite: String,
    name: String,
    time: f64,
    status: TestStatus,
    message: Option<String>,
    log: Option<String>,
}

fn read_attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.as_ref() == key {
            return Some(String::from_utf8_lossy(&a.value).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_junit() {
        let raw = br#"<?xml version="1.0"?>
<testsuites>
  <testsuite name="pkg.Suite">
    <testcase classname="pkg.Suite" name="t1" time="0.01"/>
    <testcase classname="pkg.Suite" name="t2" time="0.02">
      <failure message="oops" type="AssertionError">stack trace here</failure>
    </testcase>
    <testcase classname="pkg.Suite" name="t3" time="0">
      <skipped/>
    </testcase>
  </testsuite>
</testsuites>"#;
        let out = JunitXmlParser.parse(raw).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].status, TestStatus::Passed);
        assert_eq!(out[1].status, TestStatus::Failed);
        assert_eq!(out[1].message.as_deref(), Some("oops"));
        assert_eq!(out[2].status, TestStatus::Skipped);
    }
}
