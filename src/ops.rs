use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::config::Config;
use crate::jj::{self, BookmarkPresence, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seed {
    Revset(String), // seed -r <revset>, no fetch (local bookmark, or `name@git`)
    Fetch(String),  // real-remote only: fetch -b <name>, then -r <name>
    Fresh,          // nothing matches → no -r
}

/// Pure: choose the seed strategy. Precedence:
/// local jj bookmark → colocated git branch → real remote → fresh.
pub fn decide_seed(name: &str, p: &BookmarkPresence) -> Seed {
    if p.local {
        Seed::Revset(name.to_string())
    } else if p.git {
        Seed::Revset(format!("{name}@git"))
    } else if p.remote.is_some() {
        Seed::Fetch(name.to_string())
    } else {
        Seed::Fresh
    }
}

/// Create a workspace named `name` at `path`, seeding its working copy from a
/// matching bookmark when one exists (local → `@git` → real remote → fresh).
pub fn create_seeded(name: &str, path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        jj::is_valid_name(name),
        "invalid workspace name {name:?}: use only letters, digits, and - _ . /",
    );
    let presence = jj::bookmark_presence(name)?;
    match decide_seed(name, &presence) {
        Seed::Revset(rev) => jj::add_workspace_at(name, path, Some(&rev)),
        Seed::Fetch(_) => {
            jj::fetch_bookmark(name)?;
            jj::add_workspace_at(name, path, Some(name))
        }
        Seed::Fresh => jj::add_workspace_at(name, path, None),
    }
}

/// Resolve an *existing* workspace named `name` to its path. Errors (with a hint
/// to use `-c`) when there is no such workspace — mirrors `git switch` refusing an
/// unknown branch unless `-c` is given.
pub fn go(name: &str) -> anyhow::Result<PathBuf> {
    jj::list_workspaces()?
        .into_iter()
        .find(|w| w.name == name)
        .map(|w| w.path)
        .with_context(|| {
            format!("no workspace named '{name}' (use `jw switch -c {name}` to create it)")
        })
}

/// Create a *new* workspace named `name`, seeded from a matching bookmark. Errors
/// when a workspace with that name already exists (drop `-c` to switch to it).
pub fn create(name: &str, config: &Config, repo_root: &Path) -> anyhow::Result<PathBuf> {
    if jj::list_workspaces()?.iter().any(|w| w.name == name) {
        anyhow::bail!("workspace '{name}' already exists (drop -c to switch to it)");
    }
    let path = crate::app::workspace_path(config, repo_root, name);
    create_seeded(name, &path)?;
    Ok(path)
}

/// Pure: may this workspace be removed? `Err(reason)` when blocked.
pub fn check_removable(ws: &Workspace, force: bool) -> Result<(), String> {
    if ws.is_current {
        return Err("refusing to remove the current workspace".to_string());
    }
    if !force && (ws.conflict || !ws.empty) {
        return Err(format!(
            "workspace '{}' has uncommitted changes or conflicts; pass --force to remove anyway",
            ws.name
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RemoveOpts {
    pub keep: bool,
    pub force: bool,
}

/// Forget workspace `name` and (unless `opts.keep`) delete its directory.
/// Enforces `check_removable`; assumes any interactive confirmation already
/// happened in the caller.
pub fn remove(name: &str, opts: RemoveOpts) -> anyhow::Result<()> {
    let ws = jj::list_workspaces()?
        .into_iter()
        .find(|w| w.name == name)
        .ok_or_else(|| anyhow::anyhow!("no workspace named '{name}'"))?;
    check_removable(&ws, opts.force).map_err(|e| anyhow::anyhow!(e))?;
    // Delete the directory first: a failure here leaves jj state intact and
    // retryable, instead of orphaning the dir after an early forget.
    if !opts.keep {
        match std::fs::remove_dir_all(&ws.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {} // already gone; fine
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("removing workspace directory {}", ws.path.display())
                });
            }
        }
    }
    jj::forget_workspace(name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jj::Workspace;
    use std::path::PathBuf;

    fn ws(name: &str, current: bool, conflict: bool, empty: bool) -> Workspace {
        Workspace {
            name: name.into(),
            path: PathBuf::from(format!("/repo.{name}")),
            change_id: "aaaa".into(),
            description: "d".into(),
            conflict,
            empty,
            stale: false,
            is_current: current,
        }
    }

    #[test]
    fn never_remove_current() {
        assert!(check_removable(&ws("x", true, false, true), false).is_err());
        assert!(check_removable(&ws("x", true, false, true), true).is_err()); // even with force
    }

    #[test]
    fn dirty_or_conflict_blocked_without_force() {
        assert!(check_removable(&ws("x", false, false, false), false).is_err()); // !empty
        assert!(check_removable(&ws("x", false, true, true), false).is_err()); // conflict
        assert!(check_removable(&ws("x", false, false, false), true).is_ok()); // force overrides
        assert!(check_removable(&ws("x", false, false, true), false).is_ok()); // clean & empty
    }

    #[test]
    fn contract_sigs() {
        const _: fn(&Workspace, bool) -> Result<(), String> = check_removable;
        const _: fn(&str, &std::path::Path) -> anyhow::Result<()> = create_seeded;
        const _: fn(&str) -> anyhow::Result<PathBuf> = go;
        const _: fn(&str, &Config, &Path) -> anyhow::Result<PathBuf> = create;
        const _: fn(&str, RemoveOpts) -> anyhow::Result<()> = remove;
    }

    const _: fn(&str, &BookmarkPresence) -> Seed = decide_seed;

    #[test]
    fn seed_precedence() {
        let local = BookmarkPresence {
            local: true,
            git: true,
            remote: Some("origin".into()),
        };
        assert_eq!(decide_seed("x", &local), Seed::Revset("x".into()));
        let git = BookmarkPresence {
            local: false,
            git: true,
            remote: Some("origin".into()),
        };
        assert_eq!(decide_seed("x", &git), Seed::Revset("x@git".into()));
        let remote = BookmarkPresence {
            local: false,
            git: false,
            remote: Some("origin".into()),
        };
        assert_eq!(decide_seed("x", &remote), Seed::Fetch("x".into()));
        assert_eq!(decide_seed("x", &BookmarkPresence::default()), Seed::Fresh);
    }
}
