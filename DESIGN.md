# jj workspace picker — design doc

> **Decided:** project `jj-workspace` (dir being renamed from `jj-worktrunk`); command/binary `jw`.
> Status: **design refined & resolved; project initialized; not yet coded.** Originally written
> 2026-06-22 in another repo; picked up the same day. **All §8 open questions are now resolved and
> the implementation-ready spec lives in [`docs/plans/2026-06-22-jj-workspace-picker.md`](docs/plans/2026-06-22-jj-workspace-picker.md).**
> This file keeps the long-form background; the plan doc is the source of truth for building.
>
> Resolved since first draft: cd mechanism is now worktrunk's **directive-file** pattern (not
> stdout=path); one batched `jj workspace list -T` replaces the per-name `jj log` loop; `o`/`a`
> open actions; sibling `<repo>.<name>` path template; v1 = M1+M2+M3.

---

## 0. TL;DR

A **small, ephemeral Rust + Ratatui picker** for **Jujutsu (jj) workspaces**, modeled on
worktrunk's `wt switch`: launch it, fuzzy-filter the list of workspaces, hit Enter, and your
shell is now `cd`'d into that workspace's directory. It quits immediately after selection — it is
NOT a persistent "live in it" TUI.

It exists because workspace directory names get long and typing `cd ../repo.some-long-feature-name`
every time to hop between parallel coding-agent workspaces is friction. A picker removes the typing.

**Differentiator over worktrunk:** because jj makes per-workspace state trivially queryable, the
picker shows a **live preview** of each workspace (current change description, diff stat, conflict
state) in a side pane — so you pick by *"where's the work I care about"*, not by remembering names.

This is deliberately scoped *tiny*: one screen, a filterable list, a preview pane, cd-on-exit. No
DAG rendering, no diff viewer, none of the hard TUI work. Use **lazyjj** or **jjui** as the
full stack-ops TUI; this tool only does workspace switching.

---

## 1. Background & why this shape (context for a cold reader)

This came out of a longer exploration. Condensed conclusions that justify the design:

- The user runs **multiple AI coding agents in parallel**, each in its own working directory.
  Today that's `git worktree` managed by **worktrunk** (`wt`, Rust). They're evaluating moving to
  **jujutsu (jj)**, whose equivalent of worktrees is **workspaces** (`jj workspace add`).
- jj would consolidate worktrees + PR-stack restacking + WIP sync into one tool (see the parent
  exploration). The one thing jj's ecosystem lacks is worktrunk's **`wt switch` picker UX** for
  jumping between parallel working copies. Neither **jjui** (Go/BubbleTea) nor **lazyjj**
  (Rust/Ratatui) has workspace switching.
- A persistent TUI (lazyjj/jjui) is the *wrong host* for "switch and leave": those tools run, you
  work, you quit back to where you started. This need is the opposite — a tool whose whole job is
  to **exit somewhere else**. So it must be its own tiny binary, not a lazyjj PR.
- The user wants to build in **Rust + Ratatui** (worktrunk's stack; also a learning goal). This
  picker is a well-scoped first Rust+Ratatui+jj project — a weekend, not months.

### Naming caution
`jj-worktrunk` borrows worktrunk's name (a different author's tool). For the published binary pick
an original name (`jjws` / `jjump` / `wsp`). The dir name is just a placeholder.

### Build-location note (verified 2026-06-22 — earlier warning was machine-specific)
The earlier draft warned: "don't init VCS under `~/devspace/personal` — Seafile corrupts `.git`."
**On this machine that premise is false.** `~/devspace` is a symlink to
`/Volumes/ExternalSamsung/devspace`, and the running `seaf-daemon` watches
`/Volumes/ExternalSamsung/Seafile` — a *sibling* of `devspace`, not this project. So jj/git here is
**not** raced by Seafile. The repo was therefore initialized in place: **jj-native
(`jj git init`, no colocate)**, repo-local author `ben.firestone@gmail.com`.

The one real caveat: this is an **external/removable drive** — a live repo is fine but don't yank
the drive mid-operation. If you ever clone this onto a machine where `~/devspace` *is* inside a
Seafile library, the original warning applies again — re-verify with
`pgrep -fl seaf-daemon` and check the `-w` worktree path.

---

## 2. Goals / Non-goals

### Goals
1. Launch → fuzzy-filter workspaces → Enter → shell ends up in the chosen workspace dir.
2. Live preview pane per highlighted workspace: change description, diff stat, conflict/empty flags.
3. Fast (sub-100ms to first paint on a repo with a handful of workspaces).
4. Works in bash, zsh, fish via a thin shell shim (the cd-on-exit mechanism).
5. A couple of inline actions beyond cd (create / forget a workspace) — but only after the core works.

