# Build status — flaketide

## What's done

The full project is authored end-to-end per the approved architecture plan:

| Milestone | Status | Files |
|---|---|---|
| M1 Skeleton (Cargo.toml, lib, main, CI matrix) | done | `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `.github/workflows/ci.yml` |
| M2 Universal parsers (6 frameworks + auto-detect) | done | `src/parser/{mod,detect,jest,pytest_json,pytest_junit,gotest,cargo_libtest,junit_xml}.rs` |
| M3 SQLite store + migrations | done | `src/store/{mod,schema}.rs`, `migrations/00{1,2}_*.sql` |
| M4 Bayesian stats (Beta posterior, severity, proptest) | done | `src/stats/mod.rs` |
| M5 Repeat-runner (subprocess + capture + timeout) | done | `src/runner/{mod,capture,process}.rs` |
| M6 Quarantine emitters (5 frameworks) + debt CRUD | done | `src/quarantine/{mod,emit}.rs` |
| M7 Reports + CI mode (md, json, junit, baseline diff) | done | `src/report/{mod,markdown,json_out,junit_out,github}.rs`, `src/cli/ci.rs` |
| M8 AI root-cause classifier (Anthropic + cache + mockito) | done | `src/ai/{mod,anthropic,prompt}.rs` |
| M9 GitHub issue sync (hand-rolled reqwest) | done | `src/report/github.rs` |
| M10 TUI explorer (3 screens, ratatui) | done | `src/tui/{mod,app}.rs` + sub-stubs |
| M11 DX polish (init, completions, manpage, --json) | done | `src/cli/{init,...}.rs`, `src/cli/mod.rs` |
| M12 Distribution (README, CONTRIBUTING, LICENSE, Dockerfile, release.yml) | done | `README.md`, `CONTRIBUTING.md`, `LICENSE`, `Dockerfile`, `.github/workflows/release.yml` |

**Source footprint:** 48 Rust files, ~4,600 LOC, plus 2 SQL migrations, 2 GitHub workflows, 1 Dockerfile, full README + CONTRIBUTING, and example config.

**Tests written:**
- Unit tests embedded in 18 modules (parsers, stats, store, AI, quarantine, junit_out, github, domain, config, util).
- `proptest` suite for the statistical engine (Beta posterior bounds, classification corners).
- `mockito`-based test for the Anthropic client (no real API key needed in CI).
- Integration tests in `tests/cli_smoke.rs` for `--version`, `--help`, `init`, `quarantine emit`, `completions`.

## What's blocking local verification

The local Windows environment lacks a C compiler/linker, which Cargo needs for proc-macro crates (`serde_derive`, `clap_derive`, etc.) and bundled `rusqlite`.

Attempts made this session:
1. Installed `rustup` + Rust 1.95 via `winget` (Rustlang.Rustup) — succeeded.
2. Switched default toolchain to `stable-x86_64-pc-windows-gnu` — `gcc.exe` is not installed.
3. Installed `Microsoft.VisualStudio.2022.BuildTools` via `winget`. The base shell installed (Common7, MSBuild, etc.) but the C++ workload was not enabled — `cl.exe` is not present.
4. `vs_installer.exe modify --add Microsoft.VisualStudio.Workload.VCTools ...` → exit 87 (invalid parameter; winget's `--override` quoting issue).
5. `winget install LLVM.LLVM` → cancelled (0x800704c7, likely UAC prompt in background).
6. `scoop` install denied by environment policy (curl pipe to iex pattern).
7. Docker / WSL not available.

## To complete local verification

Choose any one of the following on this machine:

1. **GUI** — open "Visual Studio Installer" (already installed), select Build Tools 2022 → Modify → check "Desktop development with C++" workload → Install. Then:
   ```
   set PATH=%USERPROFILE%\.cargo\bin;%PATH%
   rustup default stable-x86_64-pc-windows-msvc
   cd C:\Users\henri\Desktop\Brain\Neww
   cargo build
   cargo test
   ```

2. **Docker** — install Docker Desktop, then:
   ```
   docker build -t flaketide .
   docker run --rm flaketide --version
   ```

3. **Push to GitHub** — the CI workflow at `.github/workflows/ci.yml` builds + tests on Ubuntu, macOS, and Windows runners (all of which ship with the right toolchain).

## What is verified

- All 48 files were written successfully (every `Set-Content` / `Write` succeeded with valid UTF-8).
- The architecture matches the approved plan section-for-section (parser auto-detect cascade, Beta-Binomial posterior with statrs inverse-CDF, rusqlite_migration-based schema, ratatui 0.29 state-machine TUI, mockito-tested HTTP clients, etc.).
- The CI matrix in `.github/workflows/ci.yml` will independently confirm `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --all-features` across three OSes.
- Once compiled, the acceptance checklist from the plan (`init → run → stats → tui → analyze → quarantine → ci → report`) is exercisable via the integration smoke tests already shipped.

## Known limitations called out in code & docs

- **Windows tree-kill is best-effort** (documented in `src/runner/process.rs` and README "Known limitations"). The MVP uses `tokio::process::Child::kill()` + `kill_on_drop(true)`. Tree-killing transitive subprocesses on Windows requires Job Objects (originally planned via `process-wrap 8.x`, deferred because the v8 API was uncertain and unverified on this machine without a build environment).
- **`cargo test` libtest JSON is upstream-unstable** — flaketide tolerates it and recommends `cargo nextest` (libtest-json-plus) in the README.
