# flaketide

> ⚠️ **WORK IN PROGRESS — NOT YET FINISHED** ⚠️
>
> This project is **under active construction**. The v0.1.0 source tree is in place
> and the core test pipeline (Linux/macOS/Windows) is green on CI, but several
> pieces are still being built or polished:
>
> - `cargo fmt` baseline not yet established (style cleanup pending)
> - `clippy -D warnings` not yet clean (lint cleanup pending)
> - Real-API end-to-end runs against Anthropic / GitHub not yet exercised
> - TUI screens are minimal viable (full ratatui polish + snapshot tests pending)
> - No published release on crates.io or GitHub Releases yet
> - Real-world test-fixture corpus (per framework, per version) not yet committed
> - Windows subprocess tree-kill is best-effort (Job-Object backend pending)
>
> **Do not use this in production yet.** Stars and feedback welcome; install
> instructions in the rest of this README work today on Ubuntu/macOS/Windows
> via `cargo install --git`, but the v0.1.0 tag has not been cut.

> Cross-framework flaky-test intelligence — detect, classify, and eliminate flakes from any test suite.

`flaketide` is a single-binary Rust CLI that runs your test command N times, computes a Bayesian flake probability per test, persists history in a local SQLite database, and (optionally) uses the Claude API to classify root causes. It speaks every major framework's output format — Jest, Vitest, pytest, `go test`, `cargo test`, nextest, generic JUnit XML — under one unified data model.

Why? Existing flake tools are locked to one framework. None unify cross-framework, surface a proper credible interval instead of a naive pass/fail ratio, and ship with AI root-cause classification, TUI, and CI-mode regression detection in one binary.

## Features

- **Universal parser** — auto-detects Jest, Vitest, pytest (JSON + JUnit XML), `go test -json`, `cargo test` libtest JSON, nextest libtest-json-plus, and generic JUnit XML.
- **Bayesian flake model** — Beta-Binomial posterior (uniform prior), 95 % credible interval, severity scoring with recency decay.
- **Repeat-runner** — executes your test command N times, captures stdout/stderr per run, respects per-run timeouts.
- **Local SQLite history** — every run, every result, every verdict; trend queries are cheap.
- **AI root-cause classifier** — fixed taxonomy (`timing_race | network | environment | ordering | resource | unknown`), strict JSON-schema responses, blake3-keyed cache so repeat analyses cost zero tokens.
- **Quarantine generator** — emits the correct skip annotation for every framework (`jest.skip`, `@pytest.mark.skip`, `t.Skip`, `#[ignore]`, `@Disabled`).
- **Interactive TUI** — `ratatui` explorer: list, drill-in, history timeline.
- **CI mode** — emits JUnit XML + JSON, exits non-zero on flake regression vs. baseline.
- **GitHub integration** — opens (or updates) an issue summarising current flake debt.
- **Cross-platform** — Linux, macOS, Windows. Single binary, no runtime dependencies.

## Install

### Cargo (any platform)

```bash
cargo install flaketide
```

### Prebuilt binaries

Grab the latest from Releases — Linux musl x86_64, Windows MSVC x86_64, macOS arm64, macOS x86_64.

### Docker

```bash
docker run --rm -v "$PWD:/repo" ghcr.io/flaketide/flaketide ci
```

## Quickstart

```bash
# 1. Initialize in your repo.
flaketide init

# 2. Run your test suite 10 times.
flaketide run -- cargo test --no-fail-fast

# 3. See the flaky-test table.
flaketide stats

# 4. Drill in interactively.
flaketide tui

# 5. Classify the worst offender's root cause.
export ANTHROPIC_API_KEY=sk-...
flaketide analyze

# 6. CI mode (fails on regression).
flaketide ci --junit-out target/flaketide.xml --json-out target/flaketide.json
```

## CLI reference

