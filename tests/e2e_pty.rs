//! End-to-end smoke test for the cd-on-exit loop, driven through a real PTY.
//!
//! This is the one test that exercises the whole chain a user actually hits:
//! launch `jw` in a terminal → fuzzy-filter → Enter → the chosen workspace path
//! lands in the cd-directive file (which the shell shim would `builtin cd` to).
//!
//! It is `#[ignore]` by default because PTY timing is environment-sensitive and we
//! don't want it flaking CI. Run it locally with:
//!     cargo test --test e2e_pty -- --ignored
//! Requires `jj` on PATH.

use std::process::Command;

mod common;
use common::{have, jj};

#[test]
#[ignore = "PTY-driven; run locally with --ignored (needs jj + a terminal)"]
fn enter_writes_selected_workspace_to_cd_directive() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }

    // Temp jj repo with a second workspace `feature` as a sibling dir.
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    jj(&repo, &["git", "init"]);
    jj(&repo, &["config", "set", "--repo", "user.name", "Test"]);
    jj(
        &repo,
        &["config", "set", "--repo", "user.email", "test@example.com"],
    );
    let feature = base.path().join("repo.feature");
    jj(
        &repo,
        &[
            "workspace",
            "add",
            "--name",
            "feature",
            feature.to_str().unwrap(),
        ],
    );

    // The path the picker should emit: jj's own resolution of `feature`.
    let expected = String::from_utf8(
        Command::new("jj")
            .args(["workspace", "root", "--name", "feature"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    // Directive file the binary writes the chosen path into.
    let cd_file = tempfile::NamedTempFile::new().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jw"));
    cmd.arg("switch") // the picker is `jw switch` (no name)
        .current_dir(&repo)
        .env("JW_DIRECTIVE_CD_FILE", cd_file.path())
        .env_remove("JW_DIRECTIVE_EXEC_FILE");

    let mut session =
        rexpect::session::spawn_command(cmd, Some(20_000)).expect("spawn jw in a pty");

    // Let the app finish startup (list workspaces, enter raw mode → echo off, first
    // render) before sending input. Sending too early means the PTY echoes the bytes
    // in cooked mode and crossterm never sees them.
    std::thread::sleep(std::time::Duration::from_millis(2500));

    // Type a filter that uniquely matches `feature`, let it narrow, then Enter.
    session.send("feature").unwrap();
    session.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    session.send_control('m').unwrap(); // carriage return = Enter
    session.flush().unwrap();

    // Picker should select, write the directive, and exit.
    session.exp_eof().expect("jw exited after Enter");

    let written = std::fs::read_to_string(cd_file.path()).unwrap();
    assert_eq!(
        written.trim(),
        expected,
        "cd-directive file should hold the `feature` workspace path"
    );
}

/// Regression: an `Alt+<key>` chord delivered as a *split* `ESC`/`<key>` pair (the
/// common case over slow ptys / SSH, and how many terminals send Meta) must NOT be
/// read as a lone `Esc` that aborts the picker. Here we drive `Alt+o` (open) split
/// across two writes and assert the run-directive gets the editor command — i.e. the
/// picker performed Open instead of "dropping to a shell".
#[test]
#[ignore = "PTY-driven; run locally with --ignored (needs jj + a terminal)"]
fn split_alt_o_opens_instead_of_aborting() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }

    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    jj(&repo, &["git", "init"]);
    jj(&repo, &["config", "set", "--repo", "user.name", "Test"]);
    jj(
        &repo,
        &["config", "set", "--repo", "user.email", "test@example.com"],
    );

    let cd_file = tempfile::NamedTempFile::new().unwrap();
    let run_file = tempfile::NamedTempFile::new().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jw"));
    cmd.arg("switch") // the picker is `jw switch` (no name)
        .current_dir(&repo)
        .env("JW_DIRECTIVE_CD_FILE", cd_file.path())
        .env("JW_DIRECTIVE_EXEC_FILE", run_file.path())
        // Make the emitted editor command a recognizable sentinel.
        .env("EDITOR", "MARKER_EDITOR");

    let mut session =
        rexpect::session::spawn_command(cmd, Some(20_000)).expect("spawn jw in a pty");

    std::thread::sleep(std::time::Duration::from_millis(2500));

    // Send ESC and the letter as two separate, flushed writes so crossterm sees a
    // bare `Esc` first — the exact sequence that used to abort the picker.
    session.send("\x1b").unwrap();
    session.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    session.send("o").unwrap();
    session.flush().unwrap();

    session.exp_eof().expect("jw exited after Alt+o (open)");

    let run = std::fs::read_to_string(run_file.path()).unwrap();
    assert!(
        run.contains("MARKER_EDITOR"),
        "split Alt+o should emit the editor run-directive (got {run:?}); \
         an empty directive means the picker aborted on a bare Esc"
    );
}
