//! Minimal Anthropic Messages API client.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::error::{FlaketideError, Result};

#[derive(Clone)]
pub struct AnthropicClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    temperature: f32,
    messages: Vec<Message<'a>>,
}
#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct Response {
    content: Vec<ContentBlock>,
}
#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

impl AnthropicClient {
    pub fn new(api_key: &str, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(api_key)
            .map_err(|e| FlaketideError::Ai(format!("api key: {e}")))?);
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| FlaketideError::Ai(format!("client: {e}")))?;
        Ok(Self { client, base_url: base_url.trim_end_matches('/').to_string() })
    }

    pub async fn complete(&self, model: &str, prompt: &str) -> Result<String> {
        let url = format!("{}/v1/messages", self.base_url);
        let req = Request {
            model, max_tokens: 1024, temperature: 0.0,
            messages: vec![Message { role: "user", content: prompt }],
        };
        let resp = self.client.post(&url).json(&req).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(FlaketideError::Ai(format!("anthropic {}: {}", status, body)));
        }
        let parsed: Response = serde_json::from_str(&body)
            .map_err(|e| FlaketideError::Ai(format!("decode response: {e}; body: {body}")))?;
        let text = parsed.content.iter()
            .filter(|b| b.kind == "text")
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn talks_to_mock_server() {
        let mut server = mockito::Server::new_async().await;
        let _m = server.mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"content":[{"type":"text","text":"hello"}]}"#)
            .create_async()
            .await;
        let client = AnthropicClient::new("test-key", &server.url()).unwrap();
        let out = client.complete("claude-test", "hi").await.unwrap();
        assert_eq!(out, "hello");
    }
}
