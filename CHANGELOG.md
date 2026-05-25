# Changelog

All notable changes to flaketide are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`flaketide mcp`** — Model Context Protocol server over stdio.
  Exposes `list_flaky_tests`, `get_test_history`, `get_test_stats`,
  `get_quarantine_debt`, and `analyze_test` as MCP tools so agentic
  coding tools (Claude Code, Cursor, Continue, OpenCode) can query
  flaketide directly from inside an editor.
- **`flaketide bisect <test_id> -- <cmd>`** — git-bisect for flakes.
  Walks the commit range in bisection order, runs the test N times
  per candidate commit, finds the first commit where the Bayesian
  posterior crosses a configurable threshold.
- **`flaketide cost`** — dollar-impact report. Multiplies failed-test
  minutes by a configurable CI runner $/minute rate (default GitHub
  Actions Linux), prints the monthly burn and top offenders.
- **`flaketide demo`** — self-contained walkthrough on a synthetic
  flaky suite (30 runs in under 2 minutes). Useful for screenshots,
  conference demos, and smoke-testing a fresh install.
- **`flaketide tag`** — heuristic flake-pattern detection without any
  LLM. Tags tests as `scheduling` / `ordering` / `flapping` /
  `worsening` based purely on observed correlations in local history.

## [0.1.0-rc1] - 2026-05-24

### Added

- First public source release of `flaketide`.
- Universal test-output parser:
  Jest, Vitest, pytest (JSON + JUnit XML), `go test -json`,
  `cargo test` libtest JSON, `cargo-nextest` libtest-json-plus,
  generic JUnit XML.
- Auto-detection cascade: explicit `--framework` → `flaketide.toml` →
  byte-sniff of the first 8 KiB.
- Bayesian flake-probability engine:
  Beta-Binomial posterior with uniform `Beta(1, 1)` prior,
  95 % equal-tailed credible interval via `statrs::Beta::inverse_cdf`,
  severity = `mean * confidence * recency` with configurable thresholds.
- Repeat-runner:
  `tokio::process::Command` with `kill_on_drop(true)`,
  per-run timeout (`tokio::time::timeout`), opt-in isolated TMPDIR per run,
  `FLAKETIDE_RUN_ID` env var injection, 8-MiB capture cap per stream,
  optional tee to parent terminal.
- Local SQLite history store:
  `rusqlite` (bundled) wrapped in `tokio::sync::Mutex` + `spawn_blocking`,
  five tables (`runs`, `results`, `flake_verdicts`, `ai_cache`, `quarantine`),
  STRICT-mode schema with FK cascade,
  migration framework via `rusqlite_migration`.
- AI root-cause classifier:
  Anthropic Messages API, fixed taxonomy
  (`timing_race | network | environment | ordering | resource | unknown`),
  strict JSON-schema response with one-shot example,
  blake3-keyed response cache (repeat calls cost zero tokens),
  graceful degradation when `ANTHROPIC_API_KEY` is unset.
- Quarantine-annotation generator for every supported framework
  (`jest.skip`, `@pytest.mark.skip`, `t.Skip`, `#[ignore]`, `@Disabled`),
  plus debt tracking in the store.
- Reports: Markdown, JSON, JUnit XML, GitHub Issue sync (hand-rolled
  reqwest client; only the three endpoints we need).
- `ratatui` TUI explorer (List / Detail / Help screens).
- CI mode (`flaketide ci`) with optional baseline comparison and
  exit-code 4 on flake regression.
- DX polish:
  interactive `flaketide init` with repo-aware framework auto-detection,
  shell completions via `clap_complete` (bash, zsh, fish, powershell, elvish),
  manpages via `clap_mangen`,
  `--json` mode on every read command,
  documented exit codes (0, 1, 2, 3, 4, 130).
- Distribution:
  multi-stage `Dockerfile` with `cargo-chef`,
  `.github/workflows/release.yml` building prebuilt binaries for
  `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc`,
  `aarch64-apple-darwin`, `x86_64-apple-darwin`.
- Tests:
  18 modules with unit tests,
  `proptest` suite for the Beta-Binomial engine,
  `mockito`-based tests for the Anthropic and GitHub HTTP clients
  (real-API tests gated behind the `live_api` cargo feature),
  end-to-end smoke tests in `tests/cli_smoke.rs`.
- CI matrix on Ubuntu, macOS, and Windows x stable Rust.
- Repo meta files: `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`,
  `deny.toml` (license allow-list + RUSTSEC advisories),
  issue and PR templates.

### Known limitations (also called out in README and code)

- Windows subprocess **tree-kill is best-effort**; only the immediate
  child is killed on timeout. Children spawned by the test runner may
  leak. A future release will use `process-wrap` with Windows Job
  Objects to fix this.
- `cargo test`'s libtest JSON format is officially unstable upstream;
  prefer `cargo-nextest` (libtest-json-plus) for production CI.
- `cargo fmt` baseline not yet established; `clippy -D warnings`
  not yet clean. These advisory CI jobs surface diffs but do not
  block the build for the first RC.

[Unreleased]: https://github.com/HenrikBrehm/flaketide/compare/v0.1.0-rc1...HEAD
[0.1.0-rc1]: https://github.com/HenrikBrehm/flaketide/releases/tag/v0.1.0-rc1
