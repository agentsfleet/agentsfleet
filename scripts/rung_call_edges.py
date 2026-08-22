#!/usr/bin/env python3
"""Which `@import` edges are CALLS, and which files call one named function.

Split out of `classify_rung_callers.py`, which classifies rungs on top of this.
The classifier answers "can this FILE leak"; everything here answers the two
questions underneath it, and both were learned the expensive way.

**An import taken for a type or a constant is not an edge.** `semconv.zig` does
`const Mode = @import("../state/tenant_provider.zig").Mode;` — one enum, never a
call — and reading that as reachability marked five `state/**` files long-lived
and cost three proofs. `import_graph` keeps an edge A→B only when A executes
something in B, and errs toward KEEPING: an unparsed `@import`, a
`usingnamespace`, or a reference to a type of B's that carries methods all keep
it, so the pruning can shrink a work list and never a risk set.

**A class label is per file; a proof is per function.** `callers_of` closes that
gap for one function by matching call sites directly, so a file that imports the
target for a type is absent from its answer by construction.
"""

from __future__ import annotations

import os
import re
from collections import defaultdict
from pathlib import Path
from typing import NamedTuple

IMPORT_RE = re.compile(r'@import\("([^"]+\.zig)"\)')

#: A `const NAME = @import("path.zig");` or `const NAME = @import("path.zig").Symbol;`
#: binding. Anything else holding an `@import` — inline, `usingnamespace`, a
#: multi-line expression — is left unparsed and keeps its edge.
BINDING_RE = re.compile(
    r'\bconst\s+(\w+)\s*=\s*@import\("([^"]+\.zig)"\)\s*(?:\.\s*(\w+))?\s*;'
)

#: Every function a file declares. Over-inclusive on purpose: a private `fn` in
#: the set can only KEEP an edge, and keeping is the safe direction.
FN_DECL_RE = re.compile(r"\bfn\s+(\w+)\s*\(")


def is_test_path(path: str) -> bool:
    base = os.path.basename(path)
    return (
        path.endswith("_test.zig")
        or "/test/" in path
        or base.startswith("test_")
        or base.endswith("test_support.zig")
        or "test_harness" in path
        or "test_fixtures" in path
    )


def zig_sources(src_root: str) -> list[str]:
    found = []
    for dirpath, _dirs, names in os.walk(src_root):
        for name in names:
            if name.endswith(".zig"):
                found.append(os.path.normpath(os.path.join(dirpath, name)))
    return sorted(found)


class TargetShape(NamedTuple):
    """What an imported file offers a caller: functions, and types with methods."""

    fns: frozenset[str]
    containers: frozenset[str]


def read_source(path: str) -> str:
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def declared_fns(text: str) -> set[str]:
    return set(FN_DECL_RE.findall(text))


#: Builtins that take a TYPE and run none of its code. A symbol used only as an
#: argument to one of these is a compile-time reference, not a call.
COMPTIME_ONLY_RE = (
    r"@(?:typeInfo|sizeOf|alignOf|bitSizeOf|typeName|hasDecl|hasField|field|"
    r"FieldType|unionInit|enumFromInt|intFromEnum|Type)\s*\(\s*{name}\b"
)

CONTAINER_RE = re.compile(
    r"\bconst\s+(\w+)\s*=\s*(?:extern\s+|packed\s+)?(?:struct|enum|union|opaque)\b[^{{]*\{"
)
BODY_FN_RE = re.compile(r"\bfn\s+\w+\s*\(")


def containers_with_fns(text: str) -> set[str]:
    """Container declarations that carry methods.

    A struct whose methods live in B runs B's code however the caller got hold
    of an instance, so an import taken for such a type is a real edge. A
    method-less enum or a plain constant is not.
    """
    found: set[str] = set()
    for match in CONTAINER_RE.finditer(text):
        depth, i = 1, match.end()
        while i < len(text) and depth:
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
            i += 1
        if BODY_FN_RE.search(text[match.end() : i]):
            found.add(match.group(1))
    return found


def comptime_only(text: str, name: str) -> bool:
    """Every mention of `name` outside its own binding is a comptime reference.

    `semconv.zig` takes `Mode` out of `tenant_provider.zig` for one
    `@typeInfo(Mode)`. `Mode` has a method, so the type carries code — but no
    value of it is ever built here, so none of that code runs. This is the only
    shape allowed to prune a methods-carrying type, because it is the only one
    where absence of a value is visible without following one.
    """
    body = re.sub(rf"\bconst\s+{re.escape(name)}\s*=\s*@import\([^;]*;", "", text)
    mentions = list(re.finditer(rf"\b{re.escape(name)}\b", body))
    if not mentions:
        return True
    spans = [
        (m.start(), m.end() + 200)
        for m in re.finditer(COMPTIME_ONLY_RE.format(name=re.escape(name)), body)
    ] + list(type_literal_spans(body))
    return all(any(lo <= m.start() < hi for lo, hi in spans) for m in mentions)


TYPE_LITERAL_RE = re.compile(r"\[[^\]\n]*\]\s*type\s*\{")


def type_literal_spans(text: str):
    """Spans of `[_]type{ ... }` literals — a list of types runs none of them."""
    for match in TYPE_LITERAL_RE.finditer(text):
        depth, i = 1, match.end()
        while i < len(text) and depth:
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
            i += 1
        yield (match.start(), i)


