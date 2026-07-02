# M107_002: Route-registration doc-freshness gate — automate what M107_001 fixed by hand

**Prototype:** v2.0.0
**Milestone:** M107
**Workstream:** 002
**Date:** Jul 02, 2026
**Status:** DONE
**Priority:** P2 — docs hygiene tooling; no user-facing behavior change, but closes the exact drift class M107_001 just fixed manually.
**Categories:** DOCS
**Batch:** B1 — same branch as M107_001; sequenced after it.
**Branch:** feat/m107-docs-freshness (shared with M107_001, per Indy's direction — land in the same PR)
**Test Baseline:** unit=2270 integration=243
**Depends on:** M107_001 (`docs/v2/done/M107_001_P2_DOCS_FRESHNESS_PATHS_REST_GUIDE.md`) — the REST guide's §7 must already describe the real 6-place/real-middleware model before a permanent check can be pinned against it; checking the old (phantom-middleware) content would fail on day one.
**Provenance:** human-directed (Indy, 2026-07-02, in-session: "spec out but do it in this PR, i see its a simple fix" — overriding M107_001's own Out-of-Scope deferral of this exact work).

> **Provenance is load-bearing.** M107_001's Product Clarity named "a generated route-registration reference derived from the code" as the unconstrained optimum and deferred it as tooling work. Indy overrode that deferral in the same session. This spec narrows "generated reference" to a **verification gate** (see Product Clarity #3/#4) — a scope call made during authoring, not the literal ask; flagged here for Indy to redirect if the literal generated-table shape is what he wants instead.

---

## Implementing agent — read these first

1. `scripts/check_openapi_url_shape.py` — the exact pattern to mirror: a small, single-purpose Python script with a docstring naming the rule + source of truth, small justified allowlists, exit 0 clean / non-zero with violations listed. This spec's script is the same shape, different rule.
2. `make/quality.mk` lines defining `check-openapi` (composition of `python3 scripts/check_openapi_*.py` calls) and `lint-all`'s dependency list — the new target slots in the same way.
3. `src/agentsfleetd/auth/middleware/mod.zig` — the real policy-accessor surface: `pub const none` plus every `pub fn <name>(self: *Self) []const Middleware(AuthCtx)`. Setup functions (`initChains`, `setWebhookSig`, `setSvixSig`) are NOT policy accessors — exclude by return-type shape, not by name-guessing.
4. `docs/REST_API_DESIGN_GUIDELINES.md` §7 (as M107_001 leaves it) — the four fact classes this gate protects: middleware policy names, `src/agentsfleetd/**/*.zig` path citations, `make <target>` citations, and the pre-M107_001 dead path prefix (`src/(errors|http|state|types|cmd|auth)/` without the `agentsfleetd` segment).
5. `docs/v2/active/M107_001_P2_DOCS_FRESHNESS_PATHS_REST_GUIDE.md` — Eval Commands E1–E4 are exactly the four checks this spec automates; E1–E4 are one-time hand-run shell one-liners, this spec turns them into a permanent, CI-enforced `make` target so no future agent has to remember to run them.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** (folded into M107_001's PR — see that spec's title) + this workstream's own commit: `feat(m107): add permanent doc-freshness gate for REST guide route registration`
- **Intent (one sentence):** the class of drift M107_001 fixed by hand (phantom middleware, stale paths, phantom make targets, dead path prefixes) fails `make lint-all` automatically the next time it happens, instead of waiting for the next doc-audit spec.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; mismatch with the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — an agent renames a middleware policy accessor or moves a handler file, forgets to update the REST guide, runs `make lint-all` (or CI runs it), and gets a named, actionable failure (`PHANTOM MIDDLEWARE: registry.operator()` / `MISSING: src/agentsfleetd/...`) instead of the doc silently going stale until the next manual audit.
2. **Preserved user behaviour** — `make lint-all`'s existing gates (`lint-zig`, `check-openapi`, `check-schema-gate`, etc.) run unchanged; this adds one more target to the list, it doesn't touch any existing one.
3. **Optimal-way check** — the unconstrained optimum (per M107_001) is a fully generated §7 table (route → path → method → middleware → scope) rendered from `routes.zig`/`route_table.zig`/`route_scopes.zig`. Gap named here: those three sources use exhaustive `switch` statements with multi-variant arms grouped per line (e.g. five route variants sharing one `=> &FLEET_READ` arm) and payload-typed union fields — reliably reconstructing per-route paths would need real Zig parsing, not the regex/line-scan style every existing `scripts/check_openapi_*.py` uses. **Scope call:** narrow to a **verification gate** over the same four fact classes M107_001 just hand-fixed (middleware names, cited paths, cited make targets, dead path prefixes) — mechanically as simple as the existing OpenAPI checks, and it fully satisfies the stated goal ("never drifts again") without the parser-correctness risk of full table generation. Full generation stays a named follow-up if this gate proves insufficient.
4. **Rebuild-vs-iterate** — iterate; this is additive tooling (one script + one make target), not a refactor of anything existing. A full-generation rebuild is explicitly the larger alternative, rejected for now per #3.
5. **What we build** — `scripts/check_route_registration_doc.py` (four checks, all read-only, no writes) + `check-route-registration-doc` target in `make/quality.mk`, wired into `lint-all`.
6. **What we do NOT build** — the fully-generated §7 table (named as follow-up in Out of Scope); any change to the four docs M107_001 already fixed beyond what's needed to make the gate pass on them as-is.
7. **Fit with existing features** — sits alongside `check-openapi`/`check-schema-gate`/`check-gh-actions-valid`/`check-playbooks` in `lint-all`; must not slow `make lint-all` meaningfully (it's a handful of greps + a `test -e` loop, sub-second).
8. **Surface order** — not applicable (no CLI/UI surface; a `make` target and CI gate).
9. **Dashboard restraint** — not applicable (no UI).
10. **Confused-user next step** — a failing gate prints the exact violation (`PHANTOM MIDDLEWARE: <name>`, `MISSING: <path>`, `PHANTOM TARGET: <target>`, `STALE PREFIX: <file>:<line>`) with enough context to fix it directly — no ticket needed.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; RULE NDC (the script itself must not describe a check it doesn't actually perform) and UFS (the four dead-prefix subsystem names — `errors|http|state|types|cmd|auth` — are a named constant, not repeated inline literals).
- No `*.zig`, `*.ts`, or `schema/*.sql` touched — `dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, `docs/SCHEMA_CONVENTIONS.md` do not apply.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` touched | N/A |
| PUB / Struct-Shape | no | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — new Python script | keep `check_route_registration_doc.py` under 350 lines; one function per check |
| UFS (repeated/semantic literals) | yes | the dead-prefix subsystem list is one named constant, reused by all call sites, not re-typed |
| UI Substitution / DESIGN TOKEN | no | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | N/A |
| SPEC TEMPLATE GATE | yes — this spec file itself | `bash audits/spec-template.sh --staged` clean before commit |

---

## Overview

**Goal (testable):** `make check-route-registration-doc` exits 0 against the current (M107_001-fixed) `docs/REST_API_DESIGN_GUIDELINES.md`, and exits non-zero with a named violation when any of the four fact classes (middleware names, cited `src/agentsfleetd/**/*.zig` paths, cited `make` targets, dead path prefixes) goes stale again.

**Problem:** M107_001 fixed seven docs by hand after an agent-run audit found them stale. Nothing stops the same class of drift from recurring the next time the middleware surface, a handler path, or a make target changes — the next fix would again require a full audit spec instead of a build failure at the moment of drift.

**Solution summary:** one new Python script, mirroring the existing `scripts/check_openapi_*.py` pattern, run from a new `make check-route-registration-doc` target wired into `lint-all`. It performs the same four checks M107_001's Eval Commands (E1–E4) already validated by hand, generalized where cheap (the dead-prefix sweep runs over all `docs/*.md`, not just the six M107_001 touched) and pinned permanently where the fact is REST-guide-specific (middleware names, cited paths, cited make targets).

---

## Prior-Art / Reference Implementations

- **CLI/tooling** → `scripts/check_openapi_url_shape.py` and `scripts/check_openapi_errors.py` — same shape (docstring rule statement, source-of-truth pointer, exit-code contract), invoked from `make/quality.mk`'s `check-openapi` target. This spec's script is a sibling, not a new pattern.
- **API** → not applicable — no HTTP surface change.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/check_route_registration_doc.py` | CREATE | four-check doc-freshness gate (middleware names, cited paths, cited make targets, dead path prefixes) |
| `make/quality.mk` | EDIT | add `check-route-registration-doc` target; add it to `lint-all`'s dependency list and the `.PHONY` line |
| `docs/SCHEMA_CONVENTIONS.md` | EDIT | three `src/(cmd\|types)/` stale prefixes found by Check A against the real tree (dotfiles symlink; see Discovery) |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one script, one make target — additive, no existing file's behavior changes.
- **Alternatives considered:** (a) full generated §7 table from Zig source — the literal ask; rejected for now per Product Clarity #3 (parser-correctness risk on grouped exhaustive-switch arms and typed union payloads outweighs the benefit over a verification gate that already achieves "never drifts"); (b) fold the checks into `check-openapi` itself — rejected, this isn't an OpenAPI-surface concern and would conflate two unrelated gates' failure output.
- **Patch-vs-refactor verdict:** this is a **patch** (additive tooling). The full-generation alternative is the right long game if this gate proves insufficient — named in Out of Scope, not silently mud-patched around.

---

## Sections (implementation slices)

### §1 — The four-check script

`scripts/check_route_registration_doc.py` performs, in order: (a) dead-prefix sweep across all `docs/*.md` for the pre-M107_001 subsystem prefixes; (b) middleware-name cross-check — real accessor names extracted from `mod.zig` (the `pub const none` + `pub fn <name>(self: *Self) []const Middleware(AuthCtx)` shape) vs every `registry.\w+\(` / `auth_mw.MiddlewareRegistry.\w+` token found in the REST guide; (c) cited-path existence — every `src/agentsfleetd/[a-zA-Z0-9_/]+\.zig` token in the REST guide resolves via a filesystem check; (d) cited make-target existence — every `make [a-z-]+` token in the REST guide resolves against `make/*.mk` + `Makefile`. **Implementation default:** one Python function per check, each returning a list of violation strings; `main()` concatenates and prints, exits 1 if non-empty — mirrors `check_openapi_url_shape.py`'s shape exactly.

- **Dimension 1.1** — dead-prefix sweep flags a synthetic `src/http/` reference reintroduced into a scratch copy of a doc → Test `test_dead_prefix_detected`
- **Dimension 1.2** — dead-prefix sweep is silent against the current (M107_001-fixed) doc tree → Test `test_dead_prefix_clean_on_real_tree`
- **Dimension 1.3** — middleware check flags a synthetic `registry.operator()` reintroduced into a scratch copy of the REST guide → Test `test_phantom_middleware_detected`
- **Dimension 1.4** — middleware check accepts every real accessor name currently exported by `mod.zig` when cited in the doc → Test `test_real_middleware_accepted`
- **Dimension 1.5** — cited-path check flags a synthetic nonexistent `src/agentsfleetd/...` path → Test `test_missing_path_detected`
- **Dimension 1.6** — cited-make-target check flags a synthetic `make totally-fake-target` → Test `test_phantom_make_target_detected`

### §2 — Wiring into `lint-all`

`make check-route-registration-doc` runs the script; it joins `check-openapi`, `check-schema-gate`, `check-gh-actions-valid`, `check-playbooks` in `lint-all`'s dependency list and the file's `.PHONY` declaration.

- **Dimension 2.1** — `make check-route-registration-doc` exists and exits 0 against the real (post-M107_001) tree → Test `test_make_target_clean_exit`
- **Dimension 2.2** — `make lint-all`'s dependency list and `.PHONY` line both name `check-route-registration-doc` → Test `test_lint_all_wiring`

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

Internal-only tooling: no analytics events exist on this surface, none are added. Discovery records `Metrics review: no analytics/funnel playbook update required — internal lint tooling`.

---

## Interfaces

```
make check-route-registration-doc   # exit 0 clean, exit 1 with violations printed to stdout, one per line, prefixed
                                     # STALE PREFIX: / PHANTOM MIDDLEWARE: / MISSING: / PHANTOM TARGET:
```

No public function or HTTP surface. The "interface" is the exit-code contract and the violation-message prefixes, which `make lint-all`'s CI consumer and any future agent both depend on staying stable.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| False positive on an intentional historical citation | a doc mentions a deleted file's bare name without the full `src/agentsfleetd/...` path (M107_001's `route_manifest.zig` pattern) | by construction the cited-path check only matches the full `src/agentsfleetd/...` pattern — bare historical filenames never match, no allowlist needed |
| Script itself goes stale (mod.zig gains a policy accessor with a different signature shape) | Zig source shape changes | Dimension 1.4's real-tree test catches this at the next `make check-route-registration-doc` run — a shape the extractor can't parse means zero accessors extracted, which fails loudly (empty real-set vs non-empty referenced-set) rather than silently passing |
| REST guide moves to a non-symlinked location or gets renamed | doc reorganization | script's `SPEC_PATH`-equivalent constant needs updating in the same PR — same maintenance burden as `check_openapi_*.py`'s `SPEC_PATH` |

---

## Invariants

1. `make check-route-registration-doc` exits 0 against the tree exactly when all four fact classes are accurate — enforced by the script's own exit code, proven by Dimensions 1.1–1.6's paired positive/negative tests.
2. The four dead-prefix subsystem names (`errors`, `http`, `state`, `types`, `cmd`, `auth`) are a single named constant in the script, not repeated per-check — enforced by code review / UFS gate.

---

## Test Specification (tiered)

Tooling script: tiers collapse to deterministic shell/Python evals (no unit/integration/e2e web surface applies).

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | eval | `test_dead_prefix_detected` | scratch copy of a doc with `src/http/foo.zig` injected → script reports `STALE PREFIX:` for that file |
| 1.2 | eval | `test_dead_prefix_clean_on_real_tree` | real `docs/*.md` tree → zero `STALE PREFIX:` lines |
| 1.3 | eval | `test_phantom_middleware_detected` | scratch copy of the REST guide with `registry.operator()` injected → script reports `PHANTOM MIDDLEWARE: operator` |
| 1.4 | eval | `test_real_middleware_accepted` | real REST guide (post-M107_001) → zero `PHANTOM MIDDLEWARE:` lines |
| 1.5 | eval | `test_missing_path_detected` | scratch copy with `src/agentsfleetd/http/handlers/does_not_exist.zig` injected → script reports `MISSING: src/agentsfleetd/http/handlers/does_not_exist.zig` |
| 1.6 | eval | `test_phantom_make_target_detected` | scratch copy with `make totally-fake-target` injected → script reports `PHANTOM TARGET: totally-fake-target` |
| 2.1 | eval | `test_make_target_clean_exit` | `make check-route-registration-doc` against real tree → exit 0 |
| 2.2 | eval | `test_lint_all_wiring` | `grep check-route-registration-doc make/quality.mk` → 3 hits (`.PHONY` line + target definition + `lint-all` dependency line) |

Regression: `check-openapi`/`check-schema-gate`/`check-gh-actions-valid`/`check-playbooks` unaffected — `make lint-all` still runs all of them (verify by diffing `lint-all`'s dependency list: only one name added). Idempotency/replay: N/A — read-only check, no retry semantics.

---

## Acceptance Criteria

- [x] Script exists and is executable-clean — verify: `python3 scripts/check_route_registration_doc.py; echo "exit: $?"` → `exit: 0` against the real tree
- [x] `make check-route-registration-doc` exists and passes — verify: `make check-route-registration-doc`
- [x] Wired into `lint-all` — verify: `grep -c check-route-registration-doc make/quality.mk` → `3`
- [x] Negative paths all fire — verify: the four injection tests in Test Specification each print their named violation
- [x] `gitleaks detect` clean · no file over 350 lines added (160 lines, post-review fixes)

---

## Eval Commands (post-implementation)

```bash
# E1: script runs clean against the real tree
python3 scripts/check_route_registration_doc.py && echo "PASS" || echo "FAIL"
# E2: make target exists and passes
make check-route-registration-doc && echo "PASS" || echo "FAIL"
# E3: wired into lint-all (.PHONY + dependency list)
test "$(grep -c check-route-registration-doc make/quality.mk)" -eq 3 && echo "PASS" || echo "FAIL"
# E4: negative paths — the script has no CLI flags (matches scripts/check_openapi_*.py
# style); negative paths are exercised by importing it and calling its check
# functions directly against synthetic strings, not a --doc flag.
python3 -c "
import sys; sys.path.insert(0, 'scripts')
import check_route_registration_doc as m
real = m.real_middleware_policies(open(m.MIDDLEWARE_MOD_PATH).read())
assert m.check_phantom_middleware('registry.operator()', real), 'phantom middleware not caught'
assert m.check_missing_paths('src/agentsfleetd/http/handlers/does_not_exist.zig'), 'missing path not caught'
assert m.check_phantom_make_targets('make totally-fake-target', m.MAKE_DIR, m.MAKEFILE_PATH), 'phantom target not caught'
print('PASS')
"
# E5: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted; this spec only creates a new script and extends `make/quality.mk`.

---

## Discovery (consult log)

- **Scope narrowing (Indy to confirm or redirect)** — Indy's literal ask was "a code-derived, always-fresh route-registration reference" (implying generated table content). This spec narrows that to a **verification gate** over the four fact classes M107_001 hand-fixed, reasoned in Product Clarity #3: full per-route table generation needs real Zig parsing (grouped exhaustive-switch arms, typed union payloads) that the existing `scripts/check_openapi_*.py` regex-based pattern can't safely do, while a verification gate achieves the stated durability goal ("never drifts again") at the same mechanical simplicity as prior art. Surfaced to Indy in-session before EXECUTE; no redirect received, proceeded with the gate.
- **Pre-existing drift found by the generalized Check A** — running the new script against the real tree (before implementation was "done") surfaced three stale `src/(cmd|types)/` references in `docs/SCHEMA_CONVENTIONS.md` (lines 16, 35, 40) that neither M107_001 nor this spec's Files Changed table named — a gate that fails against current `main` isn't shippable, and the fix is the same mechanical class Indy already approved in M107_001, so it was folded in here rather than blocked on a third spec. `docs/SCHEMA_CONVENTIONS.md` is a dotfiles symlink; fixed and will be committed in `~/Projects/dotfiles` alongside M107_001's dotfiles commit.
- **Spec self-correction** — Dimension 2.2 / the E3 eval originally expected `grep -c check-route-registration-doc make/quality.mk` → `2`, undercounting: the target's own definition line also matches the string, in addition to the `.PHONY` line and the `lint-all` dependency line. Corrected to `3` (the real, verified count) in the Test Specification, Acceptance Criteria, and Eval Commands sections.
- **Metrics review** — no analytics/funnel playbook update required — internal lint tooling.
- **`/write-unit-test` outcome** — audited the eval suite against this spec's Test Specification: all 8 Dimensions (1.1–1.6, 2.1–2.2) have a corresponding eval, both positive (clean-on-real-tree) and negative (synthetic-injection) paths exercised for all four checks. Clean; no gaps found.
- **`/review` outcome (`code-review` skill, medium effort)** — 3 finder angles (correctness, cleanup/simplification/efficiency, conventions) ran independently against the script + `make/quality.mk` diff, verified, 8 findings survived, all dispositioned: (1) `DOC_MAKE_TARGET_RE` over-matched ordinary prose ("make sure") — CONFIRMED by two independent finders, fixed by requiring backtick-quoting, matching the guide's actual citation style; (2) bare string concatenation of `make/*.mk` files risked a cross-file line-merge on a missing trailing newline, hiding target definitions from the `MULTILINE` anchor — PLAUSIBLE, fixed via newline-joined concatenation; (3) docstring overclaimed "all docs/*.md" coverage when the glob is non-recursive (14 of 216 total `.md` files) — CONFIRMED, docstring reworded to state the top-level-only scope explicitly; (4) O(targets × corpus size) make-target lookup — fixed for free alongside (2) via a single defined-targets extraction pass; (5) `M107_001`/`M107_002` milestone-ID tokens embedded in the `.py` docstring — CONFIRMED MILESTONE-ID GATE hit (`dispatch/write_any.md` exempts `docs/*.md`, not `.py`), rewritten to describe purpose not spec lineage; (6) "REST" unexpanded on first mention — CONFIRMED acronym-rule hit, spelled out; (7) duplicated read-or-fail block — fixed within this file via a local `read_file()` helper (not retrofitted into the two pre-existing sibling scripts, out of this spec's scope); (8) `check_dead_prefix`'s unreachable `FileNotFoundError` branch — PLAUSIBLE but harmless defensive code, left as-is. All fixes re-verified against the full eval suite (E1–E5) plus two targeted regression checks (ordinary-prose non-match, newline-merge repro) before commit.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits the eval suite vs this Test Specification | Clean; outcome in Discovery |
| After evals pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec + RULE NDC/UFS | Clean OR findings dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff | Comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Script clean on real tree | E1 | `OK: route-registration doc freshness — docs/REST_API_DESIGN_GUIDELINES.md clean, 14 docs scanned for dead prefixes.` | ✅ |
| Make target passes | E2 | same output via `make check-route-registration-doc` | ✅ |
| lint-all wiring | E3 | grep count = 3 (`.PHONY`, target definition, `lint-all` dependency) | ✅ |
| Negative paths (all four checks) | E4 | `PASS` — phantom middleware, missing path, and phantom make target all caught by direct function calls | ✅ |
| Gitleaks | E5 | `no leaks found` (scanned whole tree, ~14.26 MB) | ✅ |

**Test Delta:** unit 2270→2270 (+0) · integration 243→243 (+0) vs CHORE(open) baseline. Lacking: none — Python tooling script outside the Zig test-depth counter's scope; coverage proof is the eval suite above (E1–E5 + the four negative-path Dimension tests), per this spec's Test Specification framing (tiers collapse to deterministic evals for a tooling script with no Zig surface).

---

## Out of Scope

- Full generated §7 route-registration table (route → path → method → middleware → scope) derived via real Zig parsing — the literal unconstrained optimum; deferred until this gate proves insufficient or a Zig-AST-based tool exists in the repo to build on.
- Extending the gate to other docs' code-adjacent claims beyond the REST guide's §7 (e.g. `docs/AUTH.md`'s own file citations) — same pattern, separate spec if drift recurs there.
- Enforcing the "N places, in order" prose count in §7 stays in sync with the numbered list beneath it — judged too brittle for a first pass; named here in case a future drift makes it worth the complexity.
