use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use jw::app::{App, Outcome, Step};
use jw::{config, directive, jj, ops, selfupdate, shell, ui};

#[derive(Parser)]
#[command(name = "jw", version, about = "Pick a jj workspace and cd into it")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Configuration helpers.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Create-or-go to a workspace by name (seeds from a matching bookmark).
    Switch { name: String },
    /// Forget a workspace and delete its directory.
    Remove {
        name: String,
        /// Forget the workspace but keep its directory on disk.
        #[arg(long)]
        keep: bool,
        /// Skip the dirty/conflict guard and the confirmation prompt.
        #[arg(long)]
        force: bool,
    },
    /// Manage the jw binary itself.
    #[command(name = "self")]
    SelfCmd {
        #[command(subcommand)]
        action: SelfAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Shell integration.
    Shell {
        #[command(subcommand)]
        action: ShellAction,
    },
}

#[derive(Subcommand)]
enum ShellAction {
    /// Print the shell shim for zsh|bash|fish.
    Init { shell: String },
    /// Write the shim source line into your shell's rc file (idempotent).
    /// Shell is auto-detected from $SHELL when omitted.
    Install { shell: Option<String> },
}

#[derive(Subcommand)]
enum SelfAction {
    /// Update jw to the latest release (or a pinned --version).
    Update {
        /// Report whether a newer version exists, then exit (no install).
        /// Exits 1 when an update is available, 0 when already current.
        #[arg(long)]
        check: bool,
        /// Install a specific version (X.Y.Z) instead of the latest.
        #[arg(long)]
        version: Option<String>,
        /// Reinstall even if already up to date.
        #[arg(long)]
        force: bool,
    },
}

fn resolve_cmd(cmd: &str) -> String {
    if cmd == "${EDITOR:-vi}" {
        std::env::var("EDITOR")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "vi".into())
    } else {
        cmd.to_string()
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Config {
            action: ConfigAction::Shell { action },
        }) => match action {
            ShellAction::Init { shell } => {
                let sh: shell::Shell = shell.parse()?;
                // shim goes to stdout (it's meant to be eval'd), not /dev/tty.
                print!("{}", shell::shim(sh));
                Ok(())
            }
            ShellAction::Install { shell } => install_shell(shell),
        },
        Some(Command::Switch { name }) => run_switch(&name),
        Some(Command::Remove { name, keep, force }) => run_remove(&name, keep, force),
        Some(Command::SelfCmd {
            action:
                SelfAction::Update {
                    check,
                    version,
                    force,
                },
        }) => run_self_update(check, version, force),
        None => run_picker(),
    }
}

/// `switch <name>`: create-or-go to a workspace, then cd into it.
fn run_switch(name: &str) -> Result<()> {
    let repo_root = jj::workspace_root()?;
    let config = config::load();
    let path = ops::switch(name, &config, &repo_root)?;
    directive::emit_cd(&path)?;
    Ok(())
}

/// `remove <name>`: forget the workspace and (unless --keep) delete its dir.
fn run_remove(name: &str, keep: bool, force: bool) -> Result<()> {
    let prompt = if keep {
        format!("forget workspace '{name}' (keeping its directory)?")
    } else {
        format!("remove workspace '{name}' and delete its directory?")
    };
    if !force && !confirm(&prompt)? {
        eprintln!("aborted");
        return Ok(());
    }
    ops::remove(name, ops::RemoveOpts { keep, force })?;
    eprintln!(
        "{} workspace '{name}'",
        if keep { "forgot" } else { "removed" }
    );
    Ok(())
}

/// `self update`: resolve + (unless --check) download/verify/replace the binary.
fn run_self_update(check: bool, version: Option<String>, force: bool) -> Result<()> {
    let outcome = selfupdate::run_update(selfupdate::UpdateOpts {
        check,
        version,
        force,
    })?;
    println!("{}", outcome.message());
    if matches!(outcome, selfupdate::UpdateOutcome::UpdateAvailable(_)) {
        std::process::exit(1);
    }
    Ok(())
}

