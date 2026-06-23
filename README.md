# jw — jj workspace picker

A small, fast terminal picker for [Jujutsu (jj)](https://github.com/jj-vcs/jj) **workspaces**.
Launch it, fuzzy-filter the list, hit Enter — and your shell is now `cd`'d into that workspace's
directory. It quits immediately after selection; it is *not* a persistent "live in it" TUI.

It exists to remove the friction of typing `cd ../repo.some-long-feature-name` to hop between
parallel working copies. The differentiator over a plain directory jumper: because jj makes
per-workspace state trivially queryable, `jw` shows a **live preview** of each workspace (current
change description, diff stat, conflict/empty flags) so you pick by *where the work is*, not by
remembering names.

```
┌ jw ─ agent-marketplace ─────────────────────────────────────────────┐
│ > auth_                                   │ auth@  3f2a9c1c          │
│                                           │ "wire up oauth callback" │
│   ▸ auth          ../am.auth              │                          │
│     api           ../am.api               │ M  src/auth/callback.rs  │
│     docs          ../am.docs              │ A  src/auth/oauth.rs     │
│     default       ../agent-marketplace    │ 2 files, +84 -12         │
│ 4 workspaces · 1 filtered                 │                          │
└─[enter] cd · [M-o] edit · [M-a] agent · [M-n] new · [M-d] forget─────┘
```

## Requirements

- [`jj`](https://github.com/jj-vcs/jj) ≥ 0.42 on your `PATH`.
- A POSIX shell (`zsh`/`bash`) or `fish` for the cd-on-exit integration.

## Install

### Quick install (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/bfirestone/jj-workspace/main/scripts/install.sh | bash
```

Detects your platform, downloads the matching release binary, verifies its SHA-256 checksum,
and installs `jw` to `~/.local/bin`. Options (pass after `bash -s --`): `--version X.Y.Z` to pin
a version, `--bin-dir DIR` to choose the location, `--force` to reinstall.

### From a release binary

Download the archive for your platform from the [latest release](https://github.com/bfirestone/jj-workspace/releases),
extract it, and put `jw` on your `PATH`:

```sh
tar xzf jj-workspace_<version>_<target>.tar.gz
install -m 0755 jj-workspace_<version>_<target>/jw ~/.local/bin/
```

### From source

```sh
cargo install --git https://github.com/bfirestone/jj-workspace
# or, in a clone:
cargo build --release && install -m 0755 target/release/jw ~/.local/bin/
```

## Development

A [go-task](https://taskfile.dev) `Taskfile.yml` wraps the common workflows (run `task` to list them):

```sh
task lint       # cargo fmt --check + clippy -D warnings (mirrors CI)
task test       # unit + integration tests
task ci         # lint + test, the full local gate
task build      # release binary at target/release/jw
task install    # cargo install to ~/.cargo/bin
```

## Shell integration (required for cd-on-exit)

A child process can't change its parent shell's directory, so `jw` ships a tiny shell function
that wraps the binary. The easiest way to set it up is:

```sh
jw config shell install        # writes into ~/.zshrc / ~/.bashrc / config.fish
```

This detects your shell from `$SHELL` (pass `zsh`|`bash`|`fish` to override) and splices a
marker-wrapped block into the matching rc file. It's idempotent — re-running refreshes the block
instead of duplicating it — and because the block sources the shim lazily, it keeps tracking the
installed binary across upgrades. Then restart your shell or `source` the rc file.

Prefer to wire it up by hand? Add the source line yourself:

```sh
# ~/.zshrc or ~/.bashrc
eval "$(jw config shell init zsh)"   # or: bash | fish
```

Either way you get a `jw` function that runs the binary, then `cd`s to (and optionally runs a command
in) the directory `jw` selected. Without it, `jw` will draw the picker but can't move your shell.

## Usage

Run `jw` from anywhere inside a jj repo:

| Key | Action |
|-----|--------|
| *(type)* | Fuzzy-filter the workspace list by name |
| `↑` / `↓`, `Ctrl-p` / `Ctrl-n` | Move the selection |
| `Enter` | `cd` into the selected workspace |
| `Alt-o` | `cd` there and open `$EDITOR` |
| `Alt-a` | `cd` there and launch your configured coding agent |
| `Alt-n` | Create a new workspace (prompts for a name), then `cd` into it |
| `Alt-d` | Forget the selected workspace (with confirmation; can't forget the current one) |
| `Esc` / `Ctrl-c` | Abort — your shell stays put |

> Actions live on `Alt`-chords (not bare letters) because every printable key types into the
> fuzzy filter.

## Configuration

Optional TOML at `~/.config/jw/config.toml` (override the path with `$JW_CONFIG`). All fields have
working defaults, so no config file is needed:

```toml
# Template for new-workspace paths (Alt-n). Placeholders: {parent} {repo} {name}
path_template = "{parent}/{repo}.{name}"
# Command run by Alt-o after cd. The default expands to $EDITOR (or vi).
edit_cmd = "${EDITOR:-vi}"
# Command run by Alt-a after cd — your coding agent.
agent_cmd = "claude"
# Show the preview pane.
preview = true

# Color theme. Every role is optional and defaults to the palette below; values
# accept named colors ("cyan"), hex ("#00ffff"), or 256-indexed ("42").
[theme]
accent       = "yellow"    # prompt and fuzzy-match highlight
marker       = "cyan"      # ▸ selected-row marker and overlay borders
selected     = "white"     # selected row name
normal       = "gray"      # other row names and footer counts
dim          = "darkgray"  # flags, paths, preview border, footer keys
selection_bg = "darkgray"  # selected row background
conflict     = "red"       # [conflict] marker and forget-confirm border
stale        = "yellow"    # [stale] marker

# Keybindings. Every action is optional and defaults to the chord shown. Values
# are key-strings: optional "alt-"/"ctrl-"/"shift-" prefixes plus a key token
# (a single char, or enter/esc/up/down/tab/space/backspace/…). Invalid bindings
# are ignored (that action keeps its default).
[keys]
select = "enter"   # cd into the selected workspace
open   = "alt-o"   # cd + open $EDITOR
agent  = "alt-a"   # cd + launch your coding agent
new    = "alt-n"   # create a new workspace
forget = "alt-d"   # forget the selected workspace
up     = "ctrl-p"  # move selection up
down   = "ctrl-n"  # move selection down
abort  = "esc"     # quit without moving
```

> The arrow keys (navigate), `Ctrl-c` (quit), `Backspace`, and printable-character
> filtering are always available and can't be rebound, so a `[keys]` typo can never
> lock you out. Bind actions to modified chords (`alt-`/`ctrl-`) — a bare letter would
> be swallowed by the fuzzy filter.

## How it works

`jw` renders its UI on `/dev/tty`, keeping `stdout` clean. On selection it writes the chosen path to
a temp file named by `$JW_DIRECTIVE_CD_FILE` (and, for `Alt-o`/`Alt-a`, a command to
`$JW_DIRECTIVE_EXEC_FILE`). The shell function then does `builtin cd` to that path and `eval`s the
command — the same directive-file pattern used by [worktrunk](https://github.com/max-sixty/worktrunk).
All read-only jj queries pass `--ignore-working-copy`, so the picker never snapshots or mutates a
workspace.

## License

MIT — see [LICENSE](LICENSE).
