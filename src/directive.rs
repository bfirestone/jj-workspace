use std::path::Path;

/// Write a raw absolute path to $JW_DIRECTIVE_CD_FILE if set; no-op if unset.
pub fn emit_cd(_path: &Path) -> anyhow::Result<()> {
    todo!()
}

/// Write shell to be sourced to $JW_DIRECTIVE_EXEC_FILE if set; no-op if unset.
pub fn emit_run(_shell_line: &str) -> anyhow::Result<()> {
    todo!()
}

#[cfg(test)]
mod contract {
    use super::*;
    const _: fn(&Path) -> anyhow::Result<()> = emit_cd;
    const _: fn(&str) -> anyhow::Result<()> = emit_run;
}
