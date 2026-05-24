//! SQLite-backed history store. Async wrapper over `rusqlite` using `spawn_blocking`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::domain::{
    FlakeReport, Framework, QuarantineEntry, TestId, TestResult, TestRun, TestStatus,
};
use crate::error::{FlaketideError, Result};

pub mod schema;

/// Async handle to the SQLite database.
#[derive(Clone)]
pub struct Store {
    inner: Arc<Mutex<Connection>>,
}

impl Store {
    /// Open `path`, applying migrations. Creates parent directories if missing.
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let path = path.to_path_buf();
        let path_for_perm = path.clone();
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection> {
            let mut conn = Connection::open(&path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            schema::migrations().to_latest(&mut conn)?;
            Ok(conn)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))??;
        // (M5) Tighten file mode on Unix — the history may contain test log
        // excerpts including secrets. No-op on Windows where ACLs already
        // restrict to the creating user by default.
        Self::restrict_db_permissions(&path_for_perm);
        Ok(Self { inner: Arc::new(Mutex::new(conn)) })
    }

    #[cfg(unix)]
    fn restrict_db_permissions(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(path = %path.display(), error = %e, "could not chmod 0600 on history db");
        }
    }

    #[cfg(not(unix))]
    fn restrict_db_permissions(_path: &Path) {
        // Windows: NTFS ACLs inherit from parent; the .flaketide directory
        // created with create_dir_all already restricts to the user.
    }

    /// Open an in-memory database (tests only).
    pub async fn open_in_memory() -> Result<Self> {
        let conn = tokio::task::spawn_blocking(|| -> Result<Connection> {
            let mut conn = Connection::open_in_memory()?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            schema::migrations().to_latest(&mut conn)?;
            Ok(conn)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))??;
        Ok(Self { inner: Arc::new(Mutex::new(conn)) })
    }

    /// Insert a TestRun and all its results, returning the run rowid.
    pub async fn record_run(&self, run: &TestRun) -> Result<i64> {
        let run = run.clone();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<i64> {
            let mut conn = inner.blocking_lock();
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO runs (started_at, finished_at, command, exit_code, framework, git_sha)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    run.started_at.to_rfc3339(),
                    run.finished_at.to_rfc3339(),
                    run.command,
                    run.exit_code,
                    run.framework.as_str(),
                    run.git_sha,
                ],
            )?;
            let id = tx.last_insert_rowid();
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO results (run_id, test_id, suite, name, status, duration_ms, message, log_excerpt)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;
                for r in &run.results {
                    stmt.execute(rusqlite::params![
                        id,
                        r.id.as_str(),
                        r.suite,
                        r.name,
                        r.status.as_str(),
                        r.duration.as_millis() as i64,
                        r.message,
                        r.log_excerpt,
                    ])?;
                }
            }
            tx.commit()?;
            Ok(id)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn upsert_flake_verdicts(&self, verdicts: &[FlakeReport]) -> Result<()> {
        let verdicts = verdicts.to_vec();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = inner.blocking_lock();
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM flake_verdicts", [])?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO flake_verdicts
                       (test_id, runs, failures, flake_prob, hdi_low, hdi_high, severity,
                        first_seen, last_seen, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                let now = chrono::Utc::now().to_rfc3339();
                for v in &verdicts {
                    stmt.execute(rusqlite::params![
                        v.id.as_str(),
                        v.runs as i64,
                        v.failures as i64,
                        v.flake_prob,
                        v.hdi_low,
                        v.hdi_high,
                        v.severity,
                        v.first_seen.to_rfc3339(),
                        v.last_seen.to_rfc3339(),
                        now,
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    /// Returns aggregate observation data per test across the entire history.
    pub async fn observations(&self) -> Result<Vec<TestObservation>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<TestObservation>> {
            let conn = inner.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT
                   r.test_id,
                   COUNT(*) AS runs,
                   SUM(CASE WHEN r.status IN ('failed','errored','timeout') THEN 1 ELSE 0 END) AS failures,
                   MIN(runs.started_at) AS first_seen,
                   MAX(runs.started_at) AS last_seen
                 FROM results r JOIN runs ON r.run_id = runs.id
                 GROUP BY r.test_id",
            )?;
            let mut out = Vec::new();
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let runs: i64 = row.get(1)?;
                let failures: i64 = row.get(2)?;
                let first: String = row.get(3)?;
                let last: String = row.get(4)?;
                Ok((id, runs, failures, first, last))
            })?;
            for r in rows {
                let (id, runs, failures, first, last) = r?;
                out.push(TestObservation {
                    id: TestId::from_raw(id)?,
                    runs: runs as u32,
                    failures: failures as u32,
                    first_seen: parse_rfc3339(&first)?,
                    last_seen: parse_rfc3339(&last)?,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    /// Latest non-empty failure messages for a test, deduplicated, newest first.
    pub async fn recent_failure_messages(&self, test_id: &TestId, limit: usize) -> Result<Vec<String>> {
        let key = test_id.as_str().to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            let conn = inner.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT r.message
                 FROM results r JOIN runs ON r.run_id = runs.id
                 WHERE r.test_id = ?1 AND r.message IS NOT NULL AND r.status IN ('failed','errored','timeout')
                 ORDER BY runs.started_at DESC",
            )?;
            let rows = stmt.query_map([key], |row| row.get::<_, Option<String>>(0))?;
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for row in rows {
                if let Some(msg) = row? {
                    if seen.insert(msg.clone()) {
                        out.push(msg);
                        if out.len() >= limit { break; }
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    /// Latest captured log excerpt for the most recent failing run of `test_id`.
    pub async fn latest_failure_log(&self, test_id: &TestId) -> Result<Option<String>> {
        let key = test_id.as_str().to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<String>> {
            let conn = inner.blocking_lock();
            let row: Option<Option<String>> = conn
                .query_row(
                    "SELECT r.log_excerpt
                     FROM results r JOIN runs ON r.run_id = runs.id
                     WHERE r.test_id = ?1 AND r.status IN ('failed','errored','timeout')
                     ORDER BY runs.started_at DESC LIMIT 1",
                    [key],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok();
            Ok(row.unwrap_or(None))
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    /// Pull the highest-severity flake verdict, if any.
    pub async fn top_flake(&self) -> Result<Option<FlakeReport>> {
        let all = self.list_flakes().await?;
        Ok(all.into_iter().next())
    }

    /// All flake verdicts sorted by severity desc.
    pub async fn list_flakes(&self) -> Result<Vec<FlakeReport>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<FlakeReport>> {
            let conn = inner.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT test_id, runs, failures, flake_prob, hdi_low, hdi_high, severity, first_seen, last_seen
                 FROM flake_verdicts ORDER BY severity DESC",
            )?;
            let mut out = Vec::new();
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let runs: i64 = row.get(1)?;
                let failures: i64 = row.get(2)?;
                let flake_prob: f64 = row.get(3)?;
                let hdi_low: f64 = row.get(4)?;
                let hdi_high: f64 = row.get(5)?;
                let severity: f64 = row.get(6)?;
                let first: String = row.get(7)?;
                let last: String = row.get(8)?;
                Ok((id, runs, failures, flake_prob, hdi_low, hdi_high, severity, first, last))
            })?;
            for r in rows {
                let (id, runs, failures, prob, lo, hi, sev, first, last) = r?;
                out.push(FlakeReport::new(
                    TestId::from_raw(id)?,
                    runs as u32,
                    failures as u32,
                    prob, lo, hi, sev,
                    parse_rfc3339(&first)?,
                    parse_rfc3339(&last)?,
                    Vec::new(),
                )?);
            }
            Ok(out)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn flake_prob_for(&self, test_id: &TestId) -> Result<Option<f64>> {
        let key = test_id.as_str().to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<f64>> {
            let conn = inner.blocking_lock();
            let prob: Option<f64> = conn
                .query_row(
                    "SELECT flake_prob FROM flake_verdicts WHERE test_id = ?1",
                    [key],
                    |row| row.get(0),
                )
                .ok();
            Ok(prob)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn add_quarantine(&self, e: &QuarantineEntry) -> Result<()> {
        let e = e.clone();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = inner.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO quarantine
                   (test_id, framework, reason, created_at, flake_prob_at_quarantine, author, linked_issue_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    e.id.as_str(),
                    e.framework.as_str(),
                    e.reason,
                    e.created_at.to_rfc3339(),
                    e.flake_prob_at_quarantine,
                    e.author,
                    e.linked_issue_url,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn list_quarantine(&self) -> Result<Vec<QuarantineEntry>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<QuarantineEntry>> {
            let conn = inner.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT test_id, framework, reason, created_at, flake_prob_at_quarantine, author, linked_issue_url
                 FROM quarantine ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?;
            let mut out = Vec::new();
            for r in rows {
                let (id, fw, reason, ts, prob, author, url) = r?;
                out.push(QuarantineEntry {
                    id: TestId::from_raw(id)?,
                    framework: Framework::parse(&fw)?,
                    reason,
                    created_at: parse_rfc3339(&ts)?,
                    flake_prob_at_quarantine: prob,
                    author,
                    linked_issue_url: url,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn remove_quarantine(&self, id: &TestId) -> Result<bool> {
        let key = id.as_str().to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let conn = inner.blocking_lock();
            let n = conn.execute("DELETE FROM quarantine WHERE test_id = ?1", [key])?;
            Ok(n > 0)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn history(&self, days: u32, test_id: Option<&str>) -> Result<Vec<TestRun>> {
        let test_id = test_id.map(|s| s.to_string());
        let inner = self.inner.clone();
        let since = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        tokio::task::spawn_blocking(move || -> Result<Vec<TestRun>> {
            let conn = inner.blocking_lock();
            let mut runs: Vec<TestRun> = Vec::new();
            let mut stmt = conn.prepare(
                "SELECT id, started_at, finished_at, command, exit_code, framework, git_sha
                 FROM runs WHERE started_at >= ?1 ORDER BY started_at DESC",
            )?;
            let rows = stmt.query_map([since], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?;
            for r in rows {
                let (id, started, finished, cmd, exit, fw, sha) = r?;
                let mut result_stmt = conn.prepare(
                    "SELECT test_id, suite, name, status, duration_ms, message, log_excerpt
                     FROM results WHERE run_id = ?1",
                )?;
                let mut results = Vec::new();
                let rs = result_stmt.query_map([id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                })?;
                let fw_enum = Framework::parse(&fw)?;
                for s in rs {
                    let (tid, suite, name, status, dur, msg, log) = s?;
                    let row_test_id = TestId::from_raw(tid)?;
                    if let Some(filter) = &test_id {
                        if row_test_id.as_str() != filter {
                            continue;
                        }
                    }
                    results.push(TestResult {
                        id: row_test_id,
                        suite,
                        name,
                        status: TestStatus::parse(&status)?,
                        duration: Duration::from_millis(dur as u64),
                        message: msg,
                        framework: fw_enum,
                        log_excerpt: log,
                    });
                }
                runs.push(TestRun {
                    run_id: id,
                    started_at: parse_rfc3339(&started)?,
                    finished_at: parse_rfc3339(&finished)?,
                    command: cmd,
                    exit_code: exit as i32,
                    framework: fw_enum,
                    git_sha: sha,
                    results,
                });
            }
            Ok(runs)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    /// Pull a cached AI verdict if present.
    pub async fn ai_cached(&self, test_id: &TestId, prompt_hash: &str) -> Result<Option<String>> {
        let id = test_id.as_str().to_string();
        let hash = prompt_hash.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<String>> {
            let conn = inner.blocking_lock();
            let val: Option<String> = conn
                .query_row(
                    "SELECT verdict_json FROM ai_cache WHERE test_id = ?1 AND prompt_hash = ?2",
                    [id, hash],
                    |row| row.get(0),
                )
                .ok();
            Ok(val)
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }

    pub async fn ai_store(&self, test_id: &TestId, prompt_hash: &str, model: &str, verdict_json: &str) -> Result<()> {
        let id = test_id.as_str().to_string();
        let hash = prompt_hash.to_string();
        let model = model.to_string();
        let payload = verdict_json.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = inner.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO ai_cache (test_id, prompt_hash, verdict_json, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, hash, payload, model, chrono::Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| FlaketideError::Storage(format!("spawn_blocking: {e}")))?
    }
}

/// Aggregate per-test counts used by [`crate::stats::summarize`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestObservation {
    pub id: TestId,
    pub runs: u32,
    pub failures: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| FlaketideError::Storage(format!("invalid rfc3339 timestamp '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_run() -> TestRun {
        TestRun {
            run_id: 0,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            command: "cargo test".into(),
            exit_code: 0,
            framework: Framework::Cargo,
            git_sha: Some("abc123".into()),
            results: vec![
                TestResult {
                    id: TestId::new("suite", "t1").unwrap(),
                    suite: "suite".into(),
                    name: "t1".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(5),
                    message: None,
                    framework: Framework::Cargo,
                    log_excerpt: None,
                },
                TestResult {
                    id: TestId::new("suite", "t2").unwrap(),
                    suite: "suite".into(),
                    name: "t2".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(8),
                    message: Some("boom".into()),
                    framework: Framework::Cargo,
                    log_excerpt: Some("trace".into()),
                },
            ],
        }
    }

    #[tokio::test]
    async fn record_and_history_roundtrip() {
        let store = Store::open_in_memory().await.unwrap();
        let _id = store.record_run(&sample_run()).await.unwrap();
        let runs = store.history(1, None).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].results.len(), 2);
    }

    #[tokio::test]
    async fn observations_aggregate_correctly() {
        let store = Store::open_in_memory().await.unwrap();
        for _ in 0..5 {
            store.record_run(&sample_run()).await.unwrap();
        }
        let obs = store.observations().await.unwrap();
        let t2 = obs.iter().find(|o| o.id.as_str() == "suite::t2").unwrap();
        assert_eq!(t2.runs, 5);
        assert_eq!(t2.failures, 5);
    }

    #[tokio::test]
    async fn quarantine_lifecycle() {
        let store = Store::open_in_memory().await.unwrap();
        let id = TestId::new("s", "x").unwrap();
        let entry = QuarantineEntry {
            id: id.clone(),
            framework: Framework::Cargo,
            reason: "demo".into(),
            created_at: Utc::now(),
            flake_prob_at_quarantine: 0.3,
            author: Some("alice".into()),
            linked_issue_url: None,
        };
        store.add_quarantine(&entry).await.unwrap();
        let list = store.list_quarantine().await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(store.remove_quarantine(&id).await.unwrap());
        assert_eq!(store.list_quarantine().await.unwrap().len(), 0);
    }
}
