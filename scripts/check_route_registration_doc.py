#!/usr/bin/env python3
"""Permanent freshness gate for the Representational State Transfer (REST)
guide's route-registration facts (docs/REST_API_DESIGN_GUIDELINES.md §7).

An agent-run doc audit once found four classes of drift in that guide, all
hand-fixed: phantom middleware policy names, stale src/ path prefixes,
missing cited paths, and phantom make targets. Nothing stopped the same
drift from recurring the next time the middleware surface, a handler path,
or a make target changed. This script is the mechanical gate so those four
facts stay true without needing another audit pass.

Four checks, all read-only:
  A. Dead path prefix sweep — the pre-reorg daemon subsystem paths
     (src/errors/, src/http/, src/state/, src/types/, src/cmd/, src/auth/,
     src/fleet/) never come back; the real tree only has
     src/agentsfleetd/<subsystem>/ (src/fleet/ specifically split into
     src/agentsfleetd/fleet/ and src/agentsfleetd/fleet_runtime/).
     Runs across every top-level docs/*.md file (not recursive into
     docs/v2/**, docs/architecture/**, etc. — same boundary the original
     hand-fix covered).
  B. Phantom middleware — every `registry.<name>()` / `auth_mw.MiddlewareRegistry.<name>`
     token cited in the REST guide must be a real policy accessor exported by
     src/agentsfleetd/auth/middleware/mod.zig (a `pub const none`, or a
     `pub fn <name>(self: *Self) []const Middleware(AuthCtx)` — that return-type
     shape is what separates a policy accessor from a setup function like
     `initChains`/`setWebhookSig`, which this check correctly excludes).
  C. Cited path existence — every `src/agentsfleetd/**/*.zig` path cited in the
     REST guide must exist on disk.
  D. Phantom make targets — every backtick-quoted `` `make <target>` `` cited
     in the REST guide must resolve to a real target in make/*.mk or Makefile.

Exit 0 if clean, non-zero with each violation listed.
"""
import glob
import os
import re
import sys

DOCS_GLOB = "docs/*.md"
REST_GUIDE_PATH = "docs/REST_API_DESIGN_GUIDELINES.md"
MIDDLEWARE_MOD_PATH = "src/agentsfleetd/auth/middleware/mod.zig"
MAKE_DIR = "make"
MAKEFILE_PATH = "Makefile"

# The daemon subsystems that moved under src/agentsfleetd/ — this exact prefix
# shape (subsystem directly under src/, no agentsfleetd/ segment) can never be
# correct again. Named once here per UFS — every check reuses this constant.
DEAD_PREFIX_RE = re.compile(r"src/(errors|http|state|types|cmd|auth|fleet)/")

MIDDLEWARE_ACCESSOR_RE = re.compile(
    r"pub fn (\w+)\(self: \*Self\) \[\]const Middleware\(AuthCtx\)"
)
MIDDLEWARE_NONE_RE = re.compile(r"pub const none\s*:")
DOC_MIDDLEWARE_REF_RE = re.compile(r"registry\.(\w+)\(|auth_mw\.MiddlewareRegistry\.(\w+)")
DOC_ZIG_PATH_RE = re.compile(r"src/agentsfleetd/[a-zA-Z0-9_/]+\.zig")
# Backtick-quoted only — the guide always cites commands as `` `make foo` ``.
# An unquoted `r"make ([a-z-]+)"` also matches ordinary prose ("make sure",
# "make it clear"), which would false-positive the moment such a sentence is
# added to the guide.
DOC_MAKE_TARGET_RE = re.compile(r"`make (_?[a-z][a-z0-9_-]*)`")
# Real make targets are defined as `name:` or `_name:` at column 0 across
# make/*.mk + Makefile. Extracted once into a set so each cited target is an
# O(1) membership check instead of a fresh regex scan of the whole corpus.
#
# The leading `_` sits INSIDE the capture group, and underscore is in the inner
# class. Both matter: with `^_?([a-z]...)` the target `_fmt:` registered under
# the name `fmt`, so a doc citing `make fmt` — a target that does not exist —
# passed, while `make _fmt` was reported phantom. And without `_` in the inner
# class, `_lint_zig_test_depth:` matched nothing at all.
MAKE_TARGET_DEF_RE = re.compile(r"^(_?[a-z][a-z0-9_-]*):", re.MULTILINE)


