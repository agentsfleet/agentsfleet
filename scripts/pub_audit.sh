#!/usr/bin/env bash
# pub_audit.sh — list pub symbols in src/**/*.zig (non-test) that have
# zero external references outside their declaring file.
#
# Usage: scripts/pub_audit.sh [path]
#   path: file or directory to audit (default: src/)
#
# Output (TSV): file<TAB>kind<TAB>name<TAB>refs_outside_file
#   refs_outside_file == 0 → candidate for `pub` removal
#   refs_outside_file == "TEST_ONLY" → only referenced from sibling _test.zig (keep pub)
#
# Notes
# - Uses ripgrep word-match. Indirect references (re-exports, comptime aliases,
#   @field lookup) WILL be missed — the build is the final arbiter.
# - Skips `_test.zig` files entirely.

set -uo pipefail

ROOT="$(git rev-parse --show-toplevel)"
TARGET="${1:-${ROOT}/src}"

if [[ -d "$TARGET" ]]; then
  FILES=$(find "$TARGET" -name '*.zig' -not -name '*_test.zig' | sort)
else
  FILES="$TARGET"
fi

while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  # Extract pub declarations: pub const NAME / pub fn NAME / pub var NAME / pub threadlocal var NAME
  # Match identifier: [A-Za-z_][A-Za-z0-9_]*
  grep -nE '^[[:space:]]*pub[[:space:]]+(const|fn|var|threadlocal[[:space:]]+var)[[:space:]]+[A-Za-z_][A-Za-z0-9_]*' "$file" 2>/dev/null | \
  while IFS=: read -r lineno rest; do
    # Parse "pub <kind> NAME"
    sym=$(echo "$rest" | sed -E 's/^[[:space:]]*pub[[:space:]]+(const|fn|var|threadlocal[[:space:]]+var)[[:space:]]+([A-Za-z_][A-Za-z0-9_]*).*/\2/')
    kind=$(echo "$rest" | sed -E 's/^[[:space:]]*pub[[:space:]]+(const|fn|var|threadlocal[[:space:]]+var).*/\1/' | tr -d ' ')
    [[ -z "$sym" ]] && continue
    # Skip private convention (leading underscore is not enforced in Zig but treat as intentional)
    # Search for the symbol outside the declaring file across the repo (incl. tests, build.zig, *.json schemas, etc)
    declaring=$(realpath "$file")
    test_sibling="${file%.zig}_test.zig"
    # Count references outside declaring file
    refs=$(rg -n --no-heading -w "$sym" --type-add 'zig:*.zig' -tzig "$ROOT/src" "$ROOT/build.zig" 2>/dev/null | \
      awk -F: -v decl="$declaring" '{
        # canonicalize path
        cmd = "realpath " $1
        cmd | getline rp
        close(cmd)
        if (rp != decl) print rp
      }' | sort -u || true)
    refcount=$(echo -n "$refs" | grep -c . || true)
    if [[ "$refcount" -eq 0 ]]; then
      echo -e "${file}\t${kind}\t${sym}\t0"
    else
      # Check if all references are from sibling _test.zig
      non_test=$(echo "$refs" | grep -vE '_test\.zig$' || true)
      non_test_count=$(echo -n "$non_test" | grep -c . || true)
      if [[ "$non_test_count" -eq 0 ]]; then
        echo -e "${file}\t${kind}\t${sym}\tTEST_ONLY"
      fi
    fi
  done
done <<< "$FILES"
