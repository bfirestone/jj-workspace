use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Config;
use crate::fuzzy::{self, Match};
use crate::jj::Workspace;
use crate::keymap::Action;

/// What the user chose. `main.rs` maps this to directive-file writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Cd(PathBuf),
    Open { path: PathBuf, cmd: String },
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    NewName,
    ConfirmForget,
}

/// Result of handling one key. main.rs performs any side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Continue,                               // state updated; redraw + keep looping
    Done(Outcome),                          // terminal: cd / open / abort
    Create { name: String, path: PathBuf }, // main runs jj add, then cd into path
    Forget { name: String },                // main runs jj forget, then refresh list
}

pub struct App {
    workspaces: Vec<Workspace>,
    repo_root: PathBuf,
    config: Config,
    filter: String,
    /// fuzzy::Match list in ranked order; indices into workspaces.
    matches: Vec<Match>,
    /// index into matches
    selected: usize,
    mode: Mode,
    /// new-workspace name buffer (NewName mode)
    input: String,
    /// name -> diff-stat body
    preview: HashMap<String, String>,
}

impl App {
    pub fn new(workspaces: Vec<Workspace>, repo_root: PathBuf, config: Config) -> Self {
        let mut app = Self {
            workspaces,
            repo_root,
            config,
            filter: String::new(),
            matches: Vec::new(),
            selected: 0,
            mode: Mode::Normal,
            input: String::new(),
            preview: HashMap::new(),
        };
        app.recompute();
        app
    }

    fn recompute(&mut self) {
        let names: Vec<String> = self.workspaces.iter().map(|w| w.name.clone()).collect();
        self.matches = fuzzy::rank(&names, &self.filter);
        // Clamp selection to valid range.
        if self.matches.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.matches.len() {
            self.selected = self.matches.len() - 1;
        }
    }

