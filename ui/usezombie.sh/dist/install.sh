#!/usr/bin/env bash
# usezombie installer — bootstraps the zombiectl CLI + platform-ops skill.
#
# Usage:
#   curl -fsSL https://usezombie.sh | bash
#   curl -fsSL https://usezombie.sh | bash -s v0.37.0       # version-pinned
#   curl -fsSL https://usezombie.sh | bash -s -- --force    # reinstall, no prompt
#
# Environment variables:
#   USEZOMBIE_INSTALL  - install prefix (default: ~/.usezombie); CLI lands in <prefix>/bin
#   USEZOMBIE_HOST     - force the agent host (claude|amp|codex|opencode); skips detection
#
# zombiectl is a Node CLI distributed on npm, so Node.js (node + npm) is required.
#
# Partial-download safety: helpers + constants below are side-effect-free until the
# final `main "$@"` line runs. If the curl stream truncates, that line never arrives
# and nothing executes — no half-installed system.

# ─── Constants ───────────────────────────────────────────────────────────────

readonly PKG="@usezombie/zombiectl"
readonly SKILL_REF="usezombie/usezombie"
readonly NEXT_CMD="/usezombie-install-platform-ops"
readonly NODE_MIN="18"
readonly NODE_URL="https://nodejs.org"
readonly HOST_CANDIDATES=(claude amp codex opencode)

# Exit codes
readonly EX_NET=1     # network unreachable / DNS failure
readonly EX_NPM=2     # npm install failed (non-network)
readonly EX_PREFIX=3  # install prefix not writable
readonly EX_HOST=4    # host detection ambiguous
readonly EX_NODE=5    # Node toolchain missing

# ─── Colors (only when stdout is a terminal) ─────────────────────────────────

setup_colors() {
  Off='' Red='' Green='' Yellow='' Dim='' Bold='' Cyan=''
  if [[ -t 1 ]]; then
    Off='\033[0m' Red='\033[0;31m' Green='\033[0;32m' Yellow='\033[0;33m'
    Dim='\033[0;2m' Bold='\033[1m' Cyan='\033[0;36m'
  fi
}

# ─── Output helpers ──────────────────────────────────────────────────────────

die()     { local code="$1"; shift; printf "%b\n" "${Red}error${Off}: $*" >&2; exit "$code"; }
warn()    { printf "%b\n" "${Yellow}warn${Off}: $*" >&2; }
info()    { printf "%b\n" "${Dim}  $*${Off}"; }
success() { printf "%b\n" "${Green}  $*${Off}"; }
bold()    { printf "%b\n" "${Bold}$*${Off}"; }