def executes(text: str, binding: str, symbol: str | None, target: TargetShape) -> bool:
    """Does `text` run code from the file bound to `binding`?

    A binding taken directly on one of the target's functions runs it. A binding
    taken on a type that carries methods runs them, however the instance was
    obtained — unless the type is only ever handed to a comptime builtin. A whole
    module binding runs code when something is called through it, when a member
    matching one of its functions is referenced, or when a member is a
    methods-carrying type this file re-exports.
    """
    if symbol is not None:
        if symbol in target.fns:
            return True
        if symbol in target.containers:
            return not comptime_only(text, binding)
        return False

    if re.search(rf"\b{re.escape(binding)}\b(?:\s*\.\s*\w+)*\s*\(", text):
        return True
    for member in re.findall(rf"\b{re.escape(binding)}\s*\.\s*(\w+)", text):
        if member in target.fns:
            return True
        if member in target.containers:
            alias = re.search(
                rf"\bconst\s+(\w+)\s*=\s*{re.escape(binding)}\s*\.\s*{re.escape(member)}\s*;",
                text,
            )
            if alias is None or not comptime_only(text, alias.group(1)):
                return True
    return False


def import_graph(files: list[str], pruned: list[tuple[str, str]] | None = None) -> dict[str, set[str]]:
    """Import edges, minus the ones taken for a type or a constant.

    Pass `pruned` to collect the dropped (importer, target) pairs — the audit
    trail for a change that shrinks the sweep's work list.
    """
    edges: dict[str, set[str]] = defaultdict(set)
    existing = set(files)
    shapes: dict[str, TargetShape] = {}
    dropped: set[tuple[str, str]] = set()

    def shape_of(path: str) -> TargetShape:
        if path not in shapes:
            body = read_source(path)
            shapes[path] = TargetShape(
                frozenset(declared_fns(body)), frozenset(containers_with_fns(body))
            )
        return shapes[path]

    for path in files:
        text = read_source(path)
        if not text:
            continue
        here = os.path.dirname(path)

        bindings: dict[str, list[tuple[str, str | None]]] = defaultdict(list)
        for name, rel, symbol in BINDING_RE.findall(text):
            bindings[os.path.normpath(os.path.join(here, rel))].append((name, symbol or None))

        occurrences: dict[str, int] = defaultdict(int)
        for match in IMPORT_RE.finditer(text):
            occurrences[os.path.normpath(os.path.join(here, match.group(1)))] += 1

        for target, count in occurrences.items():
            if target not in existing:
                continue
            bound = bindings.get(target, [])
            if count > len(bound):
                # At least one occurrence is not a `const NAME = @import(...)`:
                # inline `@import("x.zig").f()`, `usingnamespace`, a multi-line
                # binding. Keep the edge — over-keeping never hides a leak.
                edges[path].add(target)
                continue
            if any(executes(text, name, symbol, shape_of(target)) for name, symbol in bound):
                edges[path].add(target)
            elif (path, target) not in dropped:
                dropped.add((path, target))
                if pruned is not None:
                    pruned.append((path, target))
    return edges


def call_patterns(text: str, here: str, target: str, fn_name: str) -> list[str]:
    """Regexes matching a call to `fn_name` on `target`, from a file at `here`.

    Callers are found through the binding, so a file that merely imports the
    target for a type is absent from the answer by construction.
    """
    patterns = []
    for name, rel, symbol in BINDING_RE.findall(text):
        if os.path.normpath(os.path.join(here, rel)) != target:
            continue
        if symbol == fn_name:
            patterns.append(rf"\b{re.escape(name)}\s*\(")
        else:
            patterns.append(rf"\b{re.escape(name)}\s*\.\s*{re.escape(fn_name)}\s*\(")
    rel_any = os.path.relpath(target, here).replace(os.sep, "/")
    patterns.append(rf'@import\("{re.escape(rel_any)}"\)\s*\.\s*{re.escape(fn_name)}\s*\(')
    return patterns


def call_offsets(text: str, here: str, target: str, fn_name: str) -> list[int]:
    """Where in `text` `fn_name` is called on `target`."""
    return sorted(
        m.start()
        for pat in call_patterns(text, here, target, fn_name)
        for m in re.finditer(pat, text)
    )


def callers_of(files: list[str], target: str, fn_name: str) -> list[str]:
    """Every file that CALLS `fn_name` on `target`.

    The class label answers "can this FILE leak". A proof needs "can this
    FUNCTION leak", and the two diverge whenever a file mixes a worker-called
    function with a handler-called one — which is how three proofs in this
    milestone were written against arena-backed rungs.
    """
    hits = []
    for path in files:
        if path == target:
            continue
        text = read_source(path)
        if text and call_offsets(text, os.path.dirname(path), target, fn_name):
            hits.append(path)
    return hits


FN_SPAN_RE = re.compile(r"\bfn\s+(\w+)\s*\(")


def function_spans(text: str) -> list[tuple[str, int, int]]:
    """(name, start, end) for every function body, innermost resolvable by span.

    A call site alone does not say which function performs it, and that is the
    whole difference between "this FILE is reached from a worker" and "this
    FUNCTION is". Nested declarations are kept, so `enclosing_fn` can pick the
    innermost.
    """
    spans = []
    for match in FN_SPAN_RE.finditer(text):
        depth, i = 1, match.end()
        while i < len(text) and depth:  # past the argument list
            if text[i] == "(":
                depth += 1
            elif text[i] == ")":
                depth -= 1
            i += 1
        brace = text.find("{", i)
        semi = text.find(";", i)
        if brace < 0 or (0 <= semi < brace):
            continue  # a declaration with no body
        depth, j = 1, brace + 1
        while j < len(text) and depth:
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
            j += 1
        spans.append((match.group(1), match.start(), j))
    return spans


def enclosing_fn(spans: list[tuple[str, int, int]], offset: int) -> str | None:
    """The innermost function whose body contains `offset`."""
    best = None
    for name, start, end in spans:
        if start <= offset < end and (best is None or start > best[1]):
            best = (name, start)
    return best[0] if best else None
