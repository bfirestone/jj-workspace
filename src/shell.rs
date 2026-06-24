//! Shell shim templates for the cd-on-exit mechanism.

use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Markers delimiting jw's managed block in a user's rc file. Kept stable across
/// versions so `install` can find and refresh an existing block instead of
/// appending a duplicate (the same trick rustup/nvm use).
const MARKER_START: &str = "# >>> jw shell integration >>>";
const MARKER_END: &str = "# <<< jw shell integration <<<";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl FromStr for Shell {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "zsh" => Ok(Shell::Zsh),
            "bash" => Ok(Shell::Bash),
            "fish" => Ok(Shell::Fish),
            other => anyhow::bail!("unsupported shell: {other} (use zsh|bash|fish)"),
        }
    }
}

// zsh and bash share an identical POSIX body.
//
// Wrapped in a `command -v` guard so the function is only defined when the jw
// binary is actually reachable (on PATH or via $JW_BIN) — a missing binary then
// leaves `jw` untouched instead of shadowing it with a wrapper that fails on
// every call. $JW_BIN lets you point the wrapper at a dev build without
// reinstalling (e.g. `export JW_BIN=~/src/jw/target/debug/jw`).
const POSIX_SHIM: &str = r#"if command -v "${JW_BIN:-jw}" >/dev/null 2>&1; then
jw() {
    local cd_file run_file ec=0
    cd_file="$(mktemp)"
    run_file="$(mktemp)"
    JW_DIRECTIVE_CD_FILE="$cd_file" JW_DIRECTIVE_EXEC_FILE="$run_file" \
        command "${JW_BIN:-jw}" "$@" || ec=$?
    # cd_file holds a raw path. `builtin cd` bypasses any user `cd` alias or
    # function (e.g. zoxide's `alias cd=__zoxide_z`) that would otherwise be
    # substituted into this function body when it is defined.
    if [ -s "$cd_file" ]; then
        builtin cd -- "$(cat "$cd_file")"
    fi
    # run_file holds shell to source (the editor/agent launch from `o`/`a`).
    if [ -s "$run_file" ]; then
        . "$run_file"
    fi
    rm -f "$cd_file" "$run_file"
    return $ec
}
fi
"#;

const FISH_SHIM: &str = r#"if type -q jw; or set -q JW_BIN
    function jw
        set -l cd_file (mktemp)
        set -l run_file (mktemp)
        env JW_DIRECTIVE_CD_FILE=$cd_file JW_DIRECTIVE_EXEC_FILE=$run_file command "$JW_BIN_OR_DEFAULT" $argv
        set -l ec $status
        # `builtin cd` bypasses any user `cd` alias/function (e.g. zoxide).
        if test -s $cd_file
            builtin cd (cat $cd_file)
        end
        if test -s $run_file
            source $run_file
        end
        rm -f $cd_file $run_file
        return $ec
    end
end
"#;

// Tab-completion registration, appended to the shim. Uses clap's dynamic
// completion (`COMPLETE=<shell> jw`) so candidates — including live workspace
// names for `jw switch <tab>` — come from the binary at completion time. The
// generated scripts call the binary by absolute path, so they bypass the `jw()`
// function and need no special handling inside it.

// zsh: a *lazy* wrapper so the binary is only spawned on the first TAB, not at
// every shell startup. Guarded on the binary existing and compinit having run
// (compdef). The first completion evals clap's real registration, which defines
// `_clap_dynamic_completer_jw` and re-binds `compdef` to it for later TABs.
const ZSH_COMPLETION: &str = r#"if command -v "${JW_BIN:-jw}" >/dev/null 2>&1 && (( $+functions[compdef] )); then
    _jw_lazy_complete() {
        if ! (( $+functions[_clap_dynamic_completer_jw] )); then
            eval "$(COMPLETE=zsh command "${JW_BIN:-jw}" 2>/dev/null)" || return
        fi
        _clap_dynamic_completer_jw "$@"
    }
    compdef _jw_lazy_complete jw
fi
"#;

const BASH_COMPLETION: &str = r#"if command -v "${JW_BIN:-jw}" >/dev/null 2>&1; then
    eval "$(COMPLETE=bash command "${JW_BIN:-jw}" 2>/dev/null)"
fi
"#;

