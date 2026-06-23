//! Configurable keybindings.
//!
//! Parses human key-strings (`"alt-o"`, `"ctrl-n"`, `"enter"`) into `KeyChord`s
//! and resolves an incoming `KeyEvent` to a picker `Action`. The parse layer is
//! pure (mirrors the parse/IO seam used in `jj.rs`), so the grammar is fully
//! unit-testable without a terminal.
//!
//! Only the Normal-mode action chords are rebindable. The navigation arrows,
//! `Ctrl-c`, `Backspace`, and printable-char filtering stay hardcoded in
//! `app.rs` as always-on conveniences, so a misconfigured `[keys]` table can
//! never lock you out of navigating, quitting, or typing into the filter.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;

/// A rebindable picker action (Normal mode only). Text-entry keys in the
/// NewName / ConfirmForget modes are intentionally not rebindable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Select,
    Open,
    Agent,
    New,
    Forget,
    Up,
    Down,
    Abort,
}

/// A base key plus its alt/ctrl/shift modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyChord {
    const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self { code, mods }
    }

    /// Parse a key-string like `"alt-o"`, `"ctrl-n"`, `"enter"`, or `"k"`.
    /// Modifier prefixes (`alt-`/`ctrl-`/`shift-`, joined by `-`) precede a key
    /// token. Returns `None` for anything unrecognized so the caller can fall
    /// back to a default binding.
    pub fn parse(s: &str) -> Option<KeyChord> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let parts: Vec<&str> = s.split('-').collect();
        let (mod_parts, key) = parts.split_at(parts.len() - 1);
        let key = key[0];
        if key.is_empty() {
            return None;
        }
        let mut mods = KeyModifiers::NONE;
        for m in mod_parts {
            match m.to_ascii_lowercase().as_str() {
                "alt" | "meta" | "m" => mods |= KeyModifiers::ALT,
                "ctrl" | "control" | "c" => mods |= KeyModifiers::CONTROL,
                "shift" | "s" => mods |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        Some(KeyChord {
            code: parse_key_token(key)?,
            mods,
        })
    }

    /// True if `ev` is this chord. Only the alt/ctrl/shift bits are compared, so
    /// platform-specific extras (e.g. KEYPAD) in the event don't defeat a match.
    pub fn matches(&self, ev: &KeyEvent) -> bool {
        let relevant =
            ev.modifiers & (KeyModifiers::ALT | KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        ev.code == self.code && relevant == self.mods
    }
}

fn parse_key_token(tok: &str) -> Option<KeyCode> {
    let code = match tok.to_ascii_lowercase().as_str() {
        "enter" | "return" | "ret" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "tab" => KeyCode::Tab,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "space" => KeyCode::Char(' '),
        _ => {
            // A single literal character (e.g. "o", "k", "/").
            let mut chars = tok.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // multi-char, unknown token
            }
            KeyCode::Char(c)
        }
    };
    Some(code)
}

/// The full set of action bindings. Defaults reproduce the original hardcoded
/// chords, so a config with no `[keys]` table behaves identically.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "RawKeyBindings")]
pub struct KeyBindings {
    pub select: KeyChord,
    pub open: KeyChord,
    pub agent: KeyChord,
    pub new: KeyChord,
    pub forget: KeyChord,
    pub up: KeyChord,
    pub down: KeyChord,
    pub abort: KeyChord,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            select: KeyChord::new(KeyCode::Enter, KeyModifiers::NONE),
            open: KeyChord::new(KeyCode::Char('o'), KeyModifiers::ALT),
            agent: KeyChord::new(KeyCode::Char('a'), KeyModifiers::ALT),
            new: KeyChord::new(KeyCode::Char('n'), KeyModifiers::ALT),
            forget: KeyChord::new(KeyCode::Char('d'), KeyModifiers::ALT),
            up: KeyChord::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            down: KeyChord::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            abort: KeyChord::new(KeyCode::Esc, KeyModifiers::NONE),
        }
    }
}

impl KeyBindings {
    /// Action/label/chord rows in fixed priority order. The order makes
    /// `resolve` deterministic when two actions share a chord.
    fn entries(&self) -> [(&'static str, Action, KeyChord); 8] {
        [
            ("select", Action::Select, self.select),
            ("open", Action::Open, self.open),
            ("agent", Action::Agent, self.agent),
            ("new", Action::New, self.new),
            ("forget", Action::Forget, self.forget),
            ("up", Action::Up, self.up),
            ("down", Action::Down, self.down),
            ("abort", Action::Abort, self.abort),
        ]
    }

    /// Resolve a key event to its bound action, or `None` if unbound (the caller
    /// then tries the hardcoded universal keys).
    pub fn resolve(&self, ev: &KeyEvent) -> Option<Action> {
        self.entries()
            .into_iter()
            .find(|(_, _, chord)| chord.matches(ev))
            .map(|(_, action, _)| action)
    }

    /// Warn (without failing) if two actions resolve to the same chord — the
    /// lower-priority one becomes unreachable, which is almost always a mistake.
    fn warn_on_conflicts(&self) {
        let e = self.entries();
        for i in 0..e.len() {
            for j in (i + 1)..e.len() {
                if e[i].2 == e[j].2 {
                    eprintln!(
                        "jw: keybinding conflict — '{}' and '{}' are bound to the same key; '{}' wins",
                        e[i].0, e[j].0, e[i].0
                    );
                }
            }
        }
    }
}

/// Raw `[keys]` table: each action is an optional key-string. Deserialized first,
/// then resolved into `KeyBindings` with per-field fallback so one bad binding
/// doesn't discard the rest (mirrors `config::load`'s ignore-invalid policy).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawKeyBindings {
    select: Option<String>,
    open: Option<String>,
    agent: Option<String>,
    new: Option<String>,
    forget: Option<String>,
    up: Option<String>,
    down: Option<String>,
    abort: Option<String>,
}