/// Plain stdin `[y/N]` prompt (default No). Returns true on y/yes.
fn confirm(question: &str) -> Result<bool> {
    use std::io::Write;
    eprint!("{question} [y/N] ");
    std::io::stderr().flush().ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim(), "y" | "Y" | "yes" | "Yes"))
}

/// `config shell install [shell]`: splice jw's source line into the shell's rc
/// file. Auto-detects the shell from `$SHELL` when not given.
fn install_shell(shell: Option<String>) -> Result<()> {
    let sh: shell::Shell = match shell {
        Some(s) => s.parse()?,
        None => shell::detect_shell().context(
            "could not detect shell from $SHELL; pass one explicitly: \
             jw config shell install <zsh|bash|fish>",
        )?,
    };
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .context("$HOME is not set")?;
    let rc = shell::rc_path_for(sh, &home);

    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    let updated = shell::apply_install(&existing, &shell::source_block(sh));
    if updated == existing {
        println!(
            "jw: shell integration already up to date in {}",
            rc.display()
        );
        return Ok(());
    }
    if let Some(parent) = rc.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&rc, updated).with_context(|| format!("writing {}", rc.display()))?;
    println!("jw: shell integration installed to {}", rc.display());
    println!("    restart your shell or run:  source {}", rc.display());
    Ok(())
}

/// RAII guard that restores raw mode and alternate screen on every exit path,
/// including `?`-propagated errors and panics.
struct TermGuard {
    w: std::fs::File,
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        crossterm::execute!(self.w, LeaveAlternateScreen).ok();
    }
}

fn run_picker() -> Result<()> {
    let workspaces = match jj::list_workspaces() {
        Ok(w) => w,
        Err(e) => {
            // No jj repo here → print a clean, jj-style hint and exit instead
            // of echoing the raw `jj workspace list` failure + template.
            if let Some(reason) = jj::classify_no_repo(&e.to_string(), jj::in_git_repo()) {
                eprintln!("{}", reason.message());
                std::process::exit(1);
            }
            return Err(e);
        }
    };
    if workspaces.is_empty() {
        anyhow::bail!("no jj workspaces found (are you inside a jj repo?)");
    }
    // The repo root is just the current workspace's root, which `list_workspaces`
    // already resolved — reuse it instead of paying for another `jj` shell-out on
    // the launch hot path. Fall back to an explicit query only if no workspace is
    // marked current (e.g. cwd outside any workspace tree).
    let repo_root = match workspaces.iter().find(|w| w.is_current) {
        Some(w) => w.path.clone(),
        None => jj::workspace_root()?,
    };
    let config = config::load();
    let mut app = App::new(workspaces, repo_root, config);

    // Open /dev/tty so stdout/stderr stay clean for non-TUI output.
    // Clone the fd up-front (before enabling raw mode) so the fail-fast happens early.
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .context("jw must be run in an interactive terminal")?;
    let mut backend_tty = tty.try_clone()?;
    let guard_tty = tty.try_clone()?;

    enable_raw_mode()?;
    crossterm::execute!(backend_tty, EnterAlternateScreen)?;
    // Guard is constructed AFTER raw mode is enabled; its Drop will always run,
    // even on `?`-propagated errors and panics.
    let _guard = TermGuard { w: guard_tty };
    let mut terminal = Terminal::new(CrosstermBackend::new(backend_tty))?;

    let outcome = run_loop(&mut terminal, &mut app);

    // _guard drops here (or on any earlier exit), restoring raw mode + alternate screen
    // before we act on the outcome.
    drop(_guard);

    match outcome? {
        Some(Outcome::Cd(p)) => directive::emit_cd(&p)?,
        Some(Outcome::Open { path, cmd }) => {
            directive::emit_cd(&path)?;
            directive::emit_run(&resolve_cmd(&cmd))?;
        }
        Some(Outcome::Abort) | None => {} // both directive files stay empty
    }
    Ok(())
}

