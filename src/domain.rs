//! Pure data types shared across every module. No async, no IO, no DB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::{FlaketideError, Result};

/// Recognized test-output framework.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    Jest,
    Vitest,
    PytestJunit,
    PytestJson,
    GoTest,
    Cargo,
    Nextest,
    JunitXml,
}

impl Framework {
    pub fn as_str(self) -> &'static str {
        match self {
            Framework::Jest => "jest",
            Framework::Vitest => "vitest",
            Framework::PytestJunit => "pytest-junit",
            Framework::PytestJson => "pytest-json",
            Framework::GoTest => "gotest",
            Framework::Cargo => "cargo",
            Framework::Nextest => "nextest",
            Framework::JunitXml => "junit-xml",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "jest" => Ok(Framework::Jest),
            "vitest" => Ok(Framework::Vitest),
            "pytest-junit" | "pytest_junit" => Ok(Framework::PytestJunit),
            "pytest-json" | "pytest_json" | "pytest" => Ok(Framework::PytestJson),
            "gotest" | "go-test" | "go" => Ok(Framework::GoTest),
            "cargo" | "libtest" => Ok(Framework::Cargo),
            "nextest" => Ok(Framework::Nextest),
            "junit-xml" | "junit" => Ok(Framework::JunitXml),
            other => Err(FlaketideError::UnknownFramework(other.to_string())),
        }
    }
}

impl fmt::Display for Framework {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Errored,
    Timeout,
}

impl TestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TestStatus::Passed => "passed",
            TestStatus::Failed => "failed",
            TestStatus::Skipped => "skipped",
            TestStatus::Errored => "errored",
            TestStatus::Timeout => "timeout",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "passed" => Ok(TestStatus::Passed),
            "failed" => Ok(TestStatus::Failed),
            "skipped" => Ok(TestStatus::Skipped),
            "errored" => Ok(TestStatus::Errored),
            "timeout" => Ok(TestStatus::Timeout),
            other => Err(FlaketideError::Invariant(format!(
                "unknown status: {other}"
            ))),
        }
    }

    pub fn is_failure(self) -> bool {
        matches!(self, TestStatus::Failed | TestStatus::Errored | TestStatus::Timeout)
    }
}

/// Stable identity across runs: "{suite}::{name}".
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TestId(pub String);

impl TestId {
    pub fn new(suite: &str, name: &str) -> Result<Self> {
        let combined = if suite.is_empty() {
            name.to_string()
        } else {
            format!("{suite}::{name}")
        };
        if combined.is_empty() {
            return Err(FlaketideError::Invariant("empty test id".into()));
        }
        if combined.contains('\n') {
            return Err(FlaketideError::Invariant("test id contains newline".into()));
        }
        Ok(Self(combined))
    }

