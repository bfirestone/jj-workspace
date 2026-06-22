use std::path::Path;
use std::process::Command;

fn have(bin: &str) -> bool {
    Command::new(bin).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn jj(cwd: &Path, args: &[&str]) {
    let status = Command::new("jj").args(args).current_dir(cwd).status().unwrap();
    assert!(status.success(), "jj {args:?} failed");
}

#[test]
fn jj_data_layer_end_to_end() {
    if !have("jj") {
        eprintln!("skipping: jj not installed");
        return;
    }
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    // Fresh jj repo with a usable author.
    jj(&repo, &["git", "init"]);
    jj(&repo, &["config", "set", "--repo", "user.name", "Test"]);
    jj(&repo, &["config", "set", "--repo", "user.email", "test@example.com"]);

    // Add a second workspace as a sibling dir.
    let feat = base.path().join("repo.feat");
    jj(&repo, &["workspace", "add", "--name", "feat", feat.to_str().unwrap()]);

    // Drive the library API from inside the repo. Single test => cwd is ours alone here.
    std::env::set_current_dir(&repo).unwrap();

    let list = jw::jj::list_workspaces().expect("list_workspaces");
    let names: Vec<&str> = list.iter().map(|w| w.name.as_str()).collect();
    assert!(names.contains(&"default"), "names were {names:?}");
    assert!(names.contains(&"feat"), "names were {names:?}");

    // default is current (we are in `repo`); its path resolves.
    let def = list.iter().find(|w| w.name == "default").unwrap();
    assert!(def.is_current, "default should be current");
    assert!(def.path.exists());

    // Preview works for a named workspace.
    let stat = jw::jj::diff_stat("feat").expect("diff_stat");
    let _ = stat; // may be empty for an empty change; just shouldn't error.

    // forget removes it from the list.
    jw::jj::forget_workspace("feat").expect("forget");
    let after: Vec<String> = jw::jj::list_workspaces().unwrap().into_iter().map(|w| w.name).collect();
    assert!(!after.contains(&"feat".to_string()), "feat should be gone: {after:?}");
}

#[test]
fn shell_shim_is_valid_zsh() {
    if !have("zsh") {
        eprintln!("skipping: zsh not installed");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_jw"))
        .args(["config", "shell", "init", "zsh"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let shim = String::from_utf8(out.stdout).unwrap();
    assert!(shim.contains("JW_DIRECTIVE_CD_FILE"));
    assert!(shim.contains("builtin cd"));

    // Pipe the shim into `zsh -n` (syntax check, no execution).
    use std::io::Write;
    let mut child = Command::new("zsh")
        .arg("-n")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(shim.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success(), "emitted zsh shim failed `zsh -n`");
}
