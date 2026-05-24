# Security Policy

## Reporting a vulnerability

If you discover a security issue in flaketide, please **do not** open a public issue.

Email **qvp1@web.de** with:
- a description of the issue
- the affected version (commit SHA or release tag)
- reproduction steps, ideally a minimal failing example
- impact / blast radius

You will receive an acknowledgement within 7 days. We aim to ship a patched release within 30 days of confirmation for High/Critical findings.

If you do not receive a reply within 7 days, open a public issue titled "Security report acknowledgement pending — please email me" (without disclosing the technical details).

## What is in scope

- The flaketide CLI binary and library crate as published from this repository.
- Default configuration (`flaketide.toml.example`).
- The published Dockerfile and CI workflows.

## What is NOT in scope

- Vulnerabilities in third-party dependencies (please report upstream; flaketide will update on next release).
- Issues that require an attacker to already have local code-execution on the developer's machine.
- Misuse of an `ANTHROPIC_API_KEY` or `GITHUB_TOKEN` that the user explicitly placed in environment variables — flaketide does not exfiltrate these; only sends them as Authorization headers to their respective official APIs.
- Exposure of test output that the user's own test suite chose to print (e.g., a test that prints a secret to stdout — flaketide captures and persists what the test emits).

## Sensitive data flaketide handles

- `ANTHROPIC_API_KEY` (env var) → sent to `https://api.anthropic.com` as `x-api-key` header only. Never logged, never persisted.
- `GITHUB_TOKEN` (env var) → sent to `https://api.github.com` as `Authorization: Bearer ...` header only. Never logged, never persisted.
- Test output (stdout/stderr of your test commands) → persisted to `.flaketide/history.db` (local SQLite). This file is added to `.gitignore` by `flaketide init`. If your test output contains secrets, treat this file as sensitive.

## Coordinated disclosure

After a patched release is published, the original reporter is credited in the CHANGELOG and the corresponding GitHub Security Advisory, unless they request to remain anonymous.

## Supply chain

flaketide pins all direct dependencies in `Cargo.toml`. Transitive dependencies are tracked in `Cargo.lock` (committed). `cargo-deny` enforces the license allow-list (`deny.toml`) and runs `RUSTSEC` advisory checks on every CI run.