    pub fn from_raw(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.is_empty() {
            return Err(FlaketideError::Invariant("empty test id".into()));
        }
        if s.contains('\n') {
            return Err(FlaketideError::Invariant("test id contains newline".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestResult {
    pub id: TestId,
    pub suite: String,
    pub name: String,
    pub status: TestStatus,
    #[serde(with = "humantime_serde_compat")]
    pub duration: Duration,
    pub message: Option<String>,
    pub framework: Framework,
    pub log_excerpt: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestRun {
    pub run_id: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub command: String,
    pub exit_code: i32,
    pub framework: Framework,
    pub git_sha: Option<String>,
    pub results: Vec<TestResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlakeReport {
    pub id: TestId,
    pub runs: u32,
    pub failures: u32,
    pub flake_prob: f64,
    pub hdi_low: f64,
    pub hdi_high: f64,
    pub severity: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub recent_messages: Vec<String>,
}

impl FlakeReport {
    pub fn new(
        id: TestId,
        runs: u32,
        failures: u32,
        flake_prob: f64,
        hdi_low: f64,
        hdi_high: f64,
        severity: f64,
        first_seen: DateTime<Utc>,
        last_seen: DateTime<Utc>,
        recent_messages: Vec<String>,
    ) -> Result<Self> {
        if failures > runs {
            return Err(FlaketideError::Invariant("failures > runs".into()));
        }
        if !(0.0..=1.0).contains(&flake_prob) {
            return Err(FlaketideError::Invariant("flake_prob out of [0,1]".into()));
        }
        if hdi_low > flake_prob + 1e-9 || hdi_high + 1e-9 < flake_prob {
            return Err(FlaketideError::Invariant(
                "credible interval does not bracket the mean".into(),
            ));
        }
        if hdi_low > hdi_high {
            return Err(FlaketideError::Invariant("hdi_low > hdi_high".into()));
        }
        Ok(Self {
            id,
            runs,
            failures,
            flake_prob,
            hdi_low,
            hdi_high,
            severity,
            first_seen,
            last_seen,
            recent_messages,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuarantineEntry {
    pub id: TestId,
    pub framework: Framework,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub flake_prob_at_quarantine: f64,
    pub author: Option<String>,
    pub linked_issue_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub framework: Option<Framework>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default = "Config::default_runs")]
    pub runs: u32,
    #[serde(default = "Config::default_parallel")]
    pub parallel: u8,
    #[serde(default, with = "humantime_serde_opt")]
    pub timeout: Option<Duration>,
    #[serde(default = "Config::default_db_path")]
    pub db_path: PathBuf,
    #[serde(default)]
    pub thresholds: Thresholds,
    #[serde(default)]
    pub quarantine: QuarantinePolicy,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub github: GithubConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            framework: None,
            command: None,
            runs: Self::default_runs(),
            parallel: Self::default_parallel(),
            timeout: None,
            db_path: Self::default_db_path(),
            thresholds: Thresholds::default(),
            quarantine: QuarantinePolicy::default(),
            ai: AiConfig::default(),
            github: GithubConfig::default(),
        }
    }
}

impl Config {
    pub fn default_runs() -> u32 { 10 }
    pub fn default_parallel() -> u8 { 1 }
    pub fn default_db_path() -> PathBuf { PathBuf::from(".flaketide/history.db") }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default = "Thresholds::default_flake_prob_min")]
    pub flake_prob_min: f64,
    #[serde(default = "Thresholds::default_hdi_width_max")]
    pub hdi_width_max: f64,
    #[serde(default = "Thresholds::default_severity_alert")]
    pub severity_alert: f64,
}
impl Default for Thresholds {
    fn default() -> Self {
        Self {
            flake_prob_min: Self::default_flake_prob_min(),
            hdi_width_max: Self::default_hdi_width_max(),
            severity_alert: Self::default_severity_alert(),
        }
    }
}
impl Thresholds {
    pub fn default_flake_prob_min() -> f64 { 0.05 }
    pub fn default_hdi_width_max() -> f64 { 0.30 }
    pub fn default_severity_alert() -> f64 { 0.50 }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuarantinePolicy {
    #[serde(default = "QuarantinePolicy::default_max_age_days")]
    pub max_age_days: u32,
    #[serde(default = "QuarantinePolicy::default_max_count")]
    pub max_count: u32,
}
impl Default for QuarantinePolicy {
    fn default() -> Self {
        Self {
            max_age_days: Self::default_max_age_days(),
            max_count: Self::default_max_count(),
        }
    }
}
impl QuarantinePolicy {
    pub fn default_max_age_days() -> u32 { 30 }
    pub fn default_max_count() -> u32 { 25 }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    #[serde(default = "AiConfig::default_model")]
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default = "AiConfig::default_max_log")]
    pub max_log_excerpt_chars: usize,
}
impl Default for AiConfig {
    fn default() -> Self {
        Self {
            model: Self::default_model(),
            base_url: None,
            max_log_excerpt_chars: Self::default_max_log(),
        }
    }
}
impl AiConfig {
    pub fn default_model() -> String { "claude-opus-4-7".to_string() }
    pub fn default_max_log() -> usize { 12_000 }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default = "GithubConfig::default_title")]
    pub issue_title: String,
    #[serde(default = "GithubConfig::default_labels")]
    pub issue_labels: Vec<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}
impl GithubConfig {
    pub fn default_title() -> String { "Flaky tests - flaketide report".to_string() }
    pub fn default_labels() -> Vec<String> { vec!["flaky-test".to_string(), "automated".to_string()] }
}

/// Serde helper for serializing Duration as humantime string.
mod humantime_serde_compat {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        humantime::format_duration(*d).to_string().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(d)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

mod humantime_serde_opt {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_str(&humantime::format_duration(*d).to_string()),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let opt = Option::<String>::deserialize(d)?;
        match opt {
            Some(s) => humantime::parse_duration(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framework_roundtrip() {
        for fw in [
            Framework::Jest, Framework::Vitest, Framework::PytestJunit,
            Framework::PytestJson, Framework::GoTest, Framework::Cargo,
            Framework::Nextest, Framework::JunitXml,
        ] {
            assert_eq!(fw, Framework::parse(fw.as_str()).unwrap());
        }
    }

    #[test]
    fn status_roundtrip() {
        for s in [TestStatus::Passed, TestStatus::Failed, TestStatus::Skipped,
                  TestStatus::Errored, TestStatus::Timeout] {
            assert_eq!(s, TestStatus::parse(s.as_str()).unwrap());
        }
    }

    #[test]
    fn test_id_invariants() {
        assert!(TestId::new("", "").is_err());
        assert!(TestId::new("", "foo").is_ok());
        assert!(TestId::new("suite", "foo\nbar").is_err());
        assert_eq!(TestId::new("suite", "foo").unwrap().as_str(), "suite::foo");
    }

    #[test]
    fn flake_report_invariants() {
        let id = TestId::from_raw("a::b").unwrap();
        let now = chrono::Utc::now();
        assert!(FlakeReport::new(id.clone(), 10, 11, 0.5, 0.4, 0.6, 0.5, now, now, vec![]).is_err());
        assert!(FlakeReport::new(id.clone(), 10, 3, 1.5, 0.4, 0.6, 0.5, now, now, vec![]).is_err());
        assert!(FlakeReport::new(id.clone(), 10, 3, 0.5, 0.6, 0.4, 0.5, now, now, vec![]).is_err());
        assert!(FlakeReport::new(id, 10, 3, 0.3, 0.1, 0.5, 0.4, now, now, vec![]).is_ok());
    }
}
