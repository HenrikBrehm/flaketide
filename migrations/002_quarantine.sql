CREATE TABLE quarantine (
    test_id                     TEXT PRIMARY KEY,
    framework                   TEXT NOT NULL,
    reason                      TEXT NOT NULL,
    created_at                  TEXT NOT NULL,
    flake_prob_at_quarantine    REAL NOT NULL,
    author                      TEXT,
    linked_issue_url            TEXT
) STRICT;
