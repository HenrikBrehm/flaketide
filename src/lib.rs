// flaketide — Cross-framework flaky-test intelligence CLI
// Copyright (C) 2026  Henrik Brehm
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version. See the LICENSE file for the full text.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
// FITNESS FOR A PARTICULAR PURPOSE.

//! `flaketide` library — cross-framework flaky-test intelligence.
//!
//! See `README.md` for an overview. Module map:
//! - [`domain`]: pure data types (no IO/async).
//! - [`error`]: crate-wide [`FlaketideError`] / [`Result`].
//! - [`config`]: TOML config loader.
//! - [`parser`]: per-framework test-output parsers.
//! - [`runner`]: repeat-runner with subprocess tree-kill.
//! - [`stats`]: Beta-Binomial flake probability + severity.
//! - [`store`]: SQLite history.
//! - [`ai`]: Anthropic root-cause classifier.
//! - [`quarantine`]: per-framework skip emitters + debt tracking.
//! - [`report`]: Markdown/JSON/JUnit/GitHub renderers.
//! - [`tui`]: ratatui interactive explorer.
//! - [`cli`]: clap subcommand dispatch.

pub mod ai;
pub mod cli;
pub mod config;
pub mod domain;
pub mod error;
pub mod parser;
pub mod quarantine;
pub mod report;
pub mod runner;
pub mod stats;
pub mod store;
pub mod tui;
pub mod util;

pub use error::{FlaketideError, Result};
