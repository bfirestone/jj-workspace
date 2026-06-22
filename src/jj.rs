use std::path::{Path, PathBuf};

/// A jj workspace as surfaced by the picker. One row in the list / one preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    pub path: PathBuf,
    pub change_id: String,
    pub description: String,
    pub conflict: bool,
    pub empty: bool,
    pub stale: bool,
    pub is_current: bool,
}

/// List all workspaces with summaries. (Implemented in the jj.rs task.)
pub fn list_workspaces() -> anyhow::Result<Vec<Workspace>> {
    todo!()
}

/// Lazy preview body for `name`: `jj diff -r '<name>@' --stat --color always`.
pub fn diff_stat(_name: &str) -> anyhow::Result<String> {
    todo!()
}

/// `jj workspace add --name <name> <path>`.
pub fn add_workspace(_name: &str, _path: &Path) -> anyhow::Result<()> {
    todo!()
}

/// `jj workspace forget <name>`.
pub fn forget_workspace(_name: &str) -> anyhow::Result<()> {
    todo!()
}

/// Current repo root — base for the new-workspace path template.
pub fn workspace_root() -> anyhow::Result<PathBuf> {
    todo!()
}

#[cfg(test)]
mod contract {
    use super::*;
    // --- Contract assertions (do NOT modify without updating the approved plan) ---
    const _: fn() -> anyhow::Result<Vec<Workspace>> = list_workspaces;
    const _: fn(&str) -> anyhow::Result<String> = diff_stat;
    const _: fn(&str, &Path) -> anyhow::Result<()> = add_workspace;
    const _: fn(&str) -> anyhow::Result<()> = forget_workspace;
    const _: fn() -> anyhow::Result<PathBuf> = workspace_root;

    #[test]
    fn workspace_fields_exist() {
        let w = Workspace {
            name: "default".into(),
            path: PathBuf::from("/tmp"),
            change_id: "abcd1234".into(),
            description: "(no description)".into(),
            conflict: false,
            empty: true,
            stale: false,
            is_current: true,
        };
        assert_eq!(w.name, "default");
    }
}
