//! Launch→first-paint perf harness (design success-criterion #12: <100ms).
//!
//! Marked `#[ignore]` so it never runs in CI (jj subprocess timing is noisy on
//! shared runners and would flake a hard assertion). Run locally with:
//!
//!     cargo test --test perf --release -- --ignored --nocapture
//!
//! `--release` matters: the shipped binary is optimized, so a debug-build number
//! would slander our own latency. `--nocapture` surfaces the per-stage breakdown.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn jj(cwd: &Path, args: &[&str]) {
    let status = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "jj {args:?} failed");
}

/// Median of a set of samples — robust to the occasional scheduler hiccup that a
/// mean would let poison the result.
fn median(mut xs: Vec<Duration>) -> Duration {
    xs.sort_unstable();
    xs[xs.len() / 2]
}

#[test]
#[ignore = "perf harness: spawns jj, run explicitly with --ignored --release"]
fn first_paint_under_100ms() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }

    // --- Fixture: a repo with a handful (5) of workspaces. ---
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    jj(&repo, &["git", "init"]);
    jj(&repo, &["config", "set", "--repo", "user.name", "Test"]);
    jj(
        &repo,
        &["config", "set", "--repo", "user.email", "test@example.com"],
    );
    for name in ["feat", "docs", "bugfix", "review"] {
        let dir = base.path().join(format!("repo.{name}"));
        jj(
            &repo,
            &["workspace", "add", "--name", name, dir.to_str().unwrap()],
        );
    }
    std::env::set_current_dir(&repo).unwrap();

    // Warm up: prime OS page cache + jj's own startup so we measure steady state,
    // not the first-ever cold spawn.
    let _ = jw::jj::list_workspaces().expect("warmup list_workspaces");

    // --- Measure the launch→first-paint critical path, 9 samples. ---
    // The path is: list_workspaces() + workspace_root() + config::load().
    // The first terminal.draw() is in-memory (sub-microsecond vs jj subprocess
    // spawns) and needs a real tty, so it is excluded by design.
    let mut list_samples = Vec::new();
    let mut path_samples = Vec::new();
    for _ in 0..9 {
        let t0 = Instant::now();
        let ws = jw::jj::list_workspaces().expect("list_workspaces");
        let list = t0.elapsed();

        let t1 = Instant::now();
        let _root = jw::jj::workspace_root().expect("workspace_root");
        let _cfg = jw::config::load();
        let rest = t1.elapsed();

        assert_eq!(ws.len(), 5, "expected 5 workspaces in the fixture");
        list_samples.push(list);
        path_samples.push(list + rest);
    }

    let list_med = median(list_samples);
    let total_med = median(path_samples);
    eprintln!("── jw first-paint perf (5 workspaces, median of 9) ──");
    eprintln!("  list_workspaces():        {list_med:?}");
    eprintln!("  + workspace_root + config: {total_med:?}  (total critical path)");
    eprintln!("  budget:                   100ms");

    assert!(
        total_med < Duration::from_millis(100),
        "first-paint critical path {total_med:?} exceeds 100ms budget"
    );
}
