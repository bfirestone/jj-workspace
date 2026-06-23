use std::path::{Path, PathBuf};
use std::process::Command;

// `self.root()` (the workspace's absolute path) is rendered right after the name
// so the path — which we cd into — is positionally protected from a stray tab in
// the free-text description column. One `jj workspace list` call yields every
// row's path; no per-workspace `jj workspace root` shell-outs are needed.
const LIST_TMPL: &str = concat!(
    r#"self.name() ++ "\t" ++ self.root() ++ "\t" "#,
    r#"++ self.target().change_id().shortest(8) ++ "\t" "#,
    r#"++ if(self.target().description(), self.target().description().first_line(), "(no description)") ++ "\t" "#,
    r#"++ if(self.target().conflict(), "1", "0") ++ "\t" "#,
    r#"++ if(self.target().empty(), "1", "0") ++ "\n""#,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRow {
    pub name: String,
    pub root: String,
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
            let root = f.next().unwrap_or_default().to_string();
            let change_id = f.next().unwrap_or_default().to_string();
            let description = f.next().unwrap_or_default().to_string();
            let conflict = f.next() == Some("1");
            let empty = f.next() == Some("1");
            Some(ParsedRow {
                name,
                root,
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
        let tag = args.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
        anyhow::bail!("jj {tag}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoRepo {
    GitRepoNoJj, // no jj repo, but a git repo is present here
    Nothing,     // no jj repo and no git repo
}

/// Pure: is a failed jj invocation the "no jj repo" case, and which flavor?
pub fn classify_no_repo(stderr: &str, in_git_repo: bool) -> Option<NoRepo> {
    if !stderr.contains("no jj repo") {
        return None;
    }
    Some(if in_git_repo {
        NoRepo::GitRepoNoJj
    } else {
        NoRepo::Nothing
    })
}

impl NoRepo {
    /// Pure: the jj-style hint shown to the user (printed to stderr by main).
    pub fn message(&self) -> String {
        match self {
            NoRepo::GitRepoNoJj => "jw: no jj repo here.\n\
                This looks like a git repo — create a jj repo backed by it:\n    \
                jj git init --colocate"
                .to_string(),
            NoRepo::Nothing => "jw: no jj repo here.\n\
                Start one with:\n    jj git init"
                .to_string(),
        }
    }
}

/// Walk up from the current dir looking for a `.git` entry (dir for a normal
/// repo, file for a worktree). Dependency-free; mirrors jj's own heuristic.
pub fn in_git_repo() -> bool {
    let mut dir = std::env::current_dir().ok();
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return true;
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookmarkPresence {
    pub local: bool,            // a local jj bookmark `name`
    pub git: bool,              // `name@git` (colocated git branch, not yet a jj bookmark)
    pub remote: Option<String>, // a real remote (≠ "git") holding `name`
}

const BOOKMARK_TMPL: &str = r#"name ++ "\t" ++ if(remote, remote, "") ++ "\n""#;

/// Pure: fold `jj bookmark list --all-remotes <name> -T BOOKMARK_TMPL` output
/// into presence. Rows whose name != `name` are ignored (glob safety).
pub fn parse_bookmark_presence(raw: &str, name: &str) -> BookmarkPresence {
    let mut p = BookmarkPresence::default();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let mut f = line.split('\t');
        if f.next() != Some(name) {
            continue;
        }
        match f.next().unwrap_or_default() {
            "" => p.local = true,
            "git" => p.git = true,
            r => p.remote = Some(r.to_string()),
        }
    }
    p
}

/// One `jj bookmark list --all-remotes <name>` call → structured presence.
pub fn bookmark_presence(name: &str) -> anyhow::Result<BookmarkPresence> {
    let raw = run_jj(&[
        "bookmark",
        "list",
        "--all-remotes",
        name,
        "-T",
        BOOKMARK_TMPL,
    ])?;
    Ok(parse_bookmark_presence(&raw, name))
}

/// `jj git fetch -b <name>`.
pub fn fetch_bookmark(name: &str) -> anyhow::Result<()> {
    run_jj(&["git", "fetch", "-b", name]).map(|_| ())
}

/// `jj workspace add --name <name> [-r <seed>] <path>`.
/// `seed` is the parent revset for the new working-copy commit (`-r X` =
/// "based on bookmark X"). Verified against jj 0.42.
pub fn add_workspace_at(name: &str, path: &Path, seed: Option<&str>) -> anyhow::Result<()> {
    let path = path.to_string_lossy();
    let mut args = vec!["workspace", "add", "--name", name];
    if let Some(rev) = seed {
        args.push("-r");
        args.push(rev);
    }
    args.push(&path);
    run_jj(&args).map(|_| ())
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

/// List all workspaces with summaries from a single `jj workspace list` call —
/// each row's path comes straight from the template's `self.root()` column.
pub fn list_workspaces() -> anyhow::Result<Vec<Workspace>> {
    let raw = run_jj(&[
        "workspace",
        "list",
        "--ignore-working-copy",
        "-T",
        LIST_TMPL,
    ])?;
    // One extra call to mark which row is the current workspace.
    let current = workspace_root().ok();

    let out = parse_workspace_list(&raw)
        .into_iter()
        .map(|row| {
            let path = PathBuf::from(row.root);
            let is_current = current.as_deref() == Some(path.as_path());
            Workspace {
                name: row.name,
                path,
                change_id: row.change_id,
                description: row.description,
                conflict: row.conflict,
                empty: row.empty,
                stale: false, // best-effort; jj 0.42 exposes no stale flag in the list template
                is_current,
            }
        })
        .collect();
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

/// `jj workspace add --name <name> <path>` (fresh change). Thin wrapper.
pub fn add_workspace(name: &str, path: &Path) -> anyhow::Result<()> {
    add_workspace_at(name, path, None)
}

/// `jj workspace forget <name>`.
pub fn forget_workspace(name: &str) -> anyhow::Result<()> {
    run_jj(&["workspace", "forget", name]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Columns: name, root, change_id, description, conflict, empty.
    const FIXTURE: &str = "\
default\t/repo\tmpmxtzuz\tDesign doc + M0 prototype\t0\t0
auth\t/repo.auth\t3f2a9c1c\twire up oauth callback\t1\t0
docs\t/repo.docs\t9a0b1c2d\t(no description)\t0\t1
";

    #[test]
    fn parses_tab_delimited_rows() {
        let rows = parse_workspace_list(FIXTURE);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "default");
        assert_eq!(rows[0].root, "/repo");
        assert_eq!(rows[1].name, "auth");
        assert_eq!(rows[1].root, "/repo.auth");
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

    #[test]
    fn classify_detects_git_vs_nothing() {
        assert_eq!(
            classify_no_repo("Error: There is no jj repo in \".\"", true),
            Some(NoRepo::GitRepoNoJj)
        );
        assert_eq!(
            classify_no_repo("Error: There is no jj repo in \".\"", false),
            Some(NoRepo::Nothing)
        );
        assert_eq!(classify_no_repo("some other failure", true), None);
    }

    #[test]
    fn no_repo_messages_name_the_right_command() {
        assert!(
            NoRepo::GitRepoNoJj
                .message()
                .contains("jj git init --colocate")
        );
        assert!(NoRepo::Nothing.message().contains("jj git init"));
        assert!(!NoRepo::GitRepoNoJj.message().contains("self.name()")); // no template echo
    }

    #[test]
    fn parses_bookmark_presence_buckets() {
        let raw = "feat\t\nfeat\tgit\nfeat\torigin\nother\torigin\n";
        let p = parse_bookmark_presence(raw, "feat");
        assert!(p.local);
        assert!(p.git);
        assert_eq!(p.remote.as_deref(), Some("origin"));
        assert_eq!(
            parse_bookmark_presence("", "feat"),
            BookmarkPresence::default()
        );
    }
}

#[cfg(test)]
mod contract {
    use super::*;
    // --- Contract assertions: pin the public signatures the picker depends on ---
    const _: fn() -> anyhow::Result<Vec<Workspace>> = list_workspaces;
    const _: fn(&str) -> anyhow::Result<String> = diff_stat;
    const _: fn(&str, &Path) -> anyhow::Result<()> = add_workspace;
    const _: fn(&str) -> anyhow::Result<()> = forget_workspace;
    const _: fn() -> anyhow::Result<PathBuf> = workspace_root;
    const _: fn(&str, bool) -> Option<NoRepo> = classify_no_repo;
    const _: fn(&str) -> anyhow::Result<BookmarkPresence> = bookmark_presence;
    const _: fn(&str, &Path, Option<&str>) -> anyhow::Result<()> = add_workspace_at;
    const _: fn(&str) -> anyhow::Result<()> = fetch_bookmark;

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
