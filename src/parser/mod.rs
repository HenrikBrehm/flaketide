//! Per-framework test-output parsers.
//!
//! Every parser converts framework-specific bytes into a vector of
//! [`crate::domain::TestResult`]. Auto-detection lives in [`detect`].

use crate::domain::{Framework, TestResult};
use crate::error::Result;

pub mod cargo_libtest;
pub mod detect;
pub mod gotest;
pub mod jest;
pub mod junit_xml;
pub mod pytest_json;
pub mod pytest_junit;

pub trait TestParser: Send + Sync {
    fn framework(&self) -> Framework;
    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>>;
}

pub fn parser_for(fw: Framework) -> Box<dyn TestParser> {
    match fw {
        Framework::Jest | Framework::Vitest => Box::new(jest::JestParser::new(fw)),
        Framework::PytestJunit => Box::new(pytest_junit::PytestJunitParser),
        Framework::PytestJson => Box::new(pytest_json::PytestJsonParser),
        Framework::GoTest => Box::new(gotest::GoTestParser),
        Framework::Cargo => Box::new(cargo_libtest::CargoLibtestParser::new(Framework::Cargo)),
        Framework::Nextest => Box::new(cargo_libtest::CargoLibtestParser::new(Framework::Nextest)),
        Framework::JunitXml => Box::new(junit_xml::JunitXmlParser),
    }
}

pub fn parse_with(fw: Framework, raw: &[u8]) -> Result<Vec<TestResult>> {
    parser_for(fw).parse(raw)
}
