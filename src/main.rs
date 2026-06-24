use std::ffi::OsStr;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate, ValueCompleter};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use jw::app::{self, App, Outcome, Step};
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
    /// Switch to a workspace and cd into it.
    ///
    /// With no NAME, open the interactive picker. `jw switch <name>` switches to an
    /// existing workspace; `jw switch -c <name>` creates a new one (seeded from a
    /// matching bookmark), like `git switch` / `git switch -c`. `jw switch ^` jumps
    /// to the repo root (the `default` workspace).
    Switch {
        /// Workspace to switch to. `^` is the repo root (default workspace). Omit to
        /// open the interactive picker.
        #[arg(add = workspace_name_completer())]
        name: Option<String>,
        /// Create a new workspace named NAME instead of switching to an existing one.
        #[arg(short = 'c', long)]
        create: bool,
        /// Print the resolved workspace path to stdout (in addition to switching).
        #[arg(long)]
        print_path: bool,
    },
    /// List workspaces (name, change id, path, description) to stdout.
    List,
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

/// Dynamic shell completer for workspace names: `jw switch de<tab>` → `default`.
/// Runs at completion time, so the candidates are the repo's live workspaces.
#[derive(Clone)]
struct WorkspaceNames;

impl ValueCompleter for WorkspaceNames {
    fn complete(&self, current: &OsStr) -> Vec<CompletionCandidate> {
        let prefix = current.to_string_lossy();
        // Outside a jj repo (or on any jj error), offer nothing rather than fail.
        jj::list_workspaces()
            .unwrap_or_default()
            .into_iter()
            .map(|w| w.name)
            .filter(|name| name.starts_with(prefix.as_ref()))
            .map(CompletionCandidate::new)
            .collect()
    }
}

