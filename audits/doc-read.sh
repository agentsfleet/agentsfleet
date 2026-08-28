#!/usr/bin/env bash
# doc-read.sh — turn the 📖 DOC READ proof-line into a set comparison.
#
#   audits/doc-read.sh log <path>   record that a file was read (hook target)
#   audits/doc-read.sh check        staged source vs recorded reads
#
# The DOC READ GATE asks an agent to read the façade its edit triggers, and
# then asks the same agent whether it did. That is a claim about itself
# compared against nothing. `log` records the read and `check` compares the
# record against the façades the staged files trigger.
#
# NOTHING HERE MAY DEPEND ON AN AGENT RUNTIME. `log` is a shell command any
# agent can run; a runtime that also has a read hook (Claude Code today) can
# call it automatically, and a runtime that removes that feature tomorrow
# changes nothing — the agent runs the command, the record is identical, the
# gate behaves the same. Vendor-shaped configuration stays out of this file and
# out of .githooks/, and an eval fails the build if it appears
# (evals/ledger/run.sh, ledger_doc_read_runtime_neutral).
#
# The record lives in the git directory, never in the tree: it is evidence
# about one working copy, not shared history. When it is absent — an agent
# runtime with no hook support — `check` warns and exits 0. Partial
# mechanization stated honestly beats a red that means nothing.
#
# Exit: 0 clean or unenforceable · 1 unread trigger · 2 usage error.
set -uo pipefail

# A hook exports GIT_DIR/GIT_INDEX_FILE at the repository it fired in. This
# script IS a hook target, so the scope dies before the first git call.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_PREFIX \
      GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=audits/rule-ledger-lib.sh
source "$HERE/rule-ledger-lib.sh"

MODE_LOG="log"
MODE_CHECK="check"
LOG_RELATIVE_PATH="orly/doc-reads.jsonl"
USAGE="usage: $0 log <path> | $0 check"

if [[ -t 1 ]]; then G=$'\033[32m'; R=$'\033[31m'; Y=$'\033[33m'; X=$'\033[0m'
else G=''; R=''; Y=''; X=''; fi

# Where the record lives for this working copy. Empty when the caller is not
# inside a repository at all, which only `log` tolerates.
read_log_path() {
  local git_dir
  git_dir="$(git -C "$LEDGER_ROOT" rev-parse --absolute-git-dir 2>/dev/null)" || return 1
  printf '%s/%s' "$git_dir" "$LOG_RELATIVE_PATH"
}

