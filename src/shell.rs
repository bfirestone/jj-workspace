//! Shell shim templates for the cd-on-exit mechanism.

use std::str::FromStr;

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
const POSIX_SHIM: &str = r#"jw() {
    local cd_file run_file ec=0
    cd_file="$(mktemp)"
    run_file="$(mktemp)"
    JW_DIRECTIVE_CD_FILE="$cd_file" JW_DIRECTIVE_EXEC_FILE="$run_file" \
        command "${JW_BIN:-jw}" "$@" || ec=$?
    if [ -s "$cd_file" ]; then
        builtin cd -- "$(cat "$cd_file")"
    fi
    if [ -s "$run_file" ]; then
        eval "$(cat "$run_file")"
    fi
    rm -f "$cd_file" "$run_file"
    return $ec
}
"#;

const FISH_SHIM: &str = r#"function jw
    set -l cd_file (mktemp)
    set -l run_file (mktemp)
    env JW_DIRECTIVE_CD_FILE=$cd_file JW_DIRECTIVE_EXEC_FILE=$run_file command "$JW_BIN_OR_DEFAULT" $argv
    set -l ec $status
    if test -s $cd_file
        builtin cd (cat $cd_file)
    end
    if test -s $run_file
        eval (cat $run_file)
    end
    rm -f $cd_file $run_file
    return $ec
end
"#;

/// Return the shim text for `shell`, ready to `eval`/`source` in an rc file.
pub fn shim(shell: Shell) -> String {
    match shell {
        Shell::Zsh | Shell::Bash => POSIX_SHIM.to_string(),
        // Resolve the binary name for fish (no `${VAR:-default}` syntax in fish).
        Shell::Fish => FISH_SHIM.replace(
            "\"$JW_BIN_OR_DEFAULT\"",
            "(test -n \"$JW_BIN\"; and echo $JW_BIN; or echo jw)",
        ),
    }
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
        assert!(s.contains("eval"));
        assert!(s.contains("command \"${JW_BIN:-jw}\""));
    }

    #[test]
    fn fish_shim_uses_fish_syntax() {
        let s = shim(Shell::Fish);
        assert!(s.contains("function jw"));
        assert!(s.contains("end"));
        assert!(s.contains("JW_DIRECTIVE_CD_FILE"));
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
        let status = Command::new("zsh").arg("-n").arg(f.path()).status().unwrap();
        assert!(status.success(), "emitted zsh shim failed `zsh -n`");
    }
}
