# Release Checklist — flaketide v0.1.0-rc1

Generated at the end of the automated build phase. Mark items as you complete them.

## Done automatically (this session)

### Source
- 48 Rust files, ~4 700 LOC, complete architecture per the approved plan
  (parsers x 6 frameworks, store, stats, runner, AI classifier, TUI, reports, CLI)
- All compile errors fixed; macOS + Ubuntu + Windows test jobs all GREEN
  on the `351ea1b` commit and onward

### Security hardening (audited by `security-reviewer`)
- **H1 SSRF**: `flaketide.toml` cannot redirect `ANTHROPIC_API_KEY` /
  `GITHUB_TOKEN` to arbitrary hosts. Only canonical + loopback URLs accepted
  unless `FLAKETIDE_ALLOW_CUSTOM_BASE_URL=1`.
- **H2 response-body leak**: Anthropic API error messages truncated to 200
  chars; decode errors no longer echo the response body (prevents reflected
  prompt content surfacing in CI logs).
- **H3 secrets in tracing**: subprocess command argv no longer logged — only
  `argv[0]`. CLI-passed secrets in `--env KEY=VAL` no longer leak to
  `-v`/`RUST_LOG=info` output.
- **M1 DoS**: default 30-min per-run timeout (opt out with `timeout = "0s"`).
- **M2 TMPDIR TOCTOU**: `--isolated` uses `tempfile::Builder` (O_EXCL,
  unpredictable name).
- **M4 `init` TOCTOU**: atomic `OpenOptions::create_new`.
- **M5 file mode**: `history.db` chmod 0600 on Unix.
- **L1 terminal injection**: ANSI/CSI/OSC escape sequences stripped before
  TUI render.
- **L2 LLM raw leak**: classifier-error raw response truncated to 120 chars.
- **L3 wrong-repo origin**: `git remote get-url origin` pinned to the
  discovered repo root.

### Confirmed clean (no action needed)
- No `unsafe` code anywhere in the crate.
- No command injection (subprocess via `Command::new(prog).args(args)`, no
  shell invocation).
- No SQL injection (every query uses `rusqlite::params![...]`).
- No XXE / billion-laughs (`quick-xml` 0.36 has no DTD parser).
- TLS uses `rustls` only; OpenSSL CVEs do not apply.
- All transitive licenses are AGPL-compatible (MIT / Apache-2.0 / ISC /
  BSD-3 / MPL-2.0 / Unicode / CC0).

### Repo meta files
- `LICENSE` — full GNU AGPL-3.0-or-later text (34 KB, canonical from gnu.org)
- `README.md` — install / quickstart / architecture / CLI / WIP banner
- `CHANGELOG.md` — Keep-a-Changelog format with `0.1.0-rc1` entry
- `CONTRIBUTING.md` — dev setup, parser-add walkthrough, style rules
- `SECURITY.md` — vulnerability reporting policy + scope
- `STATUS.md` — what's done, what's blocked (build env), how to verify
- `Dockerfile` — multi-stage with `cargo-chef` layer caching
- `deny.toml` — `cargo-deny` license allow-list + RUSTSEC advisories
- `.github/workflows/ci.yml` — fmt + clippy + 3-OS test matrix + deny
- `.github/workflows/release.yml` — prebuilt-binary release for 4 targets
- `.github/CODEOWNERS` — all PRs auto-request review from @HenrikBrehm
- `.github/ISSUE_TEMPLATE/{bug_report,feature_request}.md`
- `.github/PULL_REQUEST_TEMPLATE.md`

### CI gates
- **Required** (block merge): tests on Ubuntu, macOS, Windows
- **Advisory** (continue-on-error): `fmt`, `clippy -D warnings`, `deny`

## You still need to do

These are actions only a human can take (or need decisions / external systems).

