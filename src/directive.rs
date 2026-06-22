use std::path::Path;
use std::fs;

const CD_ENV: &str = "JW_DIRECTIVE_CD_FILE";
const RUN_ENV: &str = "JW_DIRECTIVE_EXEC_FILE";

fn write_directive(env_var: &str, contents: &str) -> anyhow::Result<()> {
    if let Ok(file) = std::env::var(env_var) {
        if !file.is_empty() {
            fs::write(&file, contents)?;
        }
    }
    Ok(())
}

/// Write a raw absolute path to $JW_DIRECTIVE_CD_FILE if set; no-op if unset.
pub fn emit_cd(path: &Path) -> anyhow::Result<()> {
    write_directive(CD_ENV, &path.to_string_lossy())
}

/// Write shell to be sourced to $JW_DIRECTIVE_EXEC_FILE if set; no-op if unset.
pub fn emit_run(shell_line: &str) -> anyhow::Result<()> {
    write_directive(RUN_ENV, shell_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // NOTE: these tests mutate process env; keep them in one test to avoid races,
    // or use a serial guard. Single-test approach used here for simplicity.
    #[test]
    fn writes_when_env_set_and_noops_when_unset() {
        let cd = tempfile::NamedTempFile::new().unwrap();
        let run = tempfile::NamedTempFile::new().unwrap();

        unsafe {
            std::env::set_var("JW_DIRECTIVE_CD_FILE", cd.path());
            std::env::set_var("JW_DIRECTIVE_EXEC_FILE", run.path());
        }
        emit_cd(std::path::Path::new("/tmp/ws.auth")).unwrap();
        emit_run("cd /tmp/ws.auth && $EDITOR").unwrap();
        assert_eq!(fs::read_to_string(cd.path()).unwrap(), "/tmp/ws.auth");
        assert!(fs::read_to_string(run.path()).unwrap().contains("$EDITOR"));

        unsafe {
            std::env::remove_var("JW_DIRECTIVE_CD_FILE");
            std::env::remove_var("JW_DIRECTIVE_EXEC_FILE");
        }
        // No env → no error, no panic.
        emit_cd(std::path::Path::new("/tmp/whatever")).unwrap();
        emit_run("noop").unwrap();
    }
}

#[cfg(test)]
mod contract {
    use super::*;
    const _: fn(&Path) -> anyhow::Result<()> = emit_cd;
    const _: fn(&str) -> anyhow::Result<()> = emit_run;
}
