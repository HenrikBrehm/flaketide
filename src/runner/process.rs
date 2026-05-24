//! Stub for future process-group / Job Object handling.
//!
//! On Windows, killing a parent process does not kill children. The proper
//! fix is to assign the child to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
//! On Unix, calling `setsid()` before exec and then killing the process group does the trick.
//!
//! For the MVP we rely on `tokio::process::Command::kill_on_drop(true)` and
//! `Child::kill()`, which handle the immediate child. If you observe leaked
//! workers on Windows, prefer single-process test commands or open an issue.

pub const TREE_KILL_AVAILABLE: bool = false;
