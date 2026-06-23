//! Shared helpers for the integration test binaries.
//!
//! Lives in `tests/common/mod.rs` (not `tests/common.rs`) so Cargo treats it as a
//! module to include via `mod common;` rather than its own test binary. Each test
//! binary that includes it compiles its own copy, so `allow(dead_code)` keeps the
//! binaries that use only one helper warning-free.
#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

/// True if `bin --version` runs successfully — used to skip tests when a required
/// external tool (jj, zsh) isn't installed.
pub fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `jj <args>` in `cwd`, asserting it succeeds. Panics on failure.
pub fn jj(cwd: &Path, args: &[&str]) {
    let status = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "jj {args:?} failed");
}
