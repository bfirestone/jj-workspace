#!/usr/bin/env bash
# M0 prototype: worktrunk-style jj workspace picker, zero Rust.
# Validates the UX before building the real Rust+Ratatui binary (see DESIGN.md).
#
# Requires: jj (>=0.42), fzf.
# Usage:    source prototype.sh   # then run:  jjws
# Run this from inside a jj repo that has workspaces (`jj workspace add ...`).

jjws() {
  command -v jj  >/dev/null || { echo "jj not found"  >&2; return 1; }
  command -v fzf >/dev/null || { echo "fzf not found" >&2; return 1; }

  local log_tmpl='change_id.shortest(8) ++ " " ++ if(description, description.first_line(), "(no description)") ++ if(conflict, " [conflict]", "") ++ if(empty, " [empty]", "")'

  local name
  name=$(
    jj workspace list --ignore-working-copy -T 'name ++ "\n"' \
    | fzf --ansi --height=40% --reverse --prompt='workspace> ' \
        --preview "jj log --ignore-working-copy -r '{}@' --no-graph --color always -T '$log_tmpl'; echo; jj diff --ignore-working-copy -r '{}@' --stat 2>/dev/null" \
        --preview-window=right,55%
  ) || return   # aborted

  [ -n "$name" ] || return

  local dir
  dir=$(jj workspace root --name "$name") || { echo "could not resolve '$name'" >&2; return 1; }
  cd "$dir" || return
  echo "→ $name  ($dir)"
}

# Optional: create + jump to a new workspace, worktrunk `wt switch -c` style.
# Path template mirrors worktrunk's `<repo>.<name>` convention.
jjws-new() {
  local name="$1"; [ -n "$name" ] || { echo "usage: jjws-new <name>" >&2; return 1; }
  local root base path
  root=$(jj workspace root) || return
  base=$(basename "$root")
  path="$(dirname "$root")/${base}.${name}"
  jj workspace add --name "$name" "$path" || return
  cd "$path" && echo "→ created $name  ($path)"
}
