use std::path::{Path, PathBuf};
use std::process::Command;

const LIST_TMPL: &str = concat!(
    r#"self.name() ++ "\t" ++ self.target().change_id().shortest(8) ++ "\t" "#,
    r#"++ if(self.target().description(), self.target().description().first_line(), "(no description)") ++ "\t" "#,
    r#"++ if(self.target().conflict(), "1", "0") ++ "\t" "#,
    r#"++ if(self.target().empty(), "1", "0") ++ "\n""#,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRow {
    pub name: String,
    pub change_id: String,
    pub description: String,
    pub conflict: bool,
    pub empty: bool,
}

/// Pure parser over the tab-delimited LIST_TMPL output. No I/O.
pub(crate) fn parse_workspace_list(raw: &str) -> Vec<ParsedRow> {
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut f = line.split('\t');
            let name = f.next()?.to_string();
            let change_id = f.next().unwrap_or_default().to_string();
            let description = f.next().unwrap_or_default().to_string();
            let conflict = f.next() == Some("1");
            let empty = f.next() == Some("1");
            Some(ParsedRow {
                name,
                change_id,
                description,
                conflict,
                empty,
            })
        })
        .collect()
}

fn run_jj(args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("jj").args(args).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "jj {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

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

/// Current repo root — base for the new-workspace path template.
pub fn workspace_root() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(run_jj(&["workspace", "root"])?.trim()))
}

fn workspace_root_named(name: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(
        run_jj(&["workspace", "root", "--name", name])?.trim(),
    ))
}

/// Resolve every workspace's root path concurrently.
///
/// Each `jj workspace root --name <name>` is an independent subprocess and pays
/// ~9ms of pure spawn + jj-startup overhead (the per-spawn cost dwarfs the work).
/// Because the calls share no state and don't contend, fanning them across
/// threads collapses wall time from `sum(N)` down to roughly `max(1)`.
///
/// Returns one result per name, in the same order as `names` — the caller zips
/// these back against the parsed rows, so order must be preserved.
fn resolve_roots(names: &[String]) -> Vec<anyhow::Result<PathBuf>> {
    // Spawn one scoped thread per name, THEN join them all in order. Spawning the
    // whole batch before joining any is what makes the shell-outs overlap; joining
    // inside the spawn loop would serialize them straight back to `sum(N)`.
    std::thread::scope(|s| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| s.spawn(|| workspace_root_named(name)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

/// List all workspaces with summaries. (Implemented in the jj.rs task.)
pub fn list_workspaces() -> anyhow::Result<Vec<Workspace>> {
    let raw = run_jj(&[
        "workspace",
        "list",
        "--ignore-working-copy",
        "-T",
        LIST_TMPL,
    ])?;
    let current = workspace_root().ok();
    let rows = parse_workspace_list(&raw);

    // Resolve all root paths in parallel (see `resolve_roots`): this is the launch
    // hot path, and the per-name shell-outs are what made it scale linearly.
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let paths = resolve_roots(&names);

    let mut out = Vec::with_capacity(rows.len());
    for (row, path) in rows.into_iter().zip(paths) {
        let path = path?;
        let is_current = current.as_deref() == Some(path.as_path());
        out.push(Workspace {
            name: row.name,
            path,
            change_id: row.change_id,
            description: row.description,
            conflict: row.conflict,
            empty: row.empty,
            stale: false, // best-effort; stale state is surfaced in the preview (diff_stat)
            is_current,
        });
    }
    Ok(out)
}

/// Lazy preview body for `name`: `jj diff -r '<name>@' --stat --color always`.
pub fn diff_stat(name: &str) -> anyhow::Result<String> {
    let rev = format!("{name}@");
    run_jj(&[
        "diff",
        "--ignore-working-copy",
        "-r",
        &rev,
        "--stat",
        "--color",
        "always",
    ])
}

/// `jj workspace add --name <name> <path>`.
pub fn add_workspace(name: &str, path: &Path) -> anyhow::Result<()> {
    let path = path.to_string_lossy();
    run_jj(&["workspace", "add", "--name", name, &path]).map(|_| ())
}

/// `jj workspace forget <name>`.
pub fn forget_workspace(name: &str) -> anyhow::Result<()> {
    run_jj(&["workspace", "forget", name]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
default\tmpmxtzuz\tDesign doc + M0 prototype\t0\t0
auth\t3f2a9c1c\twire up oauth callback\t1\t0
docs\t9a0b1c2d\t(no description)\t0\t1
";

    #[test]
    fn parses_tab_delimited_rows() {
        let rows = parse_workspace_list(FIXTURE);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "default");
        assert_eq!(rows[1].name, "auth");
        assert_eq!(rows[1].change_id, "3f2a9c1c");
        assert_eq!(rows[1].description, "wire up oauth callback");
        assert!(rows[1].conflict);
        assert!(!rows[1].empty);
        assert!(rows[2].empty);
        assert_eq!(rows[2].description, "(no description)");
    }

    #[test]
    fn skips_blank_lines() {
        assert_eq!(parse_workspace_list("\n\n").len(), 0);
    }
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