    // --- Accessors ---

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    /// The active color theme (from config; defaults to the legacy palette).
    pub fn theme(&self) -> &crate::config::Theme {
        &self.config.theme
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn total_count(&self) -> usize {
        self.workspaces.len()
    }

    pub fn filtered_count(&self) -> usize {
        self.matches.len()
    }

    pub fn selected_workspace(&self) -> Option<&Workspace> {
        self.matches
            .get(self.selected)
            .map(|m| &self.workspaces[m.index])
    }

    /// Returns (workspace, highlight_positions) for each visible row in ranked order.
    pub fn visible_matches(&self) -> Vec<(&Workspace, &[usize])> {
        self.matches
            .iter()
            .map(|m| (&self.workspaces[m.index], m.positions.as_slice()))
            .collect()
    }

    pub fn cache_preview(&mut self, name: &str, body: String) {
        self.preview.insert(name.to_string(), body);
    }

    pub fn cached_preview(&self, name: &str) -> Option<&str> {
        self.preview.get(name).map(|s| s.as_str())
    }

    /// Replace the workspace list and recompute. Used after a forget.
    pub fn set_workspaces(&mut self, ws: Vec<Workspace>) {
        self.workspaces = ws;
        self.recompute();
    }

    /// Selected index into the current match list. Test-only; production code uses
    /// `selected_workspace()` and `filtered_count()`.
    #[cfg(test)]
    fn selected_index(&self) -> usize {
        self.selected
    }

    // --- Key handling ---

    pub fn on_key(&mut self, ev: KeyEvent) -> Step {
        match self.mode {
            Mode::Normal => self.on_key_normal(ev),
            Mode::NewName => self.on_key_newname(ev),
            Mode::ConfirmForget => self.on_key_confirm(ev),
        }
    }

    fn on_key_normal(&mut self, ev: KeyEvent) -> Step {
        // Configurable action chords take precedence (see keymap.rs).
        if let Some(action) = self.config.keys.resolve(&ev) {
            return self.run_action(action);
        }

        // Hardcoded universal keys: always available, never rebindable, so a
        // misconfigured [keys] table can't lock you out of nav/quit/filtering.
        let alt = ev.modifiers.contains(KeyModifiers::ALT);
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        match ev.code {
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::Char('c') if ctrl => return Step::Done(Outcome::Abort),
            KeyCode::Backspace => {
                self.filter.pop();
                self.recompute();
            }
            KeyCode::Char(c) if !alt && !ctrl => {
                self.filter.push(c);
                self.recompute();
            }
            _ => {}
        }
        Step::Continue
    }

    /// Run a resolved keybinding action. Returns the resulting `Step`; actions on
    /// an empty filtered list (no selection) are no-ops that keep looping.
    fn run_action(&mut self, action: Action) -> Step {
        match action {
            Action::Select => {
                if let Some(w) = self.selected_workspace() {
                    return Step::Done(Outcome::Cd(w.path.clone()));
                }
            }
            Action::Open => {
                if let Some(w) = self.selected_workspace() {
                    return Step::Done(Outcome::Open {
                        path: w.path.clone(),
                        cmd: self.config.edit_cmd.clone(),
                    });
                }
            }
            Action::Agent => {
                if let Some(w) = self.selected_workspace() {
                    return Step::Done(Outcome::Open {
                        path: w.path.clone(),
                        cmd: self.config.agent_cmd.clone(),
                    });
                }
            }
            Action::New => {
                self.mode = Mode::NewName;
                self.input.clear();
            }
            Action::Forget => {
                // Can't forget the current workspace; guard mirrors the emission re-check.
                if self
                    .selected_workspace()
                    .map(|w| !w.is_current)
                    .unwrap_or(false)
                {
                    self.mode = Mode::ConfirmForget;
                }
            }
            Action::Up => self.move_sel(-1),
            Action::Down => self.move_sel(1),
            Action::Abort => return Step::Done(Outcome::Abort),
        }
        Step::Continue
    }

    fn on_key_newname(&mut self, ev: KeyEvent) -> Step {
        let alt = ev.modifiers.contains(KeyModifiers::ALT);
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        match ev.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                // Reject blank or whitespace-only names; stay in NewName mode.
                if self.input.trim().is_empty() {
                    return Step::Continue;
                }
                let name = self.input.clone();
                let path = self.new_workspace_path(&name);
                self.mode = Mode::Normal;
                self.input.clear();
                return Step::Create { name, path };
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) if !alt && !ctrl => {
                self.input.push(c);
            }
            _ => {}
        }
        Step::Continue
    }

    fn on_key_confirm(&mut self, ev: KeyEvent) -> Step {
        let alt = ev.modifiers.contains(KeyModifiers::ALT);
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        match ev.code {
            KeyCode::Char('y') if !alt && !ctrl => {
                // Re-check is_current at emission: defensive against async list refresh.
                if let Some(w) = self.selected_workspace().filter(|w| !w.is_current) {
                    let name = w.name.clone();
                    self.mode = Mode::Normal;
                    return Step::Forget { name };
                }
                // Selected workspace is current (or list is empty): fall back to Normal.
                self.mode = Mode::Normal;
            }
            _ => {
                self.mode = Mode::Normal;
            }
        }
        Step::Continue
    }

    fn move_sel(&mut self, delta: i64) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as i64;
        let new = (self.selected as i64 + delta).clamp(0, len - 1);
        self.selected = new as usize;
    }

    /// Compute the path for a new workspace given a name.
    fn new_workspace_path(&self, name: &str) -> PathBuf {
        let parent = self
            .repo_root
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let repo = self
            .repo_root
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        expand_path_template(&self.config.path_template, &parent, &repo, name)
    }
}

/// Pure function: expand `{parent}`, `{repo}`, `{name}` in `tmpl`.
pub fn expand_path_template(tmpl: &str, parent: &str, repo: &str, name: &str) -> PathBuf {
    PathBuf::from(
        tmpl.replace("{parent}", parent)
            .replace("{repo}", repo)
            .replace("{name}", name),
    )
}