// fish's `complete --arguments "(... )"` runs the binary at TAB time already, so
// sourcing the registration once is naturally lazy. `env` bypasses the function.
const FISH_COMPLETION: &str = r#"if type -q jw; or set -q JW_BIN
    env COMPLETE=fish (test -n "$JW_BIN"; and echo $JW_BIN; or echo jw) 2>/dev/null | source
end
"#;

/// Return the shim text for `shell`, ready to `eval`/`source` in an rc file. The
/// `jw()` function comes first, then tab-completion registration.
pub fn shim(shell: Shell) -> String {
    let func = match shell {
        Shell::Zsh | Shell::Bash => POSIX_SHIM.to_string(),
        // Resolve the binary name for fish (no `${VAR:-default}` syntax in fish).
        Shell::Fish => FISH_SHIM.replace(
            "\"$JW_BIN_OR_DEFAULT\"",
            "(test -n \"$JW_BIN\"; and echo $JW_BIN; or echo jw)",
        ),
    };
    let completion = match shell {
        Shell::Zsh => ZSH_COMPLETION,
        Shell::Bash => BASH_COMPLETION,
        Shell::Fish => FISH_COMPLETION,
    };
    format!("{func}\n{completion}")
}

/// Map a login-shell path (e.g. `$SHELL` = `/bin/zsh`) to a known `Shell` by its
/// basename. Returns `None` for unsupported shells so the caller can ask the user
/// to pass one explicitly.
pub fn shell_from_login(login_path: &str) -> Option<Shell> {
    let base = login_path.rsplit('/').next().unwrap_or(login_path);
    base.parse().ok()
}

/// Detect the user's shell from `$SHELL`. Thin env wrapper over `shell_from_login`.
pub fn detect_shell() -> Option<Shell> {
    shell_from_login(&std::env::var("SHELL").ok()?)
}

/// The rc file jw's source line belongs in, for `shell` under `home`.
pub fn rc_path_for(shell: Shell, home: &Path) -> PathBuf {
    match shell {
        Shell::Zsh => home.join(".zshrc"),
        Shell::Bash => home.join(".bashrc"),
        Shell::Fish => home.join(".config/fish/config.fish"),
    }
}

/// The marker-wrapped block to write into an rc file. Sources jw's shim *lazily*
/// (via `config shell init`) so the integration tracks the installed binary and
/// never needs re-installing after an upgrade.
pub fn source_block(shell: Shell) -> String {
    let line = match shell {
        Shell::Zsh => r#"eval "$(jw config shell init zsh)""#.to_string(),
        Shell::Bash => r#"eval "$(jw config shell init bash)""#.to_string(),
        Shell::Fish => "jw config shell init fish | source".to_string(),
    };
    format!("{MARKER_START}\n{line}\n{MARKER_END}\n")
}