tildify() {
  if [[ $1 == "$HOME"/* ]]; then echo "~${1#"$HOME"}"; else echo "$1"; fi
}

# ─── Argument parsing ────────────────────────────────────────────────────────

parse_args() {
  FORCE=0 VERSION=""
  local arg
  for arg in "$@"; do
    case "$arg" in
      --force) FORCE=1 ;;
      -*)      die "$EX_NPM" "unknown flag: $arg" ;;
      *)       VERSION="$arg" ;;
    esac
  done
  [[ -z "$VERSION" ]] && return 0
  VERSION="${VERSION#v}"
  if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    die "$EX_NPM" "invalid version: ${VERSION}
  Expected a semantic version, e.g. 0.37.0. Usage:
    curl -fsSL https://usezombie.sh | bash -s v0.37.0"
  fi
}

# ─── Host detection ──────────────────────────────────────────────────────────

detect_host() {
  HOST="${USEZOMBIE_HOST:-}"
  [[ -n "$HOST" ]] && return 0
  local found=() candidate
  for candidate in "${HOST_CANDIDATES[@]}"; do
    command -v "$candidate" >/dev/null 2>&1 && found+=("$candidate")
  done
  if (( ${#found[@]} > 1 )); then
    die "$EX_HOST" "multiple agent hosts found on PATH: ${found[*]}.
  The skill install destination differs per host, so pick one:
    USEZOMBIE_HOST=${found[0]} curl -fsSL https://usezombie.sh | bash"
  fi
  HOST="${found[0]:-generic}"
}

# ─── Prerequisites ───────────────────────────────────────────────────────────

require_node() {
  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    die "$EX_NODE" "Node.js (node + npm) is required but was not found.
  zombiectl is a Node CLI — install Node.js >=${NODE_MIN} from ${NODE_URL}, then re-run:
    curl -fsSL https://usezombie.sh | bash"
  fi
}

ensure_prefix() {
  PREFIX="${USEZOMBIE_INSTALL:-$HOME/.usezombie}"
  # The prefix is written verbatim into the shell rc (export PATH="$PREFIX/bin:$PATH").
  # Reject characters that would break out of that quoting or expand at rc-load time.
  if printf "%s" "$PREFIX" | grep -q '["`$]' || [[ "$PREFIX" == *$'\n'* ]]; then
    die "$EX_PREFIX" "USEZOMBIE_INSTALL must not contain quotes, backticks, \$, or newlines."
  fi
  BIN_DIR="$PREFIX/bin"
  if ! mkdir -p "$BIN_DIR" 2>/dev/null || [[ ! -w "$BIN_DIR" ]]; then
    die "$EX_PREFIX" "install prefix is not writable: ${PREFIX}
  Set USEZOMBIE_INSTALL to a writable directory and re-run:
    USEZOMBIE_INSTALL=\$HOME/.usezombie curl -fsSL https://usezombie.sh | bash"
  fi
}

# ─── Idempotent re-install ───────────────────────────────────────────────────

maybe_prompt_upgrade() {
  local existing=""
  if [[ -x "$BIN_DIR/zombiectl" ]]; then
    existing="$BIN_DIR/zombiectl"
  elif command -v zombiectl >/dev/null 2>&1; then
    existing="$(command -v zombiectl)"
  fi
  [[ -z "$existing" ]] && return 0
  info "Existing install detected at $(tildify "$existing") — upgrading."
  (( FORCE )) && return 0
  if [[ -t 0 ]]; then
    local reply=""
    printf "%b" "${Bold}  Re-run install and upgrade? [Y/n] ${Off}" >&2
    read -r reply || reply="y"
    case "$reply" in [nN]*) info "Skipped — existing install left unchanged."; exit 0 ;; esac
  fi
}

# ─── CLI install (npm) ───────────────────────────────────────────────────────

classify_npm_failure() {
  local out="$1"
  printf "%s\n" "$out" >&2
  if printf "%s" "$out" | grep -qiE 'ENOTFOUND|ECONNREFUSED|ETIMEDOUT|getaddrinfo|network'; then
    die "$EX_NET" "network error while installing ${PKG}. Check your connection and retry."
  fi
  die "$EX_NPM" "npm failed to install ${PKG} (see the error above)."
}

install_cli() {
  local spec="$PKG"
  [[ -n "$VERSION" ]] && spec="${PKG}@${VERSION}"
  echo ""
  bold "  Installing ${spec}..."
  echo ""
  local out
  if ! out="$(npm install -g --prefix "$PREFIX" "$spec" 2>&1)"; then
    classify_npm_failure "$out"
  fi
}

# ─── Skill install (npx) ─────────────────────────────────────────────────────

install_skill() {
  info "Adding the platform-ops skill (npx skills add ${SKILL_REF})..."
  local ok=0
  if [[ "$HOST" == "generic" ]]; then
    if npx --yes skills add "$SKILL_REF" </dev/null; then ok=1; fi
  else
    if npx --yes skills add "$SKILL_REF" --host="$HOST" </dev/null; then ok=1; fi
  fi
  if (( ! ok )); then
    local manual="npx skills add ${SKILL_REF}"
    [[ "$HOST" != "generic" ]] && manual="${manual} --host=${HOST}"
    warn "skill install did not complete. Run it manually:
    ${manual}"
  fi
}

# ─── PATH setup ──────────────────────────────────────────────────────────────

shell_config_path() {
  case "$(basename "${SHELL:-}")" in
    zsh)  echo "${ZDOTDIR:-$HOME}/.zshrc" ;;
    bash) [[ "$(uname -s)" == "Darwin" ]] && echo "$HOME/.bash_profile" || echo "$HOME/.bashrc" ;;
    fish) echo "${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/usezombie.fish" ;;
    *)    echo "" ;;
  esac
}

setup_path() {
  if [[ ":$PATH:" == *":$BIN_DIR:"* ]]; then return 0; fi
  local config line
  config="$(shell_config_path)"
  if [[ "$config" == *fish ]]; then
    line="fish_add_path ${BIN_DIR}"
  else
    line="export PATH=\"${BIN_DIR}:\$PATH\""
  fi
  if [[ -n "$config" ]] && { [[ -w "$config" ]] || [[ -w "$(dirname "$config")" ]]; }; then
    mkdir -p "$(dirname "$config")"
    grep -qF "$BIN_DIR" "$config" 2>/dev/null || printf '\n# usezombie\n%s\n' "$line" >>"$config"
    info "Added ${BIN_DIR} to PATH in $(tildify "$config") — restart your shell or run: source $(tildify "$config")"
  else
    info "Add this to your shell config:"
    bold "    ${line}"
  fi
}

# ─── Next-step hint ──────────────────────────────────────────────────────────

print_next() {
  echo ""
  if [[ "$HOST" == "generic" ]]; then
    success "zombiectl installed."
    info "Add the platform-ops skill to your agent host, then run:"
    bold "    ${Cyan}${NEXT_CMD}${Off}"
  else
    success "zombiectl + platform-ops skill installed for ${HOST}."
    bold "  Run ${Cyan}${NEXT_CMD}${Off}${Bold} in ${HOST} to get started.${Off}"
  fi
  echo ""
}

# ─── Orchestration ───────────────────────────────────────────────────────────

main() {
  set -euo pipefail
  setup_colors
  parse_args "$@"
  [[ "${EUID:-$(id -u)}" -eq 0 ]] && warn "running as root is not required for a ~/.usezombie install."
  require_node
  detect_host
  ensure_prefix
  maybe_prompt_upgrade
  install_cli
  install_skill
  setup_path
  print_next
}

# Run the installer — this line MUST be last so a partial download never runs.
main "$@"
