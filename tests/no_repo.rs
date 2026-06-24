use std::process::Command;

mod common;
use common::have;

#[test]
fn no_jj_no_git_prints_clean_hint() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // The picker is `jw switch` (no name); it's what probes for a jj repo.
    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .arg("switch")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no jj repo"), "stderr was: {stderr}");
    assert!(stderr.contains("jj git init"), "stderr was: {stderr}");
    // The verbose list template must NOT leak through.
    assert!(!stderr.contains("self.name()"), "template leaked: {stderr}");
}

#[test]
fn git_repo_without_jj_suggests_colocate() {
    if !have("jj") || !have("git") {
        eprintln!("skipping: jj or git not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .arg("switch")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("jj git init --colocate"),
        "stderr was: {stderr}"
    );
}