def read_file(path: str) -> str | None:
    try:
        with open(path) as f:
            return f.read()
    except FileNotFoundError:
        return None


def check_dead_prefix(doc_paths: list[str]) -> list[str]:
    violations = []
    for path in doc_paths:
        text = read_file(path)
        if text is None:
            continue
        for lineno, line in enumerate(text.splitlines(), start=1):
            if DEAD_PREFIX_RE.search(line):
                violations.append(f"STALE PREFIX: {path}:{lineno}: {line.strip()}")
    return violations


def real_middleware_policies(mod_zig_text: str) -> set[str]:
    policies = set(MIDDLEWARE_ACCESSOR_RE.findall(mod_zig_text))
    if MIDDLEWARE_NONE_RE.search(mod_zig_text):
        policies.add("none")
    return policies


def check_phantom_middleware(doc_text: str, real_policies: set[str]) -> list[str]:
    referenced = {a or b for a, b in DOC_MIDDLEWARE_REF_RE.findall(doc_text)}
    return [f"PHANTOM MIDDLEWARE: {name}" for name in sorted(referenced - real_policies)]


def check_missing_paths(doc_text: str) -> list[str]:
    cited = sorted(set(DOC_ZIG_PATH_RE.findall(doc_text)))
    return [f"MISSING: {p}" for p in cited if not os.path.exists(p)]


def real_make_targets(make_dir: str, makefile_path: str) -> set[str]:
    # Joined with newlines (not bare concatenation) so a file missing its
    # trailing newline can't merge with the next file's first line and hide
    # that file's target definitions from the MULTILINE `^` anchor.
    chunks = []
    for f in sorted(glob.glob(os.path.join(make_dir, "*.mk"))):
        text = read_file(f)
        if text is not None:
            chunks.append(text)
    makefile_text = read_file(makefile_path)
    if makefile_text is not None:
        chunks.append(makefile_text)
    return set(MAKE_TARGET_DEF_RE.findall("\n".join(chunks)))


def check_phantom_make_targets(doc_text: str, make_dir: str, makefile_path: str) -> list[str]:
    cited = sorted(set(DOC_MAKE_TARGET_RE.findall(doc_text)))
    defined = real_make_targets(make_dir, makefile_path)
    return [f"PHANTOM TARGET: {t}" for t in cited if t not in defined]


def main() -> int:
    doc_paths = glob.glob(DOCS_GLOB)
    violations = check_dead_prefix(doc_paths)

    doc_text = read_file(REST_GUIDE_PATH)
    if doc_text is None:
        print(f"FAIL: {REST_GUIDE_PATH} not found", file=sys.stderr)
        return 1

    mod_text = read_file(MIDDLEWARE_MOD_PATH)
    if mod_text is None:
        print(f"FAIL: {MIDDLEWARE_MOD_PATH} not found", file=sys.stderr)
        return 1

    real_policies = real_middleware_policies(mod_text)
    violations += check_phantom_middleware(doc_text, real_policies)
    violations += check_missing_paths(doc_text)
    violations += check_phantom_make_targets(doc_text, MAKE_DIR, MAKEFILE_PATH)

    if violations:
        print(
            "Route-registration doc-freshness violation(s) — "
            f"see {REST_GUIDE_PATH} §7.\n",
            file=sys.stderr,
        )
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        return 1

    print(
        f"OK: route-registration doc freshness — {REST_GUIDE_PATH} clean, "
        f"{len(doc_paths)} top-level docs scanned for dead prefixes."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
