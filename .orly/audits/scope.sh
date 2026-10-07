#!/usr/bin/env bash
# Shared filename scope and coherent index reads for deterministic scanners.

AUDIT_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

audit_scope_init() {
  AUDIT_MODE="${SCOPE:-$1}"; shift
  AUDIT_TARGETS=()
  local arg selected=0 literal=0
  for arg in "$@"; do
    if [ "$literal" -eq 1 ]; then AUDIT_TARGETS+=("$arg"); continue; fi
    case "$arg" in
      --) literal=1 ;;
      --staged|staged|--all|all)
        [ "$selected" -eq 0 ] || { printf 'choose one audit mode\n' >&2; return 2; }
        AUDIT_MODE="$arg"; selected=1 ;;
      --strict)
        [ "${0##*/}" = logging.sh ] || { printf 'unsupported audit option: %s\n' "$arg" >&2; return 2; } ;;
      -h|--help) printf 'usage: %s [--all|--staged] | file [...]\n' "$0"; exit 0 ;;
      -*) printf 'unknown audit option: %s\n' "$arg" >&2; return 2 ;;
      *) AUDIT_TARGETS+=("$arg") ;;
    esac
  done
  if [ "${#AUDIT_TARGETS[@]}" -gt 0 ]; then
    [ "$selected" -eq 0 ] || { printf 'audit modes and explicit files cannot be mixed\n' >&2; return 2; }
    AUDIT_MODE=explicit
  fi
  case "$AUDIT_MODE" in
    staged|--staged) AUDIT_MODE=--staged ;;
    all|--all) AUDIT_MODE=--all ;;
    explicit) ;;
    *) printf 'unknown audit scope: %s\n' "$AUDIT_MODE" >&2; return 2 ;;
  esac
}

# Source and dependencies share a copied index. Git's object store is read-only;
# the index copy and checked-out files belong to this invocation alone.
audit_index_snapshot() {
  [ "$AUDIT_MODE" = --staged ] || return 0
  local gitdir index changed=0
  AUDIT_SNAPSHOT_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/orly-index.XXXXXX")" || return 1
  trap 'rm -rf -- "$AUDIT_SNAPSHOT_ROOT"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  gitdir="$(git rev-parse --absolute-git-dir)" || exit 1
  index="$(git rev-parse --git-path index)" || exit 1
  cp "$index" "$AUDIT_SNAPSHOT_ROOT/index" || exit 1
  export GIT_DIR="$gitdir" GIT_INDEX_FILE="$AUDIT_SNAPSHOT_ROOT/index" GIT_OPTIONAL_LOCKS=0
  if [ "$#" -gt 0 ]; then
    git diff --cached --quiet --diff-filter=ACMRT -- "$@" || changed=$?
    case "$changed" in
      0) return 0 ;;
      1) ;;
      *) printf 'cannot select staged audit files (git exit %s)\n' "$changed" >&2; exit 2 ;;
    esac
  fi
  export GIT_WORK_TREE="$AUDIT_SNAPSHOT_ROOT/tree"
  mkdir "$GIT_WORK_TREE" || exit 1
  git checkout-index --all --prefix="$GIT_WORK_TREE/" || exit 1
  cd "$GIT_WORK_TREE" || exit 1
  [ -z "${TARGET_ROOT:-}" ] || TARGET_ROOT="$GIT_WORK_TREE"
  audit_index_links || exit 2
}

audit_index_links() {
  local entry path
  git ls-files --stage -z > "$AUDIT_SNAPSHOT_ROOT/entries" || return 1
  while IFS= read -r -d '' entry; do
    case "$entry" in 120000*) ;; *) continue ;; esac
    path="${entry#*$'\t'}"
    bun -e 'import {realpathSync} from "node:fs";
      import {relative, isAbsolute} from "node:path";
      try {
        const p = relative(process.cwd(), realpathSync(process.argv[1]));
        if (p === ".." || p.startsWith("../") || isAbsolute(p)) throw new Error("escaping index link");
      } catch { console.error("unsafe indexed symbolic link: " + process.argv[1]); process.exit(2); }' "$path" || return 2
  done < "$AUDIT_SNAPSHOT_ROOT/entries"
}

# Zero-delimited throughout: spaces, tabs, quotes and newlines are filenames.
audit_scope_paths() {
  local path pattern matched
  if [ "$AUDIT_MODE" = explicit ]; then
    for path in "${AUDIT_TARGETS[@]}"; do
      matched=0
      for pattern in "$@"; do
        case "$path" in $pattern) matched=1 ;; esac
      done
      [ "$matched" -eq 0 ] || printf '%s\0' "$path"
    done
  elif [ "$AUDIT_MODE" = --staged ]; then
    git diff --cached --name-only -z --diff-filter=ACMRT -- "$@"
  else
    git ls-files --cached --others --exclude-standard -z -- "$@"
  fi
}