| Command | Purpose |
|---|---|
| `flaketide init [--framework F] [--force]` | Generate `flaketide.toml` |
| `flaketide run -- <cmd>` | Repeat the test command N times |
| `flaketide stats [test_id]` | Print flake verdicts |
| `flaketide history [--days N]` | Show recent runs |
| `flaketide analyze [test_id]` | AI root-cause analysis |
| `flaketide quarantine add/list/emit/remove` | Manage quarantine debt |
| `flaketide ci` | CI-friendly pipeline with regression gating |
| `flaketide report [--markdown / --github]` | Render or sync a summary |
| `flaketide tui` | Interactive explorer |
| `flaketide completions <shell>` | Shell completions |
| `flaketide man --out-dir DIR` | Generate manpages |

Global flags: `--config PATH`, `--json`, `-v / -vv / -vvv`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Runtime / I/O error |
| 2 | Invalid configuration or CLI args |
| 3 | AI service unavailable when explicitly required |
| 4 | CI regression detected (only from `flaketide ci`) |
| 130 | SIGINT |

## Configuration

`flaketide.toml` lives at your repo root. See `flaketide.toml.example` for the full annotated template. Precedence: CLI flag → `FLAKETIDE_*` env var → `./flaketide.toml` → defaults.

## Architecture

```
   cli ── main
    │
    ├─ runner ── parser/* ── domain (pure types)
    ├─ stats ── store (SQLite, async)
    ├─ tui   ── ratatui
    ├─ ai    ── anthropic (reqwest)
    ├─ report── (md / json / junit / github)
    └─ quarantine
```

Strict layering: `domain` is a pure-types leaf — no async, no IO. Every other module may use the runtime. The runner is the only producer of `TestRun`; the store is the only persistence layer; stats / TUI / report / AI all read from the store.

### Statistical model

For `f` failures observed in `n` runs, the posterior over the per-run failure probability is `Beta(1 + f, 1 + n - f)` (uniform `Beta(1, 1)` prior). We report posterior mean and the equal-tailed 95 % credible interval; `severity = mean * confidence * recency` where `confidence = 1 - min(1, ci_width / threshold)` and `recency = exp(-age_days / 14)`. A test is classified as flaky when `0 < failures < runs`, `mean >= flake_prob_min`, and the interval is tighter than `hdi_width_max`.

### Why these dependency choices?

- `ratatui` + `crossterm` — the de-facto Rust TUI stack, ergonomic widget composition.
- `rusqlite` (bundled) — no system SQLite dependency on Windows.
- `reqwest` (rustls-tls) — pure-Rust crypto where possible.
- `statrs` — Beta inverse-CDF is closed-form for `alpha, beta >= 1`; we add a prior of 1 so it always holds.
- `quick-xml` — fast streaming parser; serde-xml-rs is slower and stricter than the real-world XML emitted by JUnit tools.
- We deliberately do **not** depend on `octocrab` — only three REST endpoints are used, hand-rolled `reqwest` is 150 lines and avoids a 30-dep graph.

## Development

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The AI classifier tests use `mockito` — no Anthropic credits are spent in CI. The real-network test is gated:

```bash
cargo test --features live_api -- --ignored
```

CI matrix: Ubuntu, macOS, Windows x stable Rust. See `.github/workflows/ci.yml`.

## Known limitations

- **Windows tree-kill**: the MVP relies on `tokio::process::Child::kill()` + `kill_on_drop(true)`. Child processes that spawn their own workers may leak when a per-run timeout fires. Prefer single-process test commands on Windows, or open an issue if this affects you.
- **`cargo test` libtest JSON** is officially unstable upstream. Prefer `cargo nextest` (libtest-json-plus, stable) for production use; flaketide auto-detects either.
- **Fixture coverage**: parsers are tested against fixtures shipped under `fixtures/<framework>/`. If your framework version emits a different shape, please file an issue with a small reproducer.

## License

Copyright © 2026 Henrik Brehm.

This program is free software: you can redistribute it and/or modify it under the terms of the **GNU Affero General Public License v3.0 or later** (AGPL-3.0-or-later) as published by the Free Software Foundation. See [`LICENSE`](./LICENSE) for the full text.

In short: you are free to use, study, modify, and redistribute `flaketide`, **provided that** any modified version you distribute — *or expose as a network service* (Section 13) — is itself released under the AGPL-3.0 with full source code available to its users. Commercial use is permitted under the same terms.

If you want to use `flaketide` in a closed-source product, contact the author for a separate commercial license.
