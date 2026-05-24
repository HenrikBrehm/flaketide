//! pytest --junit-xml parser. Delegates to the generic JUnit XML parser
//! but tags results with `Framework::PytestJunit`.

use crate::domain::{Framework, TestResult};
use crate::error::Result;
use crate::parser::TestParser;

pub struct PytestJunitParser;

impl TestParser for PytestJunitParser {
    fn framework(&self) -> Framework { Framework::PytestJunit }
    fn parse(&self, raw: &[u8]) -> Result<Vec<TestResult>> {
        crate::parser::junit_xml::parse_junit(raw, Framework::PytestJunit)
    }
}
