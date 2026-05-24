//! GitHub issue sync via direct REST calls. We touch exactly three endpoints:
//! - GET /search/issues   (find an existing open issue tagged "flaketide")
//! - POST /repos/{owner}/{repo}/issues
//! - PATCH /repos/{owner}/{repo}/issues/{number}
//!
//! Requires GITHUB_TOKEN in the environment.

use std::env;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

use crate::domain::{FlakeReport, GithubConfig, QuarantineEntry};
use crate::error::{FlaketideError, Result};

const DEFAULT_BASE: &str = "https://api.github.com";

pub async fn sync_issue(
    cfg: &GithubConfig,
    verdicts: &[FlakeReport],
    quarantine: &[QuarantineEntry],
) -> Result<String> {
    let token = env::var("GITHUB_TOKEN")
        .map_err(|_| FlaketideError::GitHub("GITHUB_TOKEN env var not set".into()))?;
    let repo = cfg.repo.clone()
        .or_else(detect_repo_from_git)
        .ok_or_else(|| FlaketideError::GitHub(
            "repo not configured — set `github.repo = \"owner/name\"` in flaketide.toml or pass --repo".into()
        ))?;
    let (owner, name) = repo.split_once('/').ok_or_else(|| {
        FlaketideError::GitHub(format!("expected owner/name, got {repo}"))
    })?;

    let base = cfg.base_url.as_deref().unwrap_or(DEFAULT_BASE);
    let title = cfg.issue_title.clone();
    let body = build_body(verdicts, quarantine);
    let labels = cfg.issue_labels.clone();

    let client = build_client(&token)?;
    if let Some(existing) = find_existing_issue(&client, base, owner, name, &title).await? {
        update_issue(&client, base, owner, name, existing.number, &body).await?;
        return Ok(existing.html_url);
    }

    let created = create_issue(&client, base, owner, name, &title, &body, &labels).await?;
    Ok(created.html_url)
}

fn build_client(token: &str) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| FlaketideError::GitHub(format!("bad token: {e}")))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("flaketide"));
    headers.insert("X-GitHub-Api-Version", HeaderValue::from_static("2022-11-28"));
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| FlaketideError::GitHub(format!("client: {e}")))
}

#[derive(Debug, Deserialize)]
struct IssueSummary {
    number: u64,
    html_url: String,
}

#[derive(Debug, Deserialize)]
struct SearchResp {
    items: Vec<IssueSummary>,
}

async fn find_existing_issue(
    client: &reqwest::Client, base: &str, owner: &str, name: &str, title: &str,
) -> Result<Option<IssueSummary>> {
    let q = format!("repo:{owner}/{name} is:issue is:open in:title \"{title}\"");
    let url = format!("{base}/search/issues");
    let resp = client.get(&url).query(&[("q", q)]).send().await?;
    if !resp.status().is_success() {
        return Err(FlaketideError::GitHub(format!("search: {}", resp.status())));
    }
    let body: SearchResp = resp.json().await?;
    Ok(body.items.into_iter().next())
}

async fn create_issue(
    client: &reqwest::Client, base: &str, owner: &str, name: &str,
    title: &str, body: &str, labels: &[String],
) -> Result<IssueSummary> {
    let url = format!("{base}/repos/{owner}/{name}/issues");
    let payload = serde_json::json!({ "title": title, "body": body, "labels": labels });
    let resp = client.post(&url).json(&payload).send().await?;
    if !resp.status().is_success() {
        return Err(FlaketideError::GitHub(format!("create: {}", resp.status())));
    }
    Ok(resp.json().await?)
}

async fn update_issue(
    client: &reqwest::Client, base: &str, owner: &str, name: &str, number: u64, body: &str,
) -> Result<()> {
    let url = format!("{base}/repos/{owner}/{name}/issues/{number}");
    #[derive(Serialize)] struct U<'a> { body: &'a str }
    let resp = client.patch(&url).json(&U { body }).send().await?;
    if !resp.status().is_success() {
        return Err(FlaketideError::GitHub(format!("update: {}", resp.status())));
    }
    Ok(())
}

fn build_body(verdicts: &[FlakeReport], quarantine: &[QuarantineEntry]) -> String {
    let mut s = String::new();
    s.push_str("# Flaky-test report\n\n");
    s.push_str("Automatically synced by [flaketide](https://github.com/flaketide/flaketide).\n\n");
    s.push_str(&crate::report::markdown::render(verdicts, quarantine));
    s
}

fn detect_repo_from_git() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    parse_owner_name(&url)
}

fn parse_owner_name(url: &str) -> Option<String> {
    let stripped = url
        .trim_end_matches(".git")
        .trim_end_matches('/');
    if let Some(rest) = stripped.strip_prefix("git@github.com:") {
        return Some(rest.to_string());
    }
    if let Some(rest) = stripped.strip_prefix("https://github.com/") {
        return Some(rest.to_string());
    }
    if let Some(rest) = stripped.strip_prefix("ssh://git@github.com/") {
        return Some(rest.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_urls() {
        assert_eq!(parse_owner_name("git@github.com:foo/bar.git").unwrap(), "foo/bar");
        assert_eq!(parse_owner_name("https://github.com/foo/bar").unwrap(), "foo/bar");
        assert_eq!(parse_owner_name("https://github.com/foo/bar.git/").unwrap(), "foo/bar");
        assert_eq!(parse_owner_name("ssh://git@github.com/foo/bar").unwrap(), "foo/bar");
        assert!(parse_owner_name("https://gitlab.com/foo/bar").is_none());
    }
}
