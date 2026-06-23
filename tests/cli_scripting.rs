use std::process::Command;

mod common;
use common::{have, jj};

/// Set up a fresh temp jj repo. Returns (base_dir, repo_path).
fn setup_jj_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    jj(&repo, &["git", "init"]);
    jj(&repo, &["config", "set", "--repo", "user.name", "Test"]);
    jj(
        &repo,
        &["config", "set", "--repo", "user.email", "test@example.com"],
    );
    (base, repo)
}

#[test]
fn switch_print_path_emits_path_to_stdout() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (base, repo) = setup_jj_repo();

    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["switch", "feat", "--print-path"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        out.status.success(),
        "expected exit 0; stderr: {stderr}; stdout: {stdout}"
    );

    // The path template is {parent}/{repo}.{name} => base/repo.feat
    let expected_path = base.path().join("repo.feat");
    assert!(
        stdout.contains(expected_path.to_str().unwrap()),
        "stdout should contain '{}';\ngot: {stdout}",
        expected_path.display()
    );
}

#[test]
fn switch_without_print_path_has_no_path_on_stdout() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (_base, repo) = setup_jj_repo();

    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["switch", "feat2"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        out.status.success(),
        "expected exit 0; stderr: {stderr}; stdout: {stdout}"
    );

    // Without --print-path, stdout should be empty (directive file handles cd)
    assert!(
        stdout.trim().is_empty(),
        "stdout should be empty without --print-path; got: {stdout}"
    );
}

#[test]
fn remove_unknown_workspace_exits_nonzero() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (_base, repo) = setup_jj_repo();

    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["remove", "nope", "--force"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "expected non-zero exit; stderr: {stderr}"
    );
    assert!(
        stderr.contains("no workspace"),
        "stderr should mention 'no workspace'; got: {stderr}"
    );
}

#[test]
fn remove_current_workspace_exits_nonzero() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (_base, repo) = setup_jj_repo();

    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["remove", "default", "--force"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "expected non-zero exit; stderr: {stderr}"
    );
    assert!(
        stderr.contains("current workspace"),
        "stderr should mention 'current workspace'; got: {stderr}"
    );
}
