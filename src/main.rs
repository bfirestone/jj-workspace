use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use jw::app::{App, Outcome, Step};
use jw::{config, directive, jj, shell, ui};

#[derive(Parser)]
#[command(name = "jw", about = "Pick a jj workspace and cd into it")]
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
            action:
                ConfigAction::Shell {
                    action: ShellAction::Init { shell },
                },
        }) => {
            let sh: shell::Shell = shell.parse()?;
            // shim goes to stdout (it's meant to be eval'd), not /dev/tty.
            print!("{}", shell::shim(sh));
            Ok(())
        }
        None => run_picker(),
    }
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
    let workspaces = jj::list_workspaces()?;
    if workspaces.is_empty() {
        anyhow::bail!("no jj workspaces found (are you inside a jj repo?)");
    }
    let repo_root = jj::workspace_root()?;
    let config = config::load();
    let mut app = App::new(workspaces, repo_root, config);

    // Open /dev/tty so stdout/stderr stay clean for non-TUI output.
    // Clone the fd up-front (before enabling raw mode) so the fail-fast happens early.
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
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
                jj::add_workspace(&name, &path)?;
                return Ok(Some(Outcome::Cd(path)));
            }
            Step::Forget { name } => {
                jj::forget_workspace(&name)?;
                if let Ok(ws) = jj::list_workspaces() {
                    app.set_workspaces(ws);
                }
                // else keep the stale list and keep looping
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
}