fn workspace_name_completer() -> ArgValueCompleter {
    ArgValueCompleter::new(WorkspaceNames)
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
    // Handle shell completion requests (`COMPLETE=<shell> jw …`) and exit early;
    // a normal invocation (no COMPLETE env) just falls through to parsing.
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

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
        Some(Command::Switch {
            name,
            create,
            print_path,
        }) => run_switch(name, create, print_path),
        Some(Command::List) => run_list(),
        Some(Command::Remove { name, keep, force }) => run_remove(&name, keep, force),
        Some(Command::SelfCmd {
            action:
                SelfAction::Update {
                    check,
                    version,
                    force,
                },
        }) => run_self_update(check, version, force),
        // Bare `jw` prints help; the picker is `jw switch` (no name).
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

/// `switch`: with no name, open the picker; `switch <name>` goes to an existing
/// workspace; `switch -c <name>` creates a new one. Then cd into the result.
fn run_switch(name: Option<String>, create: bool, print_path: bool) -> Result<()> {
    let Some(name) = name else {
        if create {
            anyhow::bail!("`jw switch -c` needs a workspace name");
        }
        return run_picker();
    };
    let path = if name == "^" {
        // worktrunk-style "back to root": the default workspace / repo root.
        // Resolved directly so it works even when the default workspace's path
        // was never recorded (jj's "Workspace has no recorded path").
        jj::repo_root()?
    } else if create {
        let repo_root = jj::workspace_root()?;
        let config = config::load();
        ops::create(&name, &config, &repo_root)?
    } else {
        ops::go(&name)?
    };
    directive::emit_cd(&path)?;
    if print_path {
        println!("{}", path.display());
    }
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

/// Load the workspace list, turning jj's raw "no repo" failure into a clean,
/// jj-style hint + `exit(1)`. Shared by the picker and `jw list`.
fn load_workspaces() -> Result<Vec<jj::Workspace>> {
    match jj::list_workspaces() {
        Ok(w) => Ok(w),
        Err(e) => {
            // No jj repo here → print a clean, jj-style hint and exit instead
            // of echoing the raw `jj workspace list` failure + template.
            if let Some(reason) = jj::classify_no_repo(&e.to_string(), jj::in_git_repo()) {
                eprintln!("{}", reason.message());
                std::process::exit(1);
            }
            Err(e)
        }
    }
}

/// `jw list`: print the workspaces (name, change id, path, description) to stdout.
fn run_list() -> Result<()> {
    let workspaces = load_workspaces()?;
    print!("{}", app::format_list(&workspaces));
    Ok(())
}

fn run_picker() -> Result<()> {
    let workspaces = load_workspaces()?;
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

/// How long to wait after a lone `Esc` for a follow-up key before treating it as a
/// genuine Escape. Terminals deliver `Alt+<key>` as the bytes `ESC` then `<key>`;
/// when they land in separate reads crossterm surfaces a bare `Esc`. This window
/// lets us recombine them (same trick readline/vim use). Kept short so a real
/// Escape still feels instant, but comfortably longer than the sub-ms gap between
/// the two bytes of a single Alt chord.
const ESC_SEQUENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

/// Read the next key *press*, folding a lone `Esc` that is immediately followed by
/// another key into an `Alt`-modified chord. Without this, an `Alt+<key>` delivered
/// as a split `ESC`/`<key>` pair reads as `Esc` → aborts the picker (the user
/// "drops to a shell" and the trailing letter leaks to their prompt).
fn read_key() -> Result<KeyEvent> {
    loop {
        match event::read()? {
            // Filter out release/repeat events (crossterm 0.28 may deliver them).
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                if k.code == KeyCode::Esc
                    && k.modifiers.is_empty()
                    && let Some(folded) = peek_alt_followup()?
                {
                    return Ok(folded);
                }
                return Ok(k);
            }
            _ => continue,
        }
    }
}

/// After a bare `Esc`, wait up to `ESC_SEQUENCE_TIMEOUT` for a follow-up key press.
/// If one arrives the chord was really `Alt+<key>`, so return it with `ALT` set;
/// `None` means the window elapsed with no key (a genuine `Esc`).
fn peek_alt_followup() -> Result<Option<KeyEvent>> {
    let deadline = std::time::Instant::now() + ESC_SEQUENCE_TIMEOUT;
    loop {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return Ok(None);
        };
        if !event::poll(remaining)? {
            return Ok(None);
        }
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                return Ok(Some(KeyEvent {
                    modifiers: k.modifiers | KeyModifiers::ALT,
                    ..k
                }));
            }
            _ => continue, // ignore non-press events; keep waiting within the window
        }
    }
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

        let key = read_key()?;
        match app.on_key(key) {
            Step::Continue => {}
            Step::Done(Outcome::Abort) => return Ok(None),
            Step::Done(o) => return Ok(Some(o)),
            Step::Create { name, path } => {
                // Seed the new workspace from a matching bookmark (local/@git/remote),
                // then return to the list with the new workspace selected so the user
                // can confirm with Enter (rather than auto-cd'ing straight out).
                ops::create_seeded(&name, &path)?;
                if let Ok(ws) = jj::list_workspaces() {
                    app.set_workspaces(ws);
                }
                app.focus_workspace(&name);
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
        assert!(matches!(
            s.command,
            Some(Command::Switch { name: Some(ref n), create: false, print_path: false }) if n == "feat"
        ));
        let r = Cli::try_parse_from(["jw", "remove", "feat", "--keep", "--force"]).unwrap();
        assert!(matches!(
            r.command,
            Some(Command::Remove { ref name, keep: true, force: true }) if name == "feat"
        ));
    }

    #[test]
    fn cli_parses_switch_no_name_and_create() {
        // Bare `jw switch` (no name) → picker; name is None.
        let bare = Cli::try_parse_from(["jw", "switch"]).unwrap();
        assert!(matches!(
            bare.command,
            Some(Command::Switch { name: None, create: false, .. })
        ));
        // `-c <name>` sets the create flag.
        for args in [["jw", "switch", "-c", "feat"], ["jw", "switch", "--create", "feat"]] {
            let c = Cli::try_parse_from(args).unwrap();
            assert!(matches!(
                c.command,
                Some(Command::Switch { name: Some(ref n), create: true, .. }) if n == "feat"
            ));
        }
    }

    #[test]
    fn cli_parses_switch_print_path() {
        let with_flag = Cli::try_parse_from(["jw", "switch", "feat", "--print-path"]).unwrap();
        assert!(matches!(
            with_flag.command,
            Some(Command::Switch { name: Some(ref n), print_path: true, .. }) if n == "feat"
        ));
        let without_flag = Cli::try_parse_from(["jw", "switch", "feat"]).unwrap();
        assert!(matches!(
            without_flag.command,
            Some(Command::Switch { name: Some(ref n), print_path: false, .. }) if n == "feat"
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