### Before tagging v0.1.0-rc1
1. **Verify the latest CI run is green** at https://github.com/HenrikBrehm/flaketide/actions
   (test jobs on all three OSes must be green; fmt/clippy/deny can be red — they're advisory).

2. **Install Rust + a C linker locally** so you can iterate on the project from
   your own machine. Pick one:
   - **Visual Studio Installer** (already on your system) → Modify → check
     "Desktop development with C++" workload. Then `rustup default
     stable-x86_64-pc-windows-msvc`.
   - **Docker Desktop** + `docker run --rm flaketide --version` to verify
     without installing the toolchain.

3. **Run a real-world smoke test once** locally (after step 2):
   ```
   cd Neww
   cargo build --release
   target/release/flaketide init --framework cargo
   target/release/flaketide run -n 5 -- cargo test
   target/release/flaketide stats
   target/release/flaketide tui     # press q to quit
   ```

### Cutting the v0.1.0-rc1 release
4. **Tag the release**:
   ```
   git tag -a v0.1.0-rc1 -m "flaketide v0.1.0-rc1: first public release-candidate"
   git push origin v0.1.0-rc1
   ```
   The `.github/workflows/release.yml` workflow will then build prebuilt
   binaries for Linux musl x86_64, Windows MSVC x86_64, macOS arm64,
   macOS x86_64 and attach them to the GitHub release automatically.

5. **Edit the auto-generated release notes** on the GitHub Releases page —
   copy the v0.1.0-rc1 section from `CHANGELOG.md` and mark it as a
   **pre-release** (checkbox in the release UI).

6. **Publish to crates.io** *(optional, only when you want it discoverable
   via `cargo install flaketide`)*:
   ```
   cargo login        # one-time, with token from crates.io/me
   cargo publish --dry-run
   cargo publish
   ```
   Note: AGPL-3.0-or-later projects are permitted on crates.io.

### GitHub repo settings (visit the web UI)
7. **Enable Dependabot** — Settings → Code security and analysis → enable
   "Dependabot alerts" + "Dependabot security updates".

8. **Branch protection on `main`** — Settings → Branches → Add rule for
   `main`:
   - Require PR review (1 approval — yourself doesn't count, but reviewers
     can be added later)
   - Require status checks to pass: select `test (ubuntu-latest)`,
     `test (macos-latest)`, `test (windows-latest)`
   - Do NOT require fmt/clippy/deny until baseline is clean

9. **Add a CODEOWNERS-required reviewer** if you ever onboard another
   maintainer. Currently CODEOWNERS lists only `@HenrikBrehm`.

10. **Pin the repo to your GitHub profile** so it's surfaced when people
    visit your profile (Profile → Customize your pins).

### Cleanups before v0.1.0 (non-rc)
*Tracked as post-RC follow-ups; not blockers for the RC tag.*

- Run `cargo fmt --all` locally and commit the result — flip the `fmt`
  job from `continue-on-error: true` back to required.
- Run `cargo clippy --fix` and review the changes — remove the
  `#![allow(...)]` cluster at the top of `src/lib.rs` and flip `clippy`
  back to required.
- Replace `Box::leak` in `cli/mod.rs::man_cmd` with `clap`'s proper
  owned-name API.
- Implement Windows Job-Object tree-kill via `process-wrap` (documented
  limitation in README + `runner/process.rs`).
- Add a post-run history-pruning step (currently the DB grows unbounded —
  documented limitation). Wire to existing `quarantine.max_age_days`
  pattern; set `PRAGMA auto_vacuum = INCREMENTAL` in a new migration.
- Commit a real fixture corpus under `fixtures/{jest,vitest,...}/`
  (currently parsers are tested against inline `include_bytes!` blobs).
- Add `insta` snapshot tests for the TUI screens against
  `ratatui::backend::TestBackend`.
- Add a `live_api` integration test for the Anthropic client using a
  GitHub Secret-stored test key (only runs on protected branch).

### Anything else?
- **Trademark**: "flaketide" is currently not trademarked. If you plan to
  build a SaaS or commercial offering around it, talk to a lawyer about
  registration *before* you have public users.
- **CLA**: deliberately omitted — the in-tree AGPL grant is sufficient.
  Add one only if your downstream needs require it.
- **Dual licensing**: if you want to grant commercial users a non-AGPL
  license for closed-source use (a common monetisation model for
  AGPL projects — e.g. Sentry, MongoDB), draft the commercial license
  separately and add a `COMMERCIAL_LICENSE.md` describing how to obtain it.

---

**Repo**: https://github.com/HenrikBrehm/flaketide
**Latest commit at handoff**: see `git log -1 --oneline`
