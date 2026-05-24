//! End-to-end CLI smoke tests.
//! Invoked via `cargo test --test cli_smoke`.

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn version_prints() {
    Command::cargo_bin("flaketide")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("flaketide"));
}

#[test]
fn help_lists_subcommands() {
    Command::cargo_bin("flaketide")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("init"))
        .stdout(contains("run"))
        .stdout(contains("analyze"))
        .stdout(contains("tui"))
        .stdout(contains("quarantine"))
        .stdout(contains("ci"))
        .stdout(contains("report"))
        .stdout(contains("history"))
        .stdout(contains("stats"));
}

#[test]
fn init_creates_config_in_tempdir() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("flaketide")
        .unwrap()
        .args(["init", "--framework", "cargo", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();
    assert!(dir.path().join("flaketide.toml").exists());
    assert!(dir.path().join(".flaketide").exists());
}

#[test]
fn init_refuses_overwrite_without_force() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("flaketide.toml"), "runs = 5\n").unwrap();
    Command::cargo_bin("flaketide")
        .unwrap()
        .args(["init", "--framework", "cargo"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn completions_generate_bash() {
    Command::cargo_bin("flaketide")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("_flaketide"));
}

#[test]
fn quarantine_emit_cargo() {
    Command::cargo_bin("flaketide")
        .unwrap()
        .args(["quarantine", "emit", "my_suite::my_test", "--framework", "cargo"])
        .assert()
        .success()
        .stdout(contains("ignore"))
        .stdout(contains("flaketide-quarantined"));
}

#[test]
fn quarantine_emit_jest() {
    Command::cargo_bin("flaketide")
        .unwrap()
        .args(["quarantine", "emit", "Suite > test", "--framework", "jest"])
        .assert()
        .success()
        .stdout(contains("test.skip"));
}