/// Runs the event loop. Returns the final Outcome (None on a clean abort).
fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<Option<Outcome>> {
    loop {
        // Lazy-load the preview for the current selection.
        if let Some(ws) = app.selected_workspace() {
            let name = ws.name.clone();
            if app.cached_preview(&name).is_none() {
                let body = jj::diff_stat(&name).unwrap_or_default();
                app.cache_preview(&name, body);
            }
        }
        terminal.draw(|f| ui::render(f, app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Filter out key release events (crossterm 0.28 may deliver them on some platforms).
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.on_key(key) {
            Step::Continue => {}
            Step::Done(Outcome::Abort) => return Ok(None),
            Step::Done(o) => return Ok(Some(o)),
            Step::Create { name, path } => {
                // Seed the new workspace from a matching bookmark (local/@git/remote).
                ops::create_seeded(&name, &path)?;
                return Ok(Some(Outcome::Cd(path)));
            }
            Step::Forget { name } => {
                // The TUI already confirmed and the action is gated to non-current
                // workspaces, so force past the CLI dirty guard here. Deletes the dir.
                ops::remove(
                    &name,
                    ops::RemoveOpts {
                        keep: false,
                        force: true,
                    },
                )?;
                if let Ok(ws) = jj::list_workspaces() {
                    app.set_workspaces(ws);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_default_editor_sentinel() {
        unsafe {
            std::env::set_var("EDITOR", "nvim");
        }
        assert_eq!(resolve_cmd("${EDITOR:-vi}"), "nvim");
        unsafe {
            std::env::remove_var("EDITOR");
        }
        assert_eq!(resolve_cmd("${EDITOR:-vi}"), "vi");
    }

    #[test]
    fn resolve_passes_through_concrete() {
        assert_eq!(resolve_cmd("claude"), "claude");
    }

    #[test]
    fn cli_parses_shell_init() {
        let cli = Cli::try_parse_from(["jw", "config", "shell", "init", "zsh"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Config { action: ConfigAction::Shell { action: ShellAction::Init { ref shell } } }) if shell == "zsh"
        ));
    }

    #[test]
    fn cli_parses_bare_run() {
        let cli = Cli::try_parse_from(["jw"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_parses_shell_install_with_and_without_arg() {
        let with = Cli::try_parse_from(["jw", "config", "shell", "install", "bash"]).unwrap();
        assert!(matches!(
            with.command,
            Some(Command::Config { action: ConfigAction::Shell { action: ShellAction::Install { shell: Some(ref s) } } }) if s == "bash"
        ));
        let without = Cli::try_parse_from(["jw", "config", "shell", "install"]).unwrap();
        assert!(matches!(
            without.command,
            Some(Command::Config {
                action: ConfigAction::Shell {
                    action: ShellAction::Install { shell: None }
                }
            })
        ));
    }

    #[test]
    fn cli_parses_switch_and_remove() {
        let s = Cli::try_parse_from(["jw", "switch", "feat"]).unwrap();
        assert!(matches!(s.command, Some(Command::Switch { ref name }) if name == "feat"));
        let r = Cli::try_parse_from(["jw", "remove", "feat", "--keep", "--force"]).unwrap();
        assert!(matches!(
            r.command,
            Some(Command::Remove { ref name, keep: true, force: true }) if name == "feat"
        ));
    }

    #[test]
    fn cli_parses_self_update() {
        let c = Cli::try_parse_from(["jw", "self", "update", "--check"]).unwrap();
        assert!(matches!(
            c.command,
            Some(Command::SelfCmd {
                action: SelfAction::Update { check: true, .. }
            })
        ));
        let v =
            Cli::try_parse_from(["jw", "self", "update", "--version", "0.2.0", "--force"]).unwrap();
        assert!(matches!(
            v.command,
            Some(Command::SelfCmd { action: SelfAction::Update { force: true, ref version, .. } })
                if version.as_deref() == Some("0.2.0")
        ));
    }
}
