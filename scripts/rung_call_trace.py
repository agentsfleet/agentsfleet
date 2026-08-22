#!/usr/bin/env python3
"""Resolve one function to the allocator its callers actually pass.

`callers_of` answers one hop, and one hop is not enough. It classifies each
caller by that caller's FILE class, so an inflated label propagates: six of the
eight leak-capable files in `state/**` funnel through `state/tenant_provider.zig`,
which is `repeating` for exactly one reason — `Mode` is an enum carrying a method,
held as a value, and the edge rule keeps that on purpose. One hop calls those six
leak-capable; the truth is that every path out of them ends in a handler.

This walks the (file, function) graph until each branch terminates in something
that decides the answer:

  a long-lived root   the function is executed by a loop that outlives requests
  the handler tree    the arena boundary; that branch cannot leak
  no caller at all    the trail broke — a method value, a function pointer, a
                      shape regex cannot follow. The walk does NOT call that
                      branch dead: it falls back to the caller file's own class
                      and marks the verdict degraded, because dropping a rung on
                      a lost trail is the one error direction that hides a leak.

The worst branch wins, and the witness chain for it is returned alongside, because
a verdict a proof author cannot check is the thing that cost this milestone three
proofs.
"""

from __future__ import annotations

import os
import re
from typing import NamedTuple

from rung_call_edges import (
    call_offsets,
    enclosing_fn,
    function_spans,
    intra_file_call_offsets,
    read_source,
)

CLASS_REPEATING = "repeating"
CLASS_BOOT_ONCE = "boot-once"
CLASS_ARENA = "arena"
CLASS_UNREACHED = "unreached"

#: Worst-first. A function reached both ways is as dangerous as its worst caller.
SEVERITY = (CLASS_REPEATING, CLASS_BOOT_ONCE, CLASS_ARENA, CLASS_UNREACHED)


class Site(NamedTuple):
    file: str
    fn: str

    def label(self, src_root: str) -> str:
        return f"{os.path.relpath(self.file, src_root)}:{self.fn}"


class Verdict(NamedTuple):
    cls: str
    chain: list[Site]
    #: True when some branch fell back to a FILE class because the call trail
    #: broke. The verdict is then no sharper than the old one-hop answer.
    degraded: bool = False


class Tree:
    """The files, indexed once, so a recursive walk does not re-read them."""

    def __init__(self, src_root: str, files: list[str], handler_root: str,
                 repeating_roots, boot_roots, file_class: dict[str, str]) -> None:
        self.src_root = src_root
        self.file_class = file_class
        self.files = files
        self.handler_root = handler_root
        self.repeating = frozenset(repeating_roots)
        self.boot = frozenset(boot_roots)
        self._text = {f: read_source(f) for f in files}
        self._spans: dict[str, list] = {}
        self._memo: dict[Site, Verdict] = {}

    def spans(self, path: str):
        if path not in self._spans:
            self._spans[path] = function_spans(self._text[path])
        return self._spans[path]

    def terminal(self, path: str) -> str | None:
        """The class a file decides on its own, or None to keep walking."""
        if path.startswith(self.handler_root + os.sep):
            return CLASS_ARENA
        if path in self.repeating:
            return CLASS_REPEATING
        if path in self.boot:
            return CLASS_BOOT_ONCE
        return None

    def callers(self, site: Site) -> list[Site]:
        found = []
        own_spans = self.spans(site.file)
        for offset in intra_file_call_offsets(
            self._text[site.file], site.fn, [s for s in own_spans if s[0] == site.fn]
        ):
            caller = enclosing_fn(own_spans, offset)
            if caller and caller != site.fn:
                found.append(Site(site.file, caller))
        for path in self.files:
            if path == site.file:
                continue
            text = self._text[path]
            if not text:
                continue
            offsets = call_offsets(text, os.path.dirname(path), site.file, site.fn)
            if not offsets:
                continue
            spans = self.spans(path)
            for offset in offsets:
                found.append(Site(path, enclosing_fn(spans, offset) or "<file scope>"))
        return sorted(set(found))

    def resolve(self, site: Site, stack: frozenset = frozenset()) -> Verdict:
        """The class of the allocator reaching `site`, with a witness chain."""
        decided = self.terminal(site.file)
        if decided is not None:
            return Verdict(decided, [site])
        if site in stack:
            return Verdict(CLASS_UNREACHED, [site])  # a cycle decides nothing
        if site in self._memo:
            return self._memo[site]

        callers = self.callers(site)
        if not callers:
            # Nothing calls the function being PROVEN: say so, and let the author
            # read the call sites. Nothing calls an intermediate hop: the trail
            # broke mid-walk, so inherit that file's class rather than drop the
            # branch — the one error direction that would hide a leak.
            if not stack:
                return Verdict(CLASS_UNREACHED, [site])
            return Verdict(self.file_class.get(site.file, CLASS_UNREACHED), [site], True)

        best = None
        for caller in callers:
            below = self.resolve(caller, stack | {site})
            candidate = Verdict(below.cls, below.chain + [site], below.degraded)
            if best is None or SEVERITY.index(candidate.cls) < SEVERITY.index(best.cls):
                best = candidate
            elif candidate.degraded and not best.degraded:
                best = best._replace(degraded=True)
            if best.cls == CLASS_REPEATING and not best.degraded:
                break
        self._memo[site] = best
        return best


