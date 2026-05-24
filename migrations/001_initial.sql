CREATE TABLE runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at   TEXT NOT NULL,
    finished_at  TEXT NOT NULL,
    command      TEXT NOT NULL,
    exit_code    INTEGER NOT NULL,
    framework    TEXT NOT NULL,
    git_sha      TEXT
) STRICT;

CREATE TABLE results (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    test_id      TEXT NOT NULL,
    suite        TEXT NOT NULL,
    name         TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('passed','failed','skipped','errored','timeout')),
    duration_ms  INTEGER NOT NULL,
    message      TEXT,
    log_excerpt  TEXT
) STRICT;

CREATE INDEX idx_results_test_id     ON results(test_id);
CREATE INDEX idx_results_run_id      ON results(run_id);
CREATE INDEX idx_runs_started_at     ON runs(started_at DESC);

CREATE TABLE flake_verdicts (
    test_id       TEXT PRIMARY KEY,
    runs          INTEGER NOT NULL,
    failures      INTEGER NOT NULL,
    flake_prob    REAL NOT NULL,
    hdi_low       REAL NOT NULL,
    hdi_high      REAL NOT NULL,
    severity      REAL NOT NULL,
    first_seen    TEXT NOT NULL,
    last_seen     TEXT NOT NULL,
    updated_at    TEXT NOT NULL
) STRICT;

CREATE TABLE ai_cache (
    test_id       TEXT NOT NULL,
    prompt_hash   TEXT NOT NULL,
    verdict_json  TEXT NOT NULL,
    model         TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    PRIMARY KEY (test_id, prompt_hash)
) STRICT;
