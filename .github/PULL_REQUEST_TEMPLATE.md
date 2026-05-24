<!--
Thanks for contributing to flaketide!

By submitting this PR you agree that your contribution is licensed under the
project's GNU AGPL-3.0-or-later. (We don't require a separate CLA.)
-->

## Summary

<!-- 1-3 sentences. What does this PR change and why. -->

## Type

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change
- [ ] Documentation only
- [ ] Refactor / internal cleanup

## Checklist

- [ ] `cargo test --all-features` passes locally
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean (or new lint suppressions are justified in code)
- [ ] `cargo fmt --all --check` clean
- [ ] New public API has at least one unit test and a doc comment
- [ ] CHANGELOG.md updated under `[Unreleased]`
- [ ] If touching the SQLite schema: a new migration file added (never edit a published migration)

## Notes for the reviewer

<!-- Anything reviewer-specific: trade-offs, alternatives, open questions. -->
