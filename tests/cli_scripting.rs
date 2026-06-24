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
        .args(["switch", "-c", "feat", "--print-path"])
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
        .args(["switch", "-c", "feat2"])
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
fn list_shows_all_workspaces_and_marks_current() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (base, repo) = setup_jj_repo();
    let feature = base.path().join("repo.feature");
    jj(
        &repo,
        &["workspace", "add", "--name", "feature", feature.to_str().unwrap()],
    );

    // Run from the default workspace: it should be the one marked current.
    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .arg("list")
        .current_dir(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "expected exit 0; stderr: {stderr}");
    assert!(stdout.contains("feature"), "should list the added workspace; got: {stdout}");
    assert!(
        stdout.lines().any(|l| l.starts_with("* default")),
        "default should be marked current with '*'; got: {stdout}"
    );
}

#[test]
fn switch_caret_returns_to_repo_root() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (base, repo) = setup_jj_repo();

    // Add a second workspace and run `jw switch ^` from *inside* it.
    let feature = base.path().join("repo.feature");
    jj(
        &repo,
        &["workspace", "add", "--name", "feature", feature.to_str().unwrap()],
    );

    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["switch", "^", "--print-path"])
        .current_dir(&feature)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "expected exit 0; stderr: {stderr}; stdout: {stdout}"
    );
    // `^` resolves to the repo root (the default workspace), not the current one.
    let repo_canon = std::fs::canonicalize(&repo).unwrap();
    assert_eq!(
        stdout.trim(),
        repo_canon.to_str().unwrap(),
        "`switch ^` should print the repo root"
    );
}

#[test]
fn switch_unknown_without_create_exits_nonzero() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let (_base, repo) = setup_jj_repo();

    // No `-c`: switching to a workspace that does not exist must fail (like
    // `git switch` on an unknown branch), and hint at `-c`.
    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["switch", "ghost"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected non-zero exit; stderr: {stderr}"
    );
    assert!(
        stderr.contains("no workspace named 'ghost'") && stderr.contains("-c"),
        "stderr should explain the workspace is missing and hint -c; got: {stderr}"
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
