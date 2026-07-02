# M107_001: Docs freshness — stale `src/` paths + REST guide refresh

**Prototype:** v2.0.0
**Milestone:** M107
**Workstream:** 001
**Date:** Jul 02, 2026
**Status:** DONE
**Priority:** P2 — docs hygiene; no user-facing behavior changes, but every stale pointer taxes the next implementing agent with a dead-path detour.
**Categories:** DOCS
**Batch:** B1 — standalone; no code dependency.
**Branch:** feat/m107-docs-freshness
**Test Baseline:** unit=2270 integration=243
**Depends on:** M106_001 (the doc audit that inventoried this drift ran there; its M106-scoped fixes landed in PR #468 — this spec is the agreed carve-out for the pre-existing remainder).
**Provenance:** agent-generated (pre-spec; M106 CHORE(close) doc-freshness audit, Jul 02, 2026 — three parallel audit agents cross-checked all 28 `docs/*.md` + `docs/architecture/*.md` files against the code).

> **Provenance is load-bearing.** Every stale reference below was verified against the tree on Jul 02, 2026 (paths resolved with `ls`/`grep`, middleware surface read from `src/agentsfleetd/auth/middleware/mod.zig`, make targets from `make/*.mk`). Line numbers cited are approximate anchors — re-locate by quoted text, not line.

**Canonical architecture:** not applicable — this spec corrects reference documentation to match shipped code; it introduces no new architecture. The source of truth per claim is the named code file.

---

## Implementing agent — read these first

1. `src/agentsfleetd/auth/middleware/mod.zig` — the ACTUAL middleware registry surface (`none`, `bearer()`, `runnerBearer()`, `webhookHmac()`, `webhookSig()`, `svix()`); the `bearer()` docstring explains why admin/operator chains were replaced by the route→scope table.
2. `src/agentsfleetd/http/route_scopes.zig` — `requiredScopes()` is the compile-enforced 6th route-registration site the REST guide predates; read the exhaustive-switch shape before rewriting §7.
3. `docs/REST_API_DESIGN_GUIDELINES.md` §7 — the section being refreshed; keep its voice and table style, replace its facts.
4. `src/agentsfleetd/http/route_table.zig` + `route_matchers_connectors.zig` + `route_table_invoke_connectors.zig` — the split-file registration reality (matchers/invoke shims now per-domain), including M106's two `none`-policy raw handlers.
5. `docs/EXECUTE_DOC_READS.md` — the trigger→doc map that must stay consistent with whatever §7 ends up saying ("5-place" → "6-place").

---

## PR Intent & comprehension handshake

- **PR title (eventual):** docs: fix stale src/ paths and refresh the REST route-registration guide
- **Intent (one sentence):** an implementing agent following any repo doc reaches real files and true registration rules on the first try — no dead `src/http/…` paths, no phantom middleware, no missing registration site.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; mismatch with the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — an agent (or Kishore) opens `REST_API_DESIGN_GUIDELINES.md` to add a route, follows §7 verbatim, and the route compiles with the correct scope on the first pass — no "that file doesn't exist" detour.
2. **Preserved user behaviour** — every rule that is still true stays word-for-word; this is a facts refresh, not a rewrite. Doc structure, section numbering, and voice are preserved.
3. **Optimal-way check** — the unconstrained optimum is a generated route-registration reference derived from the code (never drifts). Gap accepted: generation is tooling work out of P2 scope; the refreshed prose plus grep-able eval commands is the right size now.
4. **Rebuild-vs-iterate** — iterate. A doc-generation pipeline is a refactor of how docs are produced; nothing about this drift justifies it yet. Named as future work in Out of Scope.
5. **What we build** — corrected path prefixes in six docs; a §7 rewrite of the REST guide's middleware/registration model; corrected make-gate names; the two M106 raw handlers added to §7's exception table.
6. **What we do NOT build** — doc generation tooling (out of P2 scope); prose-style changes to untouched sections (churn without value); fixes to `docs/architecture/` (already reconciled in PR #468).
7. **Fit with existing features** — compounds with `docs/EXECUTE_DOC_READS.md` (the doc-read dispatch depends on these files being trustworthy); must not destabilize the SPEC TEMPLATE gate (this spec's own edits to `docs/TEMPLATE.md` are path-prefix-only).
8. **Surface order** — not applicable (docs only; no CLI/UI surface).
9. **Dashboard restraint** — not applicable (no UI).
10. **Confused-user next step** — a reader who suspects a doc path is stale runs the E-commands in this spec (grep sweeps); each returns zero hits when the doc is honest.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; specifically RULE NDC (the refreshed §7 must not describe dead code as alive) and RULE NLR (touch-it-fix-it: while editing a doc for paths, fix any co-located falsehood in the same pass rather than leaving a half-honest file).
- No code-surface rule files apply — the diff touches only `docs/*.md`.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` touched | N/A |
| PUB / Struct-Shape | no | N/A |
| File & Function Length (≤350/≤50/≤70) | no — `.md` exempt | N/A |
| UFS (repeated/semantic literals) | no — docs only | N/A |
| UI Substitution / DESIGN TOKEN | no | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | N/A |
| SPEC TEMPLATE GATE | yes — this spec file itself | `bash audits/spec-template.sh --staged` clean before commit |

---

## Overview

**Goal (testable):** after this PR, `grep -nE "src/(errors|http|state|types|cmd|auth)/" docs/*.md` returns zero hits outside code blocks that intentionally show historical paths, and every middleware name, registration site, file path, and make target named in `docs/REST_API_DESIGN_GUIDELINES.md` §7 resolves against the tree.

**Problem:** the daemon tree moved under `src/agentsfleetd/` and the auth middleware collapsed to a scope table, but seven docs still cite the old world. An agent reading them burns a detour per dead pointer, and the REST guide's §7 actively teaches a registration model (`registry.admin()`/`operator()`/`slack()`, "5 places") that no longer compiles.

**Solution summary:** one docs-only PR in two motions — (a) mechanical path-prefix sweep across six docs, (b) a §7-scoped refresh of the REST guide that describes the real middleware surface, the six registration sites, the split matcher/invoke files, the current raw-handler exception table (including M106's two `none`-policy handlers), and the real make gates.

---

## Prior-Art / Reference Implementations

- The M106 PR #468 docs commits (`c74e35af`, `e2299970`) are the pattern to mirror: code-verified claims, minimal diffs that keep the doc's voice, every corrected fact anchored to a named code file.
- The REST guide's own §7 table style is retained — replace rows, not the table shape.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | §7 refresh: middleware model, 6-place registration, raw-handler table, make gates, path prefixes; §3/§8 fabricated code fixed after adversarial fact-check proved them false (see Discovery) |
| `docs/EXECUTE_DOC_READS.md` | EDIT | "5-place" → "6-place" descriptor; trigger glob `src/http/handlers/**` → `src/agentsfleetd/http/handlers/**` |
| `docs/AUTH_DEVICE_LOGIN.md` | EDIT | two `src/…` path prefixes → `src/agentsfleetd/…` |
| `docs/CHANGELOG_VOICE.md` | EDIT | rate-constant pin path → `src/agentsfleetd/state/tenant_billing.zig` |
| `docs/TEMPLATE.md` | EDIT | two rule-reference globs → `src/agentsfleetd/http/handlers/**` |
| `docs/VERIFY_TIERS.md` | EDIT | tier-2 trigger globs → real paths (`src/agent/` never existed; use `src/agentsfleetd/fleet/…`) |
| `docs/SKILL_FRONTMATTER_SCHEMA.md` | EDIT | "See also" paths → `src/agentsfleetd/fleet_runtime/{config_parser,yaml_frontmatter}.zig` |

> `docs/CHANGELOG_VOICE.md` is a dotfiles symlink — per the symlinked-dotfiles rule, that edit commits in `~/Projects/dotfiles` (verify with `readlink` before editing; same for `TEMPLATE.md`/`VERIFY_TIERS.md`/`EXECUTE_DOC_READS.md` if they resolve there).

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two motions in one workstream — a mechanical prefix sweep (§1) and a judgment-bearing §7 refresh (§2, §3) — because they share one review context (doc honesty) and one eval harness (grep sweeps).
- **Alternatives considered:** (a) fold into PR #468 — rejected by Indy (keeps the feature PR reviewable; this drift pre-dates M106); (b) generate the route-registration doc from code — right long game, wrong size for P2 docs hygiene; named in Out of Scope.
- **Patch-vs-refactor verdict:** this is a **patch** — the docs' structure is sound; only their facts drifted.

---

## Sections (implementation slices)

### §1 — Path-prefix sweep (mechanical)

Every doc pointer resolves against the tree. The daemon subsystems (`errors`, `http`, `state`, `types`, `cmd`, `auth`) live only under `src/agentsfleetd/`; `src/lib/` and `src/runner/` remain top-level and must NOT be rewritten. `SKILL_FRONTMATTER_SCHEMA.md` additionally renames `fleet/` → `fleet_runtime/`. Respect the dotfiles-symlink rule per file.

- **Dimension 1.1** — six docs' `src/…` references resolve (`AUTH_DEVICE_LOGIN`, `CHANGELOG_VOICE`, `TEMPLATE`, `VERIFY_TIERS`, `EXECUTE_DOC_READS`, `SKILL_FRONTMATTER_SCHEMA`) → Test `eval_no_stale_prefix`
- **Dimension 1.2** — no over-rewrite: `src/lib/` and `src/runner/` citations remain untouched → Test `eval_toplevel_paths_intact`

### §2 — REST guide §7: middleware + registration model

§7 teaches the real auth surface: the middleware registry exposes exactly `none`, `bearer()`, `runnerBearer()`, `webhookHmac()`, `webhookSig()`, `svix()`; admin/operator capability is data in the route→scope table, not middleware; route registration is six compile-enforced places including `route_scopes.zig::requiredScopes()` (exhaustive switch — a new route variant fails to compile until assigned a scope); matchers and invoke shims are split per-domain (`route_matchers_*.zig`, `route_table_invoke_*.zig`).

- **Dimension 2.1** — §7's middleware policy table lists only registry members that exist in `auth/middleware/mod.zig` → Test `eval_no_phantom_middleware`
- **Dimension 2.2** — §7 (and `EXECUTE_DOC_READS.md`) says six registration places and names `route_scopes.zig` as one → Test `eval_six_places`

### §3 — REST guide §7: exception table + gates

The raw-handler exception table names files that exist (GitHub callback at `handlers/connectors/github/callback.zig`; `webhooks.zig` is now the `handlers/webhooks/` directory; `agent_relay.zig` and `runs/stream.zig` rows removed; streaming rows point at `handlers/fleets/{events_stream,create_stream}.zig`; auth sessions at `handlers/auth/sessions.zig`) and gains M106's two `none`-policy handlers (Slack OAuth callback, Slack events ingress — cross-reference `docs/AUTH.md` §OAuth connectors rather than re-explaining). Make-gate names match `make/*.mk`: `make check-openapi` (absorbing the folded error-schema check); the nonexistent `make test-auth` reference is removed or repointed at the real target the guide means.

- **Dimension 3.1** — every file path in §7's exception + reference tables passes `test -e` → Test `eval_exception_paths_exist`
- **Dimension 3.2** — every `make <target>` named in the guide exists in `make/*.mk` → Test `eval_make_targets_exist`

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

Internal-only documentation cleanup: no analytics events exist on this surface, none are added, and no funnel changes. Discovery records `Metrics review: no analytics/funnel playbook update required — docs-only diff`.

---

## Interfaces

Not applicable — no public function, endpoint, or data shape changes. The "interface" being corrected is prose; its lock is the eval-command suite below.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Over-rewrite of a valid path | sweep regex catches `src/lib/`/`src/runner/` | Dimension 1.2 eval fails; reviewer sees a diff hunk on a line that was already correct |
| Corrected path itself wrong | typo or moved-again file | `eval_exception_paths_exist` / `eval_no_stale_prefix` return hits; PR blocked |
| Symlinked doc committed in the wrong repo | dotfiles symlink edited in product repo | `git status` in `~/Projects/dotfiles` shows drift; the symlink rule's post-edit commit step catches it |
| §7 rewrite contradicts `AUTH.md` | two docs describing one surface diverge | cross-reference instead of restating; reviewer greps both for the claim |

---

## Invariants

1. Zero stale daemon-subsystem prefixes in the touched docs — enforced by `eval_no_stale_prefix` (grep, exit-code checked in Acceptance Criteria).
2. Every path §7 names exists on disk — enforced by `eval_exception_paths_exist` (`test -e` loop).
3. Every make target the REST guide names exists — enforced by `eval_make_targets_exist` (grep over `make/*.mk` + `Makefile`).

---

## Test Specification (tiered)

Docs-only diff: the tiers collapse to deterministic shell evals (no unit/integration/e2e code applies; `/write-unit-test` scope is the eval suite).

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | eval | `eval_no_stale_prefix` | grep for `src/(errors\|http\|state\|types\|cmd\|auth)/` across the six touched docs → 0 hits |
| 1.2 | eval | `eval_toplevel_paths_intact` | `src/lib/` + `src/runner/` citations in touched docs unchanged vs origin/main → identical count |
| 2.1 | eval | `eval_no_phantom_middleware` | grep §7 for `registry.admin\|registry.operator\|registry.slack` → 0 hits; every named registry member greps in `auth/middleware/mod.zig` |
| 2.2 | eval | `eval_six_places` | REST guide + EXECUTE_DOC_READS both say six places and name `route_scopes.zig` → 2 hits |
| 3.1 | eval | `eval_exception_paths_exist` | every path extracted from §7 tables `test -e` → all pass |
| 3.2 | eval | `eval_make_targets_exist` | every `make <target>` in the guide resolves in `make/*.mk`/`Makefile` → all found |

Regression: untouched doc sections byte-identical (review the diff hunks — only path/fact lines change). Idempotency/replay: N/A — no retry semantics.

---

## Acceptance Criteria

- [x] Zero stale daemon prefixes — verify: `grep -rnE "src/(errors|http|state|types|cmd|auth)/" docs/AUTH_DEVICE_LOGIN.md docs/CHANGELOG_VOICE.md docs/TEMPLATE.md docs/VERIFY_TIERS.md docs/EXECUTE_DOC_READS.md docs/SKILL_FRONTMATTER_SCHEMA.md docs/REST_API_DESIGN_GUIDELINES.md; test $? -eq 1`
- [x] No phantom middleware — verify: `grep -nE "registry\.(admin|operator|slack)\(" docs/REST_API_DESIGN_GUIDELINES.md; test $? -eq 1`
- [x] §7 paths exist — verify: the E3 loop below prints no `MISSING:` lines
- [x] Make targets real — verify: `grep -oE "make [a-z-]+" docs/REST_API_DESIGN_GUIDELINES.md | sort -u | while read -r _ t; do grep -qrE "^${t}:|^_${t}:" make/ Makefile || echo "PHANTOM: $t"; done` prints nothing
- [x] `gitleaks detect` clean · dotfiles-symlinked docs committed in `~/Projects/dotfiles` with clean status there (commit `fec9ee4`, pushed to `origin/master`)

---

## Eval Commands (post-implementation)

```bash
# E1: stale-prefix sweep (exit 1 = pass)
grep -rnE "src/(errors|http|state|types|cmd|auth)/" docs/AUTH_DEVICE_LOGIN.md docs/CHANGELOG_VOICE.md docs/TEMPLATE.md docs/VERIFY_TIERS.md docs/EXECUTE_DOC_READS.md docs/SKILL_FRONTMATTER_SCHEMA.md docs/REST_API_DESIGN_GUIDELINES.md && echo "FAIL" || echo "PASS"
# E2: phantom middleware (exit 1 = pass)
grep -nE "registry\.(admin|operator|slack)\(" docs/REST_API_DESIGN_GUIDELINES.md && echo "FAIL" || echo "PASS"
# E3: §7 cited paths exist (no MISSING lines = pass)
grep -oE "src/agentsfleetd/[a-z_/]+\.zig" docs/REST_API_DESIGN_GUIDELINES.md | sort -u | while read -r p; do [ -e "$p" ] || echo "MISSING: $p"; done
# E4: make targets named in the guide exist (no PHANTOM lines = pass)
grep -oE "make [a-z-]+" docs/REST_API_DESIGN_GUIDELINES.md | sort -u | sed 's/^make //' | while read -r t; do grep -qrE "^${t}:|^_${t}:" make/ Makefile || echo "PHANTOM: $t"; done
# E5: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted; prose rows removed from §7 tables reference files already gone from the tree (their absence is the point).

---

## Discovery (consult log)

- **Provenance of scope** — carved out of M106 CHORE(close) by decision:
  > Indy (2026-07-02): "M106 fixes here, rest → ticket (Recommended)" — context: AskUserQuestion on doc-fix scope for PR #468; M106-caused staleness landed there (AUTH.md, docs/architecture/), this pre-existing drift became this spec.
- **Metrics review** — no analytics/funnel playbook update required — docs-only diff.
- **§8 wrapper-table staleness — escalated from "flagged, deferred" to "fixed"** — during PLAN, `hx.zig`'s own top comment confirmed the `authenticated()`/`authenticatedWithParam()` comptime wrappers §8 documents were removed in M18_002 Batch D. Initially deferred (outside this spec's Files-Changed row, `AskUserQuestion` unanswered, Hard Safety default = no unapproved scope expansion). A subsequent adversarial fact-check agent (part of the `/review` skill-chain step) proved the staleness was far more severe than "conceptually outdated": §8 contained outright fabricated code — a `db()`/`releaseDb()`/`redis()` method set on `Hx` that doesn't exist (real `Hx` only has `ok()`/`fail()`/`noContent()`), a fabricated example file `webhooks.zig` with an `agent_id`+`url_secret` signature that doesn't exist anywhere in the tree, and a `common.authenticate` call that doesn't exist. Given this doc's own audience section calls it "Canonical instruction set. Read this before adding, modifying, or removing any HTTP endpoint," shipping proven-fabricated code examples is a materially worse RULE NDC violation than initially assessed — reversed the deferral and fixed §8 (Hx struct reference, the wrapper table, the multi-path-param claim, the two "enforced by comptime wrappers" lines) with facts verified against `hx.zig` and `server.zig::dispatchMatchedRoute()`.
- **§3 pagination helper names — fixed (own-edit miss)** — the same fact-check found `parsePaginationParams`/`derivePaginationResult` (cited in §3 as the shared keyset-pagination helper in `common.zig`) don't exist anywhere in the codebase. This line was in this spec's own edit scope (I fixed its path prefix earlier without verifying the cited function names existed) — corrected to describe the real split: cursor encode/decode goes through `fleet_runtime/keyset_cursor.zig`, `limit` parsing is currently per-handler (no shared helper for that piece yet), and the legacy `page`/`page_size` shape is backed by `pagination.zig::parsePageParams`.
- **`/review` outcome (adversarial fact-check agent)** — ran one thorough pass cross-referencing every path, function name, and behavioral claim in the diff against the real source tree (`test -e` per path, `grep`/read per function/behavior claim). 5 findings, all CONFIRMED and fixed (the two above, both in §8/§3). Everything else — the "six places" registration model, all six middleware policy names, the raw-handler exceptions table, the reference-implementations table, both non-REST-guide docs' path fixes — independently verified accurate against the real tree.
- **`/write-unit-test` outcome** — docs-only diff; Test Specification is entirely eval-tier (E1–E5), all run and passing (see Verification Evidence). No code-test framework applies.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits the eval suite vs this Test Specification (docs diff ⇒ eval commands are the coverage) | Clean; outcome in Discovery |
| After evals pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec + RULE NDC/NLR (no dead code described as alive; co-located falsehoods fixed) | Clean OR findings dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff | Comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Stale-prefix sweep | E1 | 0 hits across all seven docs | ✅ |
| Phantom middleware | E2 | 0 hits for `registry.(admin\|operator\|slack)(` | ✅ |
| Cited paths exist | E3 | 0 `MISSING:` lines (all `src/agentsfleetd/*.zig` citations resolve; the one genuinely-deleted file, `route_manifest.zig`, is cited without a full compilable path since it's an intentional historical reference) | ✅ |
| Make targets exist | E4 | 0 `PHANTOM:` lines (`make check-openapi`, `make test-unit-agentsfleetd`, `make lint-zig`, `make bench`, `make test-integration` all resolve in `make/*.mk`) | ✅ |
| Gitleaks | E5 | `no leaks found` (scanned `docs/`, ~6.16 MB) | ✅ |

**Test Delta:** unit 2270→2270 (+0) · integration 243→243 (+0) vs CHORE(open) baseline. Lacking: none — docs-only diff, no code surface touched, zero delta is expected.

---

## Out of Scope

- Doc generation tooling (route-registration reference derived from code) — right long game; separate spec when drift recurs.
- `docs/architecture/*.md` — already reconciled to M106 in PR #468 (`c74e35af`).
- `docs/AUTH.md` — its M106 §OAuth-connectors section landed in PR #468 (`e2299970`); §7 cross-references it instead of restating.
- Prose/style edits to sections whose facts are correct — churn without value.
- `DESIGN_SYSTEM.md:344` Slack "Planned" decision-log row — append-only historical record per the archive-don't-rewrite convention; not a defect.