### Non-goals (YAGNI — keep it tiny)
- No commit graph, rebase, squash, describe, diff viewer → that's lazyjj/jjui's job.
- No PR/stack management → that's git-spice / a jj forge helper.
- No cross-machine sync → that's jj's `git push` + bookmarks.
- No multi-repo dashboard (v1 is "workspaces of the current repo"). Multi-repo is a maybe-later.

---

## 3. The cd-on-exit mechanism (the one critical architectural decision)

A child process **cannot** change its parent shell's working directory. The canonical solution
(used by zoxide, fzf cd-widgets, worktrunk):

1. The binary renders its TUI to **`/dev/tty`** (or stderr), keeping **stdout clean**.
2. On selection, it prints **only the chosen absolute path** to **stdout**, then exits 0.
3. A **shell function** wraps the binary and `cd`s to its stdout:

```sh
# zsh / bash
jjws() {
  local dir
  dir="$(command jjws-bin "$@")" || return   # TUI drew on /dev/tty; stdout = chosen path
  [ -n "$dir" ] && cd "$dir"
}
```
```fish
function jjws
    set -l dir (command jjws-bin $argv); or return
    test -n "$dir"; and cd $dir
end
```

Exit codes: `0` = path on stdout, `cd`. `130` (or non-zero with empty stdout) = user aborted
(Esc/Ctrl-C), shell function does nothing. Ship `jjws --print-shell {zsh,bash,fish}` to emit the
wrapper so install is `eval "$(jjws-bin --print-shell zsh)"`.

---

## 4. Data layer — exact jj commands (verified against jj 0.42.0)

The binary shells out to `jj` and parses text (same architecture as lazyjj). Always pass
`--ignore-working-copy` on read-only queries so the picker never snapshots/mutates a workspace.

| Need | Command |
|---|---|
| List workspace names | `jj workspace list --ignore-working-copy -T 'name ++ "\n"'` |
| Resolve a workspace's dir | `jj workspace root --name <NAME>` |
| Preview: change summary | `jj log --ignore-working-copy -r '<NAME>@' --no-graph --color always -T <LOG_TMPL>` |
| Preview: diff stat | `jj diff --ignore-working-copy -r '<NAME>@' --stat` |
| Create workspace (action) | `jj workspace add --name <NAME> <PATH>` |
| Remove workspace (action) | `jj workspace forget <NAME>` |
| Repo root (for templated paths) | `jj workspace root` (current) |

Key jj facts that make this work (confirmed in 0.42.0):
- `<NAME>@` is a revset meaning "the working-copy commit of workspace NAME" → per-workspace preview
  with no cd required.
