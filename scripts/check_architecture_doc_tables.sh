#!/usr/bin/env bash
# Table assertions for check_architecture_doc.sh. Sourced by it, never run
# alone: it reads that script's `err`, `ok`, `FAIL` and `ARCH_DIR`.
#
#   * architecture_tool_table_matches_catalog — runner_execution.md's Tool
#     catalog table names exactly the tools afr_tools publishes
#
# TOOL_CATALOG is overridable so the self-tests can point the check at a
# fixture catalog. Nothing else sets it.

TOOL_CATALOG="${TOOL_CATALOG:-rustd/crates/afr_tools/src/catalog.rs}"
readonly TOOL_TABLE_PAGE="$ARCH_DIR/runner_execution.md"
readonly TOOL_TABLE_HEADING='## Tool catalog'

# Every name in `PUBLISHED`, resolved through the `Entry::new("<name>", …)`
# constant each member names, one per line, sorted.
published_tool_names() {
  awk '
    match($0, /^pub const [A-Z0-9_]+: Entry = Entry::new\("[a-z0-9_]+"/) {
      line = substr($0, RSTART, RLENGTH)
      split(line, words, " ")
      constant = words[3]; sub(/:$/, "", constant)
      name = line; sub(/.*Entry::new\("/, "", name); sub(/"$/, "", name)
      entry[constant] = name
    }
    /^pub const PUBLISHED/ { listing = 1; next }
    listing && /^\];/ { listing = 0 }
    listing && match($0, /&[A-Z0-9_]+/) {
      member = substr($0, RSTART + 1, RLENGTH - 1)
      print (member in entry) ? entry[member] : "<unresolved " member ">"
    }
  ' "$1" | sort
}

# Every backticked name in the first cell of the table under the heading.
documented_tool_names() {
  awk -v heading="$TOOL_TABLE_HEADING" '
    $0 == heading { inside = 1; next }
    inside && /^## / { inside = 0 }
    inside && /^\| `/ {
      split($0, cells, "|")
      cell = cells[2]
      while (match(cell, /`[^`]+`/)) {
        print substr(cell, RSTART + 1, RLENGTH - 2)
        cell = substr(cell, RSTART + RLENGTH)
      }
    }
  ' "$1" | sort
}

if [ -f "$TOOL_TABLE_PAGE" ]; then
  tools_published="$(published_tool_names "$TOOL_CATALOG")"
  tools_documented="$(documented_tool_names "$TOOL_TABLE_PAGE")"
  if [ -z "$tools_published" ] || [ -z "$tools_documented" ]; then
    err "architecture_tool_table_matches_catalog: read no tools from $TOOL_CATALOG or $TOOL_TABLE_PAGE — the scan is broken, not the tree"
  elif [ "$tools_published" != "$tools_documented" ]; then
    err "architecture_tool_table_matches_catalog: $TOOL_TABLE_PAGE's Tool catalog table and $TOOL_CATALOG's PUBLISHED disagree (< published only, > documented only):"
    diff <(printf '%s\n' "$tools_published") <(printf '%s\n' "$tools_documented") | grep '^[<>]' >&2 || true
  else
    ok "architecture_tool_table_matches_catalog: the Tool catalog table names exactly the published tools"
  fi
fi
