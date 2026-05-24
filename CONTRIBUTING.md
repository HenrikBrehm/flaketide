# Contributing to flaketide

Thanks for your interest! flaketide is licensed under the **GNU AGPL-3.0-or-later** and welcomes contributions. By submitting a pull request you agree that your contribution is licensed under the same terms (no separate CLA — the in-tree license governs).

## Development setup

1. Install Rust 1.75+ from rustup.rs.
2. On Windows: install Visual Studio Build Tools with the "C++ build tools" workload.
3. Clone and build:
   ```
   git clone https://github.com/flaketide/flaketide
   cd flaketide
   cargo build
   cargo test
   ```

## Project layout

- `src/parser/` — per-framework test-output parsers. Add new frameworks here.
- `src/stats/` — Bayesian flake-probability engine.
- `src/store/` — SQLite persistence layer (async wrapper over `rusqlite`).
- `src/runner/` — repeat-runner with subprocess management.
- `src/ai/` — Anthropic root-cause classifier.
- `src/quarantine/` — per-framework skip-annotation emitters.
- `src/report/` — Markdown / JSON / JUnit / GitHub renderers.
- `src/tui/` — `ratatui` interactive explorer.
- `src/cli/` — clap subcommand dispatch.

## Adding a new test-framework parser

1. Add a variant to `domain::Framework` and update `as_str` / `parse`.
2. Add a module under `src/parser/{name}.rs` implementing `TestParser`.
3. Register it in `parser/mod.rs::parser_for`.
4. Add detection logic in `parser/detect.rs`.
5. Add fixtures under `fixtures/{name}/` and an integration test in `tests/parser_{name}.rs`.

## Tests

- `cargo test` runs the full suite.
- `cargo test --lib parser` runs parser unit tests only.
- `cargo insta review` after TUI / report changes to review snapshot diffs.

The AI classifier tests use `mockito` — no Anthropic credits are spent in CI. The `live_api` cargo feature gates the real-network test:

```
ANTHROPIC_API_KEY=sk-... cargo test --features live_api -- --ignored
```

## Style

- `cargo fmt --all` before committing.
- `cargo clippy -- -D warnings` must pass.
- Prefer focused, single-purpose modules over giant files.
- New public API needs at least one unit test and a doc comment.

## Releasing

Tag `vX.Y.Z`; the `release.yml` workflow builds and publishes prebuilt binaries.
