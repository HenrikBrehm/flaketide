//! Network-safety helpers.
//!
//! `validate_base_url` is the gate for any HTTP endpoint that flaketide may
//! send credentialed requests to. The canonical Anthropic and GitHub URLs are
//! always accepted; localhost / loopback hosts (`127.0.0.1`, `::1`, `localhost`,
//! `*.local`) are accepted because they're the only useful targets for tests
//! and self-hosted proxies. Everything else is rejected unless the user
//! explicitly opts in via the `FLAKETIDE_ALLOW_CUSTOM_BASE_URL=1` env var.
//!
//! This blocks the SSRF attack vector where a malicious `flaketide.toml`
//! committed to a repo redirects `ANTHROPIC_API_KEY` / `GITHUB_TOKEN`
//! to an attacker-controlled host.

use crate::error::{FlaketideError, Result};

const ENV_OPT_IN: &str = "FLAKETIDE_ALLOW_CUSTOM_BASE_URL";

pub fn validate_base_url(url: &str, name: &str, canonical: &str) -> Result<()> {
    if url == canonical {
        return Ok(());
    }
    let parsed = url::Url::parse(url).map_err(|e| {
        FlaketideError::Config(format!("{name}.base_url is not a valid URL ({e}): {url}"))
    })?;
    let host = parsed.host_str().unwrap_or("");
    // Strip surrounding [] that some URL crates retain for IPv6 hosts (e.g. "[::1]").
    let host_lower = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let is_loopback = host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "::1"
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".localhost");
    if is_loopback {
        return Ok(());
    }
    let opt_in_set = std::env::var(ENV_OPT_IN).map(|v| v == "1").unwrap_or(false);
    if opt_in_set {
        eprintln!(
            "flaketide: warning — {name}.base_url overrides the canonical \
             endpoint to {url}. Credentials will be sent there. ({ENV_OPT_IN}=1 set, allowing.)"
        );
        return Ok(());
    }
    Err(FlaketideError::Config(format!(
        "{name}.base_url is overridden to {url}, which is not localhost and not the canonical \
         {canonical}. flaketide will not send credentials to an unknown host. Set \
         {ENV_OPT_IN}=1 to override (you accept that the request and its credentials go to \
         the configured URL)."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_canonical() {
        assert!(validate_base_url("https://api.anthropic.com", "ai", "https://api.anthropic.com").is_ok());
    }

    #[test]
    fn allows_localhost_variants() {
        assert!(validate_base_url("http://127.0.0.1:1234", "ai", "https://api.anthropic.com").is_ok());
        assert!(validate_base_url("http://localhost", "ai", "https://api.anthropic.com").is_ok());
        assert!(validate_base_url("http://[::1]:9000", "ai", "https://api.anthropic.com").is_ok());
        assert!(validate_base_url("https://flaky-mock.local", "ai", "https://api.anthropic.com").is_ok());
    }

    #[test]
    fn rejects_arbitrary_host_without_opt_in() {
        std::env::remove_var(ENV_OPT_IN);
        let r = validate_base_url("https://evil.example.com", "ai", "https://api.anthropic.com");
        assert!(r.is_err());
    }

    #[test]
    fn allows_arbitrary_host_with_opt_in() {
        std::env::set_var(ENV_OPT_IN, "1");
        let r = validate_base_url("https://evil.example.com", "ai", "https://api.anthropic.com");
        std::env::remove_var(ENV_OPT_IN);
        assert!(r.is_ok());
    }

    #[test]
    fn rejects_invalid_url() {
        assert!(validate_base_url("not a url", "ai", "https://api.anthropic.com").is_err());
    }
}