impl From<RawKeyBindings> for KeyBindings {
    fn from(raw: RawKeyBindings) -> Self {
        let d = KeyBindings::default();
        let pick = |opt: Option<String>, default: KeyChord, name: &str| -> KeyChord {
            match opt {
                Some(s) => KeyChord::parse(&s).unwrap_or_else(|| {
                    eprintln!("jw: ignoring invalid keybinding {name} = {s:?}");
                    default
                }),
                None => default,
            }
        };
        let kb = KeyBindings {
            select: pick(raw.select, d.select, "select"),
            open: pick(raw.open, d.open, "open"),
            agent: pick(raw.agent, d.agent, "agent"),
            new: pick(raw.new, d.new, "new"),
            forget: pick(raw.forget, d.forget, "forget"),
            up: pick(raw.up, d.up, "up"),
            down: pick(raw.down, d.down, "down"),
            abort: pick(raw.abort, d.abort, "abort"),
        };
        kb.warn_on_conflicts();
        kb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn parse_named_and_modified_keys() {
        assert_eq!(
            KeyChord::parse("alt-o"),
            Some(KeyChord::new(KeyCode::Char('o'), KeyModifiers::ALT))
        );
        assert_eq!(
            KeyChord::parse("ctrl-n"),
            Some(KeyChord::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            KeyChord::parse("enter"),
            Some(KeyChord::new(KeyCode::Enter, KeyModifiers::NONE))
        );
        assert_eq!(
            KeyChord::parse("esc"),
            Some(KeyChord::new(KeyCode::Esc, KeyModifiers::NONE))
        );
        // Case-insensitive, whitespace-tolerant.
        assert_eq!(
            KeyChord::parse("  Ctrl-Down "),
            Some(KeyChord::new(KeyCode::Down, KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(KeyChord::parse(""), None);
        assert_eq!(KeyChord::parse("hyper-x"), None); // unknown modifier
        assert_eq!(KeyChord::parse("nope"), None); // unknown multi-char token
        assert_eq!(KeyChord::parse("ctrl-"), None); // empty key
    }

    #[test]
    fn matches_ignores_irrelevant_modifier_bits() {
        let chord = KeyChord::new(KeyCode::Char('o'), KeyModifiers::ALT);
        assert!(chord.matches(&ev(KeyCode::Char('o'), KeyModifiers::ALT)));
        assert!(!chord.matches(&ev(KeyCode::Char('o'), KeyModifiers::NONE)));
        assert!(!chord.matches(&ev(KeyCode::Char('x'), KeyModifiers::ALT)));
    }

    #[test]
    fn default_bindings_resolve_legacy_keys() {
        let kb = KeyBindings::default();
        assert_eq!(
            kb.resolve(&ev(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::Select)
        );
        assert_eq!(
            kb.resolve(&ev(KeyCode::Char('o'), KeyModifiers::ALT)),
            Some(Action::Open)
        );
        assert_eq!(
            kb.resolve(&ev(KeyCode::Char('d'), KeyModifiers::ALT)),
            Some(Action::Forget)
        );
        assert_eq!(
            kb.resolve(&ev(KeyCode::Char('n'), KeyModifiers::CONTROL)),
            Some(Action::Down)
        );
        assert_eq!(
            kb.resolve(&ev(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Abort)
        );
        // A bare printable char is unbound (so it falls through to the filter).
        assert_eq!(
            kb.resolve(&ev(KeyCode::Char('a'), KeyModifiers::NONE)),
            None
        );
    }

    #[test]
    fn raw_override_changes_binding_and_keeps_other_defaults() {
        let raw = RawKeyBindings {
            open: Some("ctrl-e".into()),
            forget: Some("nonsense".into()), // invalid -> default
            ..Default::default()
        };
        let kb = KeyBindings::from(raw);
        // open rebound
        assert_eq!(
            kb.resolve(&ev(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            Some(Action::Open)
        );
        // old open chord is now unbound
        assert_eq!(kb.resolve(&ev(KeyCode::Char('o'), KeyModifiers::ALT)), None);
        // invalid forget fell back to the default alt-d
        assert_eq!(
            kb.resolve(&ev(KeyCode::Char('d'), KeyModifiers::ALT)),
            Some(Action::Forget)
        );
        // untouched bindings keep defaults
        assert_eq!(
            kb.resolve(&ev(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::Select)
        );
    }

    #[test]
    fn duplicate_binding_resolves_by_priority() {
        // Bind `down` to enter, colliding with `select` (higher priority).
        let raw = RawKeyBindings {
            down: Some("enter".into()),
            ..Default::default()
        };
        let kb = KeyBindings::from(raw);
        // select wins on the shared chord (it's earlier in `entries`).
        assert_eq!(
            kb.resolve(&ev(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::Select)
        );
    }
}