# Repo-relative form of a path, or nothing when it sits outside the tree — an
# agent reading its own notes elsewhere on disk is not a doc read.
repo_relative() {
  local path="$1"
  case "$path" in
    "$LEDGER_ROOT"/*) printf '%s' "${path#"$LEDGER_ROOT"/}" ;;
    /*) return 1 ;;
    *) [ -e "$LEDGER_ROOT/$path" ] && printf '%s' "$path" ;;
  esac
}

# One JSON Lines row per call, appended. Append-only with a single write per
# invocation is why two agents in two sessions cannot corrupt each other: the
# order rows land in does not change which paths the set contains.
run_log() {
  local path="$1" relative log blob
  relative="$(repo_relative "$path")" || return 0
  [ -n "$relative" ] || return 0
  log="$(read_log_path)" || return 0
  mkdir -p "$(dirname "$log")" 2>/dev/null || return 0
  blob="$(content_hash "$relative")"
  # A hash we could not compute proves nothing was read at a known version.
  # Recording it would let two failures compare equal and validate a read of a
  # file that is no longer there.
  [ -n "$blob" ] || return 0
  printf '{"ts":%s,"path":"%s","blob":"%s"}\n' "$(date +%s)" "$(json_escape "$relative")" "$blob" >> "$log"
  return 0
}

# Backslash and double quote are legal in a POSIX filename and illegal raw in a
# JSON string. Unescaped, `we"ird.md` writes a row no parser can read — and this
# one mis-splits it, reporting a path that was never read.
json_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

# The content a read actually saw. Validity is keyed to this rather than to a
# clock: an unchanged document does not need re-reading because a commit
# happened, and a changed one voids every prior read the same second it is
# edited — which a timestamp window silently fails to do.
content_hash() {
  git -C "$LEDGER_ROOT" hash-object "$LEDGER_ROOT/$1" 2>/dev/null || printf ''
}

# Façade pages the staged files trigger, one per line. Scope comes from
# ledger_facade_scope — the page's own oracle-scope tag, else its leaf's
# dispatch_init — which is the SAME resolver the ledger's reachability report
# uses, so this check and the ledger can never disagree about what fires.
# Iterating pages rather than leaves is the point: a façade without a
# deterministic leaf still has a trigger surface, and used to be unaskable.
expected_facade_pages() {
  local staged="$1" page stem scope
  while IFS= read -r page; do
    [ -n "$page" ] || continue
    scope="$(ledger_facade_scope "$page")"
    [ -n "$scope" ] || continue
    stem="$(basename "$page" .md)"
    [ "$(ledger_match_count "$scope" < "$staged")" -gt 0 ] && printf 'dispatch/%s.md\n' "$stem"
  done < <(ledger_facade_pages)
}

json_unescape() {
  printf '%s' "$1" | sed -e 's/\\"/"/g' -e 's/\\\\/\\/g'
}

# Paths whose recorded read saw the content that is on disk now. A row written
# against an older version of the document proves the agent read something
# else, so it does not count — and rows from before this milestone, which carry
# no blob, cannot prove anything and are ignored.
current_reads() {
  local log="$1" line path blob
  while IFS= read -r line; do
    case "$line" in *'"blob":"'*) ;; *) continue ;; esac
    blob="${line##*\"blob\":\"}"; blob="${blob%%\"*}"
    [ -n "$blob" ] || continue
    path="${line#*\"path\":\"}"; path="${path%\",\"blob\":\"$blob\"\}}"
    path="$(json_unescape "$path")"
    [ "$blob" = "$(content_hash "$path")" ] && printf '%s\n' "$path"
  done < "$log"
}

# File-scope so the EXIT trap can still see them: a trap fires after a
# function's locals are gone, and `set -u` turns that into a spurious error on
# the way out of a clean run.
STAGED_FILE=""
EXPECTED_FILE=""
RECORDED_FILE=""
cleanup_temporaries() { rm -f "$STAGED_FILE" "$EXPECTED_FILE" "$RECORDED_FILE"; }

run_check() {
  local log unread=0 page staged expected recorded
  STAGED_FILE="$(mktemp)"; EXPECTED_FILE="$(mktemp)"; RECORDED_FILE="$(mktemp)"
  staged="$STAGED_FILE"; expected="$EXPECTED_FILE"; recorded="$RECORDED_FILE"
  trap cleanup_temporaries EXIT
  git -C "$LEDGER_ROOT" diff --cached --name-only --diff-filter=ACMRT > "$staged" 2>/dev/null
  expected_facade_pages "$staged" | sort -u > "$expected"

  if [ ! -s "$expected" ]; then
    printf '  %s🟢%s DOC READ: nothing staged triggers a façade\n' "$G" "$X"
    return 0
  fi
  if ! log="$(read_log_path)" || [ ! -f "$log" ]; then
    printf '  %s🟠%s DOC READ: no read record for this working copy — the agent runtime\n' "$Y" "$X"
    printf '     has no Read hook, so the proof-line stays self-reported here\n'
    return 0
  fi

  # Materialised once, and never piped into `grep -q`: under `pipefail` the
  # producer takes SIGPIPE the moment grep matches and exits, and the pipeline
  # then reports 141 — which reads exactly like "not found" and reds a doc that
  # was read.
  current_reads "$log" | sort -u > "$recorded"

  while IFS= read -r page; do
    grep -qxF "$page" "$recorded" && continue
    printf '  %s🔴%s DOC READ: %s triggered by the staged diff, not read at its current content\n' "$R" "$X" "$page"
    unread=1
  done < "$expected"
  [ "$unread" -eq 0 ] && printf '  %s🟢%s DOC READ: every triggered façade was read at its current content\n' "$G" "$X"
  return "$unread"
}

case "${1:-}" in
  "$MODE_LOG")
    [ "$#" -eq 2 ] || { printf '%s\n' "$USAGE" >&2; exit 2; }
    run_log "$2"
    ;;
  "$MODE_CHECK")
    [ "$#" -eq 1 ] || { printf '%s\n' "$USAGE" >&2; exit 2; }
    run_check
    ;;
  *) printf '%s\n' "$USAGE" >&2; exit 2 ;;
esac