- `jj workspace root --name <NAME>` returns the absolute path → the cd target. **This removes the
  hardest risk** (otherwise you'd have to maintain your own name→path registry like worktrunk does).
- `jj workspace list -T` renders a `WorkspaceRef` template. The `name` keyword is what we need.
  *Future session: run `jj help -k templates` and read the `WorkspaceRef` type to confirm whether
  it also exposes the target commit/path directly — if so, one templated call can replace the
  per-name `workspace root` loop.*

Suggested `<LOG_TMPL>` (tweak to taste):
```
'change_id.shortest(8) ++ " " ++ if(description, description.first_line(), "(no description)")
   ++ if(conflict, " [conflict]", "") ++ if(empty, " [empty]", "")'
```

### Robustness notes
- jj output/templates can shift across versions → pin a tested `jj` version range, keep all jj
  invocations in one module so a format change is a one-file fix.
- Handle the `default` workspace (always present) and **stale** workspaces (jj may print a marker;
  surface "stale — run `jj workspace update-stale`" in the preview rather than failing).
- Empty repo / single workspace → still show the list (just `default`).

---

## 5. UX spec (one screen)

```
┌ jjws ─ agent-marketplace ───────────────────────────────────────────┐
│ > auth_                                   │ auth@  3f2a9c1c          │
│                                           │ "wire up oauth callback" │
│   ▸ auth          ../am.auth              │                          │
│     api           ../am.api               │ M  src/auth/callback.rs  │
│     docs          ../am.docs              │ A  src/auth/oauth.rs     │
│     default       ../agent-marketplace    │ 2 files, +84 -12         │
│                                           │ [conflict]               │
│ 4 workspaces · 1 filtered                 │                          │
└─[enter] cd  [o] open  [n] new  [d] forget  [esc] quit────────────────┘
```

- **Left:** filterable list. Type to fuzzy-filter (matches name; optionally path). `↑/↓` or
  `Ctrl-n/p` to move. Highlight current workspace distinctly.
- **Right:** preview of the highlighted workspace (`<name>@` summary + `jj diff --stat`). Lazy-load
  + cache per name; render `--color always` ANSI via a Ratatui ANSI parser.
- **Keys:** `Enter` cd (print path, exit) · `Esc`/`Ctrl-c` abort · `o` open `$EDITOR`/agent there
  (later) · `n` create workspace (prompt name, derive path from template) · `d` `workspace forget`
  (with confirm).
- v1 can ship with just filter + list + preview + Enter. `o`/`n`/`d` are fast-follows.

---

## 6. Architecture / modules

```
src/
  main.rs        # arg parse (--print-shell), wire app, stdout=path contract
  jj.rs          # ALL jj shell-outs + parsing. Workspace { name, path, summary, stat, flags }
  app.rs         # state: workspaces, filter string, selection, preview cache
  fuzzy.rs       # ranking via `fuzzy-matcher` (SkimMatcherV2)
  ui.rs          # ratatui layout: list (left) + preview (right) + footer
  shell.rs       # emit zsh/bash/fish wrapper strings
```

- Keep **all** `Command::new("jj")` calls in `jj.rs`. Nothing else knows the CLI format.
- `app.rs` is pure state + transitions (testable without a terminal).
- Render to `/dev/tty`; reserve stdout strictly for the final path.

### Crates
- `ratatui` + `crossterm` (TUI + events; same stack as worktrunk/lazyjj).
- `fuzzy-matcher` (skim algorithm) for filtering/highlighting.
- `anyhow` (errors), `clap` (args / `--print-shell`).
- Optional: `ansi-to-tui` to render `jj --color always` output in the preview pane.

---

## 7. Build milestones

- **M0 — prove the UX, zero Rust.** Use the `prototype.sh` in this dir (fzf + jj). If the feel is
  right, proceed. (This is the cheapest validation; do it first.)
- **M1 — minimal Rust picker.** `jj.rs` (list names → resolve paths) + list UI + fuzzy filter +
  Enter prints path + shell shim. This already replaces worktrunk's switch for jj.
- **M2 — preview pane.** `<name>@` summary + `jj diff --stat`, cached, ANSI-rendered. This is the
  "better than worktrunk" moment.
- **M3 — actions.** `n` (workspace add w/ templated path), `d` (workspace forget + confirm),
  `o` (open editor/agent in the dir).
- **M4 — polish.** `--print-shell` installer, config (path template, preview on/off, keybinds),
  stale-workspace surfacing, color themes.
- **Maybe-later — multi-repo.** A registry of repos so the picker can switch across projects, not
  just workspaces of one repo.

---

## 8. Open questions / decisions to make when building

1. **Path template for `n` (new workspace).** Mirror worktrunk's `~/repo.<name>` convention? Make
   it a config string. Decide default.
2. **One-call vs loop for paths.** Confirm whether the `WorkspaceRef` template exposes the path; if
   yes, drop the per-name `jj workspace root` loop for a single `jj workspace list -T`.
3. **`o` semantics.** Spawn `$EDITOR`? Launch the coding agent? Make it a configurable command
   templated with the workspace path.
4. **Preview cost.** `jj diff --stat` per selection — cache by name; debounce on fast cursor moves.
5. **Multi-repo scope.** v1 single-repo. Worth a registry later, or keep tools-do-one-thing?
6. **Distribution.** `cargo install` + `eval "$(jjws --print-shell zsh)"` in rc. Homebrew later.

---

## 9. Testing

- `jj.rs` parsing: unit-test against **captured fixture output** of `jj workspace list -T ...`,
  `jj workspace root --name`, `jj diff --stat` (record real output once, assert parsing).
- `app.rs`: pure state transitions (filter narrows, selection clamps, Enter yields the right path).
- Integration: a throwaway script that `jj git init`s a temp repo, `jj workspace add`s a few, runs
  the binary with scripted keys, asserts the printed path. (Do this on internal disk, not Seafile.)

---

## 10. Prior art / references

- **worktrunk** (`max-sixty/worktrunk`, Rust) — the UX being ported to jj. The `wt switch` picker
  + cd-on-select is the model.
- **lazyjj** (`Cretezy/lazyjj`, Rust + Ratatui, shells out to jj) — the daily-driver TUI; this tool
  complements it, doesn't compete. Good reference for jj-shell-out patterns in Rust.
- **jjui** (`idursun/jjui`, Go + BubbleTea) — the other jj TUI; same workspace-switching gap.
- **zoxide / fzf** — canonical cd-on-exit (`/dev/tty` render, stdout = path, shell wrapper).
- jj docs: `jj help -k templates` (WorkspaceRef), `jj help -k revsets` (`<name>@`).

---

## 11. How to resume this (for the next session)

1. Move this dir to internal disk first (Seafile warning, §1).
2. Run `prototype.sh` (M0) to confirm the feel.
3. `cargo new jjws`; implement `jj.rs` then M1. Spec is §4–§7.
4. The only thing to re-verify on the target machine's jj version: the exact `jj workspace list -T`
   output and `WorkspaceRef` fields (§4 robustness note). Everything else is pinned.
