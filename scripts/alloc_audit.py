#!/usr/bin/env python3
"""alloc_audit.py — find structs that hold heap-owning collections without
either an `alloc:` (or `allocator:`) field or a `///` doc-comment naming the
caller-owned-allocator pattern.

Heuristic only — best-effort, surfaces candidates for human review.

Output (TSV): file<TAB>struct_name<TAB>line<TAB>has_alloc_field<TAB>has_ownership_comment<TAB>heap_kinds
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

HEAP_PAT = re.compile(
    r"\b(ArrayList(?:Unmanaged)?|AutoHashMap(?:Unmanaged)?|StringHashMap(?:Unmanaged)?)\b"
)
ALLOC_FIELD_PAT = re.compile(
    r"^\s*(alloc|allocator|gpa|arena)\s*:\s*(std\.mem\.)?Allocator\b", re.MULTILINE
)
# Tokens that hint a struct has documented its allocator ownership convention,
# even when there isn't a literal `alloc:` field. Matches both ArrayList-style
# `.init(allocator)` calls (caller-owned) and explicit prose.
OWNERSHIP_HINT_PAT = re.compile(
    r"(allocator|caller-owned|caller owned|callers? must|owns the allocator|borrowed allocator)",
    re.IGNORECASE,
)


def find_struct_block_start(lines: list[str], idx: int) -> int | None:
    """Walk backward from idx to find the struct declaration line."""
    depth = 1
    for j in range(idx - 1, -1, -1):
        line = lines[j]
        depth -= line.count("}")
        depth += line.count("{")
        if depth >= 1:
            m = re.search(r"\bstruct\b", line)
            if m:
                return j
    return None


def name_for_struct(lines: list[str], decl_line: int) -> str:
    line = lines[decl_line]
    # Try "const Foo = struct" or "pub const Foo = struct"
    m = re.search(r"(?:pub\s+)?const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:packed\s+|extern\s+)?struct", line)
    if m:
        return m.group(1)
    # Inline / anonymous
    return f"<anon@{decl_line + 1}>"


def main() -> int:
    files = [l.strip() for l in sys.stdin if l.strip()]
    for file in files:
        path = Path(file)
        try:
            text = path.read_text()
        except FileNotFoundError:
            continue
        lines = text.splitlines()
        # Find every line that mentions a heap collection type as a field type.
        # Pattern: `name: <Allocator-aware container>(...)` or in field position.
        for i, line in enumerate(lines):
            stripped = line.strip()
            if not stripped or stripped.startswith("//"):
                continue
            if not HEAP_PAT.search(line):
                continue
            # Must be in a field-like position: contains `:` or starts with field decl
            if ":" not in stripped:
                continue
            # Skip method signatures / locals: rough heuristic — if the line ends with `{`, it's likely a fn
            if stripped.endswith("{") or stripped.startswith("fn ") or stripped.startswith("pub fn "):
                continue
            # Find enclosing struct
            decl = find_struct_block_start(lines, i)
            if decl is None:
                continue
            struct_name = name_for_struct(lines, decl)
            # Locate the closing `};` for this struct to bound search
            depth = 0
            end = len(lines) - 1
            for k in range(decl, len(lines)):
                depth += lines[k].count("{")
                depth -= lines[k].count("}")
                if depth == 0 and k > decl:
                    end = k
                    break
            block = "\n".join(lines[decl : end + 1])
            has_alloc = bool(ALLOC_FIELD_PAT.search(block))
            # Look at the 5 lines immediately above the heap field for an /// doc-comment
            ctx = "\n".join(lines[max(0, i - 5) : i])
            has_comment = bool(re.search(r"^\s*///.*", ctx, re.MULTILINE)) and bool(
                OWNERSHIP_HINT_PAT.search(ctx)
            )
            heap_kind = HEAP_PAT.search(line).group(1)
            print(
                f"{file}\t{struct_name}\t{i + 1}\t{int(has_alloc)}\t{int(has_comment)}\t{heap_kind}"
            )


if __name__ == "__main__":
    sys.exit(main())
