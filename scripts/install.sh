#!/usr/bin/env bash
#
# jw installer
# Usage: curl -fsSL https://raw.githubusercontent.com/bfirestone/jj-workspace/main/scripts/install.sh | bash
#
# Options (pass after `bash -s --`):
#   --version X.Y.Z   Install a specific version (default: latest release)
#   --bin-dir DIR     Install into DIR (default: $HOME/.local/bin)
#   --force           Reinstall even if already up to date
#   --help            Show this help
#
# Environment overrides: VERSION, BIN_DIR, FORCE=true
#
set -euo pipefail

# ============ Configuration ============

REPO="bfirestone/jj-workspace"   # GitHub repo
PROJECT="jj-workspace"           # release archive prefix
BINARY="jw"                      # binary name (inside the archive + on PATH)
TAG_PREFIX="jw-v"                # release tag is <prefix><version>, e.g. jw-v0.2.0

VERSION="${VERSION:-}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
FORCE="${FORCE:-false}"

# ============ Output Formatting ============

if [[ -t 1 ]] && command -v tput &>/dev/null && [[ "$(tput colors 2>/dev/null || echo 0)" -ge 8 ]]; then
    RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'
    BLUE=$'\033[0;34m'; BOLD=$'\033[1m'; DIM=$'\033[2m'; NC=$'\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BLUE=''; BOLD=''; DIM=''; NC=''
fi

log_info()    { echo -e "${BLUE}→${NC} $1"; }
log_success() { echo -e "${GREEN}✓${NC} $1"; }
log_warning() { echo -e "${YELLOW}!${NC} $1"; }
log_error()   { echo -e "${RED}✗${NC} $1" >&2; }
log_step()    { echo -e "${DIM}  $1${NC}"; }

# ============ Cleanup ============

TMP_DIR=""
cleanup() { [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]] && rm -rf "$TMP_DIR"; }
trap cleanup EXIT

# ============ Helpers ============

# Download $1 to stdout (curl or wget).
fetch() {
    if command -v curl &>/dev/null; then
        curl -fsSL "$1"
    elif command -v wget &>/dev/null; then
        wget -qO- "$1"
    else
        log_error "Neither curl nor wget is installed"
        exit 1
    fi
}

# Download $1 to file $2.
fetch_file() {
    if command -v curl &>/dev/null; then
        curl -fsSL -o "$2" "$1"
    else
        wget -qO "$2" "$1"
    fi
}

# Map uname output to the Rust target triple our release matrix builds.
detect_target() {
    local os arch
    case "$(uname -s)" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *) log_error "Unsupported OS: $(uname -s)"; exit 1 ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *) log_error "Unsupported architecture: $(uname -m)"; exit 1 ;;
    esac
    echo "${arch}-${os}"
}

# Currently installed version (bare X.Y.Z), or empty if not installed.
# Tolerant of an older jw that predates `--version` (the pipeline must not abort
# the script under `set -e`/`pipefail`).
installed_version() {
    command -v "$BINARY" &>/dev/null || return 0
    { "$BINARY" --version 2>/dev/null || true; } \
        | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true
}

# Latest release version (bare X.Y.Z) from the GitHub API (empty on failure).
latest_version() {
    fetch "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
        | grep '"tag_name"' | head -1 \
        | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true
}

# Verify $1 against the sha256 in file $2 (format: "<hash>  <name>").
verify_checksum() {
    local file=$1 sumfile=$2 expected actual
    expected=$(awk '{print $1}' "$sumfile")
    if command -v sha256sum &>/dev/null; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum &>/dev/null; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    elif command -v openssl &>/dev/null; then
        actual=$(openssl dgst -sha256 "$file" | awk '{print $NF}')
    else
        log_warning "No sha256 tool found — skipping checksum verification"
        return 0
    fi
    if [[ "$expected" != "$actual" ]]; then
        log_error "Checksum mismatch!"
        log_step "expected: $expected"
        log_step "actual:   $actual"
        return 1
    fi
    log_step "Checksum verified"
}