ERRDEFER_LINE_RE = re.compile(r"^[ \t]*errdefer\b", re.M)


class Rungs(NamedTuple):
    """One function's rungs and the allocator that reaches them."""

    file: str
    fn: str
    rungs: int
    verdict: Verdict


def sweep(tree: Tree, paths: list[str]) -> list[Rungs]:
    """Every `errdefer` rung in `paths`, attributed to its function and resolved.

    The class label counts rungs per FILE, which is what made the work list
    overstate itself twice over. This counts them per function and answers each
    one, because a proof covers a function: 113 rungs across 61 functions is 61
    proofs, not 113.
    """
    out: dict[tuple[str, str], Rungs] = {}
    for path in paths:
        text = tree._text.get(path) or read_source(path)
        spans = function_spans(text)
        for match in ERRDEFER_LINE_RE.finditer(text):
            fn = enclosing_fn(spans, match.start())
            if fn is None:
                continue
            key = (path, fn)
            if key in out:
                out[key] = out[key]._replace(rungs=out[key].rungs + 1)
            else:
                out[key] = Rungs(path, fn, 1, tree.resolve(Site(path, fn)))
    return sorted(
        out.values(),
        key=lambda r: (SEVERITY.index(r.verdict.cls), r.verdict.degraded, -r.rungs, r.file),
    )


def render_sweep(rows: list[Rungs], src_root: str, leak_capable) -> None:
    """The work list a proof author picks from, worst first."""
    for row in rows:
        mark = " ⚠" if row.verdict.degraded else "  "
        print(
            f"{row.rungs:>4}  {row.verdict.cls:<10}{mark} "
            f"{os.path.relpath(row.file, src_root)}:{row.fn}"
        )
    clean = [r for r in rows if r.verdict.cls in leak_capable and not r.verdict.degraded]
    degraded = [r for r in rows if r.verdict.cls in leak_capable and r.verdict.degraded]
    rest = len(rows) - len(clean) - len(degraded)
    print(
        f"\nleak-capable, clean chain: {sum(r.rungs for r in clean)} rungs in "
        f"{len(clean)} functions — these are the proofs"
        f"\nleak-capable, degraded:    {sum(r.rungs for r in degraded)} rungs in "
        f"{len(degraded)} functions — a file-class fallback decided some hop"
        f"\narena or unresolved:       {sum(r.rungs for r in rows) - sum(r.rungs for r in clean) - sum(r.rungs for r in degraded)} rungs in {rest} functions"
    )