/// Idempotently splice `block` into rc-file contents `rc`: replace an existing
/// jw-managed block in place, or append one if absent. Pure (no I/O) so the
/// replace-or-append logic is unit-testable with plain string fixtures.
pub fn apply_install(rc: &str, block: &str) -> String {
    if let (Some(start), Some(end_idx)) = (rc.find(MARKER_START), rc.find(MARKER_END)) {
        // Replace from MARKER_START through the end-of-line after MARKER_END.
        let end = end_idx + MARKER_END.len();
        let tail = &rc[end..];
        let tail = tail.strip_prefix('\n').unwrap_or(tail);
        let block = block.strip_suffix('\n').unwrap_or(block);
        return format!("{}{}\n{}", &rc[..start], block, tail);
    }
    // Append, ensuring a blank-line separation from prior content.
    let mut out = rc.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(block);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shell_names() {
        assert_eq!("zsh".parse::<Shell>().unwrap(), Shell::Zsh);
        assert_eq!("fish".parse::<Shell>().unwrap(), Shell::Fish);
        assert!("powershell".parse::<Shell>().is_err());
    }

    #[test]
    fn posix_shim_has_directive_contract() {
        let s = shim(Shell::Zsh);
        assert!(s.contains("JW_DIRECTIVE_CD_FILE"));
        assert!(s.contains("JW_DIRECTIVE_EXEC_FILE"));
        assert!(s.contains("builtin cd"));
        // Sources the exec directive rather than eval'ing a command substitution.
        assert!(s.contains(". \"$run_file\""));
        assert!(s.contains("command \"${JW_BIN:-jw}\""));
        // Guards the definition on the binary being reachable.
        assert!(s.contains(r#"command -v "${JW_BIN:-jw}""#));
    }

    #[test]
    fn fish_shim_uses_fish_syntax() {
        let s = shim(Shell::Fish);
        assert!(s.contains("function jw"));
        assert!(s.contains("end"));
        assert!(s.contains("JW_DIRECTIVE_CD_FILE"));
        // Guards the definition on the binary being reachable.
        assert!(s.contains("type -q jw"));
    }

    #[test]
    fn shim_registers_tab_completion() {
        // Each shim wires up clap's dynamic completion via `COMPLETE=<shell> jw`.
        let z = shim(Shell::Zsh);
        assert!(z.contains("compdef _jw_lazy_complete jw"));
        assert!(z.contains("COMPLETE=zsh"));

        let b = shim(Shell::Bash);
        assert!(b.contains("COMPLETE=bash"));

        let f = shim(Shell::Fish);
        assert!(f.contains("COMPLETE=fish"));
    }

    #[test]
    fn shell_from_login_reads_basename() {
        assert_eq!(shell_from_login("/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(shell_from_login("/usr/local/bin/fish"), Some(Shell::Fish));
        assert_eq!(shell_from_login("/bin/bash"), Some(Shell::Bash));
        assert_eq!(shell_from_login("/bin/sh"), None);
        assert_eq!(shell_from_login(""), None);
    }

    #[test]
    fn rc_path_maps_each_shell() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(rc_path_for(Shell::Zsh, home), home.join(".zshrc"));
        assert_eq!(rc_path_for(Shell::Bash, home), home.join(".bashrc"));
        assert_eq!(
            rc_path_for(Shell::Fish, home),
            home.join(".config/fish/config.fish")
        );
    }

    #[test]
    fn source_block_is_shell_specific_and_marked() {
        let z = source_block(Shell::Zsh);
        assert!(z.contains(MARKER_START));
        assert!(z.contains(MARKER_END));
        assert!(z.contains(r#"eval "$(jw config shell init zsh)""#));

        let f = source_block(Shell::Fish);
        assert!(f.contains("jw config shell init fish | source"));
    }

    #[test]
    fn install_appends_block_when_absent() {
        let rc = "export PATH=/usr/bin\n";
        let out = apply_install(rc, &source_block(Shell::Zsh));
        assert!(
            out.starts_with("export PATH=/usr/bin\n"),
            "preserves prior rc"
        );
        assert!(out.contains("jw config shell init zsh"));
        // Exactly one block.
        assert_eq!(out.matches(MARKER_START).count(), 1);
    }

    #[test]
    fn install_is_idempotent() {
        let block = source_block(Shell::Zsh);
        let once = apply_install("foo\n", &block);
        let twice = apply_install(&once, &block);
        assert_eq!(once, twice, "re-running install must not duplicate");
    }

    #[test]
    fn install_replaces_a_stale_block_in_place() {
        let block = source_block(Shell::Zsh);
        let installed = apply_install("foo\nbar\n", &block);
        // Simulate a block left by an older jw that ran a different command.
        let stale = installed.replace("jw config shell init zsh", "OLD jw invocation");
        let refreshed = apply_install(&stale, &block);
        assert!(refreshed.contains("jw config shell init zsh"));
        assert!(!refreshed.contains("OLD jw invocation"));
        assert_eq!(
            refreshed.matches(MARKER_START).count(),
            1,
            "no duplicate block"
        );
        assert!(
            refreshed.starts_with("foo\nbar\n"),
            "surrounding rc preserved"
        );
    }

    // Gated: only runs if `zsh` is installed. Asserts the emitted shim is valid syntax.
    #[test]
    fn zsh_shim_is_syntactically_valid() {
        use std::io::Write;
        use std::process::Command;
        if Command::new("zsh").arg("--version").output().is_err() {
            eprintln!("skipping: zsh not installed");
            return;
        }
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(shim(Shell::Zsh).as_bytes()).unwrap();
        let status = Command::new("zsh")
            .arg("-n")
            .arg(f.path())
            .status()
            .unwrap();
        assert!(status.success(), "emitted zsh shim failed `zsh -n`");
    }
}