# Ad-hoc re-sign on macOS so Gatekeeper doesn't kill the downloaded binary.
resign_macos() {
    [[ "$(uname -s)" == "Darwin" ]] || return 0
    command -v codesign &>/dev/null || return 0
    xattr -d com.apple.quarantine "$1" 2>/dev/null || true
    codesign --force --sign - "$1" 2>/dev/null && log_step "Re-signed for macOS" || true
}

# ============ Install ============

main() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version) VERSION="$2"; shift 2 ;;
            --version=*) VERSION="${1#*=}"; shift ;;
            --bin-dir) BIN_DIR="$2"; shift 2 ;;
            --bin-dir=*) BIN_DIR="${1#*=}"; shift ;;
            --force|-f) FORCE="true"; shift ;;
            --help|-h)
                cat <<'EOF'
jw installer

Usage: install.sh [options]   (e.g. curl -fsSL <url> | bash -s -- --force)

  --version X.Y.Z   Install a specific version (default: latest release)
  --bin-dir DIR     Install into DIR (default: $HOME/.local/bin)
  --force           Reinstall even if already up to date
  --help            Show this help

Environment overrides: VERSION, BIN_DIR, FORCE=true
EOF
                exit 0 ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done

    echo ""
    echo -e "${BOLD}jw installer${NC}"
    echo ""

    local target current latest
    target=$(detect_target)
    log_step "Platform: ${target}"

    current=$(installed_version)
    [[ -n "$current" ]] && log_step "Installed: ${current}"

    if [[ -n "$VERSION" ]]; then
        latest="${VERSION#v}"
        log_info "Installing pinned version ${latest}"
    else
        log_info "Resolving latest release..."
        latest=$(latest_version)
        [[ -n "$latest" ]] || { log_error "Could not determine the latest version"; exit 1; }
    fi

    if [[ "$FORCE" != "true" && -n "$current" && "$current" == "$latest" ]]; then
        log_success "jw ${current} is already up to date"
        exit 0
    fi
    [[ -n "$current" ]] && log_info "Updating ${current} → ${latest}" || log_info "Installing jw ${latest}"

    local tag archive url
    tag="${TAG_PREFIX}${latest}"
    archive="${PROJECT}_${latest}_${target}.tar.gz"
    url="https://github.com/${REPO}/releases/download/${tag}/${archive}"

    TMP_DIR=$(mktemp -d)
    log_info "Downloading ${archive}..."
    if ! fetch_file "$url" "${TMP_DIR}/${archive}"; then
        log_error "Download failed — no prebuilt binary for ${target} at ${tag}?"
        exit 1
    fi
    # Checksum is best-effort: verify when the .sha256 asset is present.
    if fetch_file "${url}.sha256" "${TMP_DIR}/${archive}.sha256" 2>/dev/null; then
        ( cd "$TMP_DIR" && verify_checksum "$archive" "${archive}.sha256" )
    else
        log_warning "No published checksum for ${archive} — skipping verification"
    fi

    log_step "Extracting..."
    tar -xzf "${TMP_DIR}/${archive}" -C "$TMP_DIR"

    # The archive holds a <project>_<version>_<target>/ dir with the binary inside.
    local bin_path
    bin_path=$(find "$TMP_DIR" -type f -name "$BINARY" | head -1)
    [[ -n "$bin_path" ]] || { log_error "Binary '${BINARY}' not found in archive"; exit 1; }

    mkdir -p "$BIN_DIR"
    install -m 0755 "$bin_path" "${BIN_DIR}/${BINARY}"
    resign_macos "${BIN_DIR}/${BINARY}"
    log_success "Installed jw ${latest} to ${BIN_DIR}/${BINARY}"

    if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
        echo ""
        log_warning "${BIN_DIR} is not on your PATH"
        echo -e "  Add to your shell profile: ${BOLD}export PATH=\"\$PATH:${BIN_DIR}\"${NC}"
    fi

    echo ""
    echo -e "${BOLD}jw${NC} is ready. Next: enable cd-on-exit shell integration:"
    echo -e "  ${BOLD}jw config shell install${NC}"
    echo ""
}

main "$@"
