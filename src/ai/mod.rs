//! Root-cause classifier: calls Anthropic Claude, caches verdicts in SQLite.

use serde::{Deserialize, Serialize};

use crate::domain::{AiConfig, TestId};
use crate::error::{FlaketideError, Result};
use crate::store::Store;

pub mod anthropic;
pub mod prompt;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    TimingRace, Network, Environment, Ordering, Resource, Unknown,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::TimingRace => "timing_race",
            Category::Network => "network",
            Category::Environment => "environment",
            Category::Ordering => "ordering",
            Category::Resource => "resource",
            Category::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Verdict {
    pub category: Category,
    pub confidence: f64,
    pub summary: String,
    #[serde(default)] pub evidence: Vec<String>,
    #[serde(default)] pub suggested_fix: String,
    #[serde(default)] pub needs_more_data: bool,
    #[serde(default)] pub model: Option<String>,
    #[serde(default)] pub test_id: Option<String>,
}

impl Verdict {
    pub fn render_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("### Verdict: `{}`\n\n", self.category.as_str()));
        s.push_str(&format!("**Confidence:** {:.2}\n\n", self.confidence));
        s.push_str(&format!("**Summary:** {}\n\n", self.summary));
        if !self.evidence.is_empty() {
            s.push_str("**Evidence:**\n");
            for e in &self.evidence { s.push_str(&format!("- {e}\n")); }
            s.push('\n');
        }
        if !self.suggested_fix.is_empty() {
            s.push_str(&format!("**Suggested fix:** {}\n", self.suggested_fix));
        }
        if self.needs_more_data {
            s.push_str("\n_Note: classifier requested more data._\n");
        }
        s
    }
}

#[derive(Clone, Debug)]
pub struct ClassifyInput {
    pub test_id: TestId,
    pub framework: String,
    pub runs: u32,
    pub failures: u32,
    pub flake_prob: f64,
    pub recent_messages: Vec<String>,
    pub log_excerpt: String,
}

impl ClassifyInput {
    pub async fn from_store(store: &Store, id: &TestId, max_log_chars: usize) -> Result<Self> {
        let flakes = store.list_flakes().await?;
        let v = flakes.iter().find(|f| f.id.as_str() == id.as_str())
            .ok_or_else(|| FlaketideError::NotFound(format!("no flake verdict for {id}")))?;
        let recent = store.recent_failure_messages(id, 5).await?;
        let log = store.latest_failure_log(id).await?.unwrap_or_default();
        let log_excerpt = crate::util::json::truncate_chars(&log, max_log_chars);
        Ok(Self {
            test_id: id.clone(),
            framework: "unknown".to_string(),
            runs: v.runs, failures: v.failures, flake_prob: v.flake_prob,
            recent_messages: recent, log_excerpt,
        })
    }
}

pub struct Classifier {
    cfg: AiConfig,
    client: anthropic::AnthropicClient,
}

const ANTHROPIC_CANONICAL_URL: &str = "https://api.anthropic.com";

impl Classifier {
    pub fn from_env(cfg: AiConfig) -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| FlaketideError::AiDisabled)?;
        let base = cfg
            .base_url
            .clone()
            .unwrap_or_else(|| ANTHROPIC_CANONICAL_URL.to_string());
        // SSRF guard: refuse to send ANTHROPIC_API_KEY to an arbitrary host.
        crate::util::net::validate_base_url(&base, "ai", ANTHROPIC_CANONICAL_URL)?;
        let client = anthropic::AnthropicClient::new(&key, &base)?;
        Ok(Self { cfg, client })
    }

    pub async fn classify(&self, store: &Store, input: &ClassifyInput, use_cache: bool) -> Result<Verdict> {
        let (prompt_text, prompt_hash) = prompt::build(input);
        if use_cache {
            if let Some(cached) = store.ai_cached(&input.test_id, &prompt_hash).await? {
                if let Ok(mut v) = serde_json::from_str::<Verdict>(&cached) {
                    if v.model.is_none() { v.model = Some(self.cfg.model.clone()); }
                    if v.test_id.is_none() { v.test_id = Some(input.test_id.as_str().to_string()); }
                    return Ok(v);
                }
            }
        }
        let response = self.client.complete(&self.cfg.model, &prompt_text).await?;
        let mut v = parse_verdict(&response)?;
        v.model = Some(self.cfg.model.clone());
        v.test_id = Some(input.test_id.as_str().to_string());
        let payload = serde_json::to_string(&v)?;
        store.ai_store(&input.test_id, &prompt_hash, &self.cfg.model, &payload).await?;
        Ok(v)
    }
}

fn parse_verdict(raw: &str) -> Result<Verdict> {
    if let Ok(v) = serde_json::from_str::<Verdict>(raw) { return Ok(v); }
    let bytes = raw.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' { if depth == 0 { start = Some(i); } depth += 1; }
        else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    let candidate = &raw[s..=i];
                    if let Ok(v) = serde_json::from_str::<Verdict>(candidate) { return Ok(v); }
                }
                start = None;
            }
        }
    }
    // (L2) Truncate raw — it can include prompt content reflected by the model.
    let snippet: String = raw.chars().take(120).collect();
    Err(FlaketideError::Ai(format!(
        "classifier response was not valid JSON: {snippet}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_clean_json() {
        let body = r#"{"category":"timing_race","confidence":0.9,"summary":"x","evidence":[],"suggested_fix":"y","needs_more_data":false}"#;
        let v = parse_verdict(body).unwrap();
        assert_eq!(v.category, Category::TimingRace);
    }
    #[test]
    fn rejects_garbage() {
        assert!(parse_verdict("no json here").is_err());
    }
}