#[cfg(test)]
mod contract {
    use super::*;
    // --- Contract assertions ---
    #[test]
    fn outcome_shapes() {
        let p = PathBuf::from("/tmp");
        let _ = Outcome::Cd(p.clone());
        let _ = Outcome::Open {
            path: p,
            cmd: String::new(),
        };
        let _ = Outcome::Abort;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;

    fn ws(name: &str, current: bool) -> Workspace {
        Workspace {
            name: name.into(),
            path: PathBuf::from(format!("/repo.{name}")),
            change_id: "aaaa".into(),
            description: "d".into(),
            conflict: false,
            empty: false,
            stale: false,
            is_current: current,
        }
    }

    fn app() -> App {
        App::new(
            vec![
                ws("auth", false),
                ws("api", false),
                ws("docs", false),
                ws("default", true),
            ],
            PathBuf::from("/work/repo"),
            Config::default(),
        )
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
    }

    fn code(k: KeyCode) -> KeyEvent {
        KeyEvent::new(k, KeyModifiers::NONE)
    }

    #[test]
    fn typing_filters_the_list() {
        let mut a = app();
        a.on_key(key('a'));
        a.on_key(key('u')); // "au"
        // fuzzy::rank matches both "auth" (score=51) and "default" (score=35) for "au";
        // the list is narrowed and "auth" ranks first (highest score), so it is selected.
        assert!(a.filtered_count() < a.total_count()); // list was narrowed
        assert_eq!(a.selected_workspace().unwrap().name, "auth");
    }

    #[test]
    fn enter_cds_to_selected() {
        let mut a = app();
        match a.on_key(code(KeyCode::Enter)) {
            Step::Done(Outcome::Cd(p)) => assert_eq!(p, PathBuf::from("/repo.auth")),
            other => panic!("expected Cd, got {other:?}"),
        }
    }

    #[test]
    fn esc_aborts() {
        let mut a = app();
        assert!(matches!(
            a.on_key(code(KeyCode::Esc)),
            Step::Done(Outcome::Abort)
        ));
    }

    #[test]
    fn alt_o_opens_editor() {
        let mut a = app();
        match a.on_key(alt('o')) {
            Step::Done(Outcome::Open { cmd, .. }) => assert_eq!(cmd, Config::default().edit_cmd),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn custom_keymap_rebinds_action_and_keeps_universal_nav() {
        // Rebind `select` to ctrl-y; everything else keeps its default.
        let cfg: Config = toml::from_str("[keys]\nselect = \"ctrl-y\"\n").unwrap();
        let mut a = App::new(
            vec![ws("auth", false), ws("api", false)],
            PathBuf::from("/work/repo"),
            cfg,
        );

        // Default Enter no longer selects (it was rebound away) -> no-op.
        assert!(matches!(a.on_key(code(KeyCode::Enter)), Step::Continue));

        // The new chord ctrl-y now selects.
        match a.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)) {
            Step::Done(Outcome::Cd(p)) => assert_eq!(p, PathBuf::from("/repo.auth")),
            other => panic!("expected Cd via ctrl-y, got {other:?}"),
        }

        // Hardcoded Down arrow still navigates regardless of the [keys] table.
        a.on_key(code(KeyCode::Down));
        assert_eq!(a.selected_workspace().unwrap().name, "api");
    }

    #[test]
    fn selection_clamps_on_filter_change() {
        let mut a = app();
        a.on_key(code(KeyCode::Down));
        a.on_key(code(KeyCode::Down));
        a.on_key(code(KeyCode::Down)); // selected = 3 (max)
        a.on_key(key('a'));
        a.on_key(key('u')); // filter "au" yields fewer matches; selected clamps within bounds
        assert!(a.selected_index() < a.filtered_count()); // selection is always valid
        assert!(a.selected_workspace().is_some()); // selection points to a real workspace
    }

    #[test]
    fn alt_n_then_name_then_enter_creates_with_templated_path() {
        let mut a = app();
        a.on_key(alt('n'));
        for c in "feat".chars() {
            a.on_key(key(c));
        }
        match a.on_key(code(KeyCode::Enter)) {
            Step::Create { name, path } => {
                assert_eq!(name, "feat");
                assert_eq!(path, PathBuf::from("/work/repo.feat")); // {parent}/{repo}.{name}
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn cannot_forget_current_workspace() {
        let mut a = app();
        // move selection to "default" (current) which is index 3
        for _ in 0..3 {
            a.on_key(code(KeyCode::Down));
        }
        assert!(a.selected_workspace().unwrap().is_current);
        assert!(matches!(a.on_key(alt('d')), Step::Continue)); // stays normal, no confirm
        assert!(matches!(a.mode(), Mode::Normal));
    }

    #[test]
    fn template_expansion_is_pure() {
        assert_eq!(
            expand_path_template("{parent}/{repo}.{name}", "/work", "repo", "x"),
            PathBuf::from("/work/repo.x"),
        );
    }

    #[test]
    fn alt_n_then_empty_enter_stays_in_newname() {
        let mut a = app();
        a.on_key(alt('n')); // enter NewName mode
        // press Enter immediately with empty input
        let step = a.on_key(code(KeyCode::Enter));
        assert!(matches!(step, Step::Continue));
        assert!(matches!(a.mode(), Mode::NewName));
    }

    #[test]
    fn forget_guard_checks_is_current_at_emission() {
        let mut a = app();
        // Select "auth" (non-current) and enter ConfirmForget
        a.on_key(alt('d'));
        assert!(matches!(a.mode(), Mode::ConfirmForget));
        // Replace workspace list so that the workspace now appears as current
        // (simulates an async refresh racing with the confirm prompt)
        a.set_workspaces(vec![
            ws("auth", true), // now marked current
            ws("api", false),
            ws("docs", false),
            ws("default", false),
        ]);
        // Pressing 'y' should NOT emit Forget because auth is now current
        let step = a.on_key(key('y'));
        assert!(matches!(step, Step::Continue));
        assert!(matches!(a.mode(), Mode::Normal));
    }
}
