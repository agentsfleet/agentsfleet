# M91_004: `zombiectl memory list|search` — the operator's window into a zombie's durable memory

**Prototype:** v2.0.0
**Milestone:** M91
**Workstream:** 004
**Date:** Jun 11, 2026
**Status:** PENDING
**Priority:** P2 — operator tooling: today "my zombie forgot X" has no self-serve move (no Command-Line Interface (CLI) verb, no UI consumes the tenant memory endpoint); the rational-but-destructive workaround is re-pasting facts into SKILL.md every run
**Categories:** CLI
**Batch:** B1 — runs **in parallel** with M91_001 (disjoint trees: `zombiectl/` here vs `src/zombied/` there; the M84_003 ∥ M84_005 pattern); one shared touchpoint — the `updated_at` wire type M91_003 changes — so this PR **lands after M91_003** and rebases the render helper
**Branch:** — added at CHORE(open)
**Depends on:** M91_003 (merge order only — `updated_at` becomes a JSON number; development is unblocked because the parsing lives in one isolated helper, the rebase touches one spot)
**Provenance:** agent-generated (memory-architecture analysis session, Jun 11, 2026) — grounded in the tenant read endpoint (`memory/handler.zig`: list / `?query=` / `?category=` / `?limit=` already shipped and read-only) and the M14_001 CLI intent (export/import deferred; read verbs never built); re-confirm at PLAN.

**Canonical architecture:** `docs/architecture/direction.md:20` (the CLI surfaces raw entries; the human — like the model — is the search engine) + the tenant memory API in `src/zombied/http/handlers/memory/`.

---

## Implementing agent — read these first

1. `zombiectl/src/commands/zombie_list.ts` and `zombiectl/src/commands/zombie_events.ts` — the read-only workspace-scoped GET command pattern to mirror: auth, workspace resolution, fetch, render.
2. `zombiectl/src/program/cli-tree-zombie.ts` (and the sibling tree files) — how nouns/verbs register; the new `memory` noun mirrors the nearest read command's registration.
3. `src/zombied/http/handlers/memory/handler.zig` — the endpoint truth: `GET /v1/workspaces/{ws}/zombies/{zid}/memories`, params `query`/`category`/`limit` (limit clamps at the server's recall maximum), errors `UZ-MEM-002` (not found) / `UZ-MEM-003` (unavailable).
4. `docs/TEMPLATE.md` Prior-Art §CLI — the "7 Pillars" (handler purity, output-as-a-service, structured errors with suggestion, auto-JSON when piped, 3-tier test pyramid) this command set must align with.
5. `dispatch/write_ts_adhere_bun.md` — TS FILE SHAPE DECISION at PLAN; Bun-primitive discipline.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m91): zombiectl memory list/search read verbs`
- **Intent (one sentence):** operators (and external agents piping JSON) can inspect any zombie's durable memory — list newest-first, filter by category, or substring-search — turning "my zombie forgot X" from a support ticket into a one-command diagnosis.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Resolve before EXECUTE: (a) whether `memory` registers as a top-level noun (`zombiectl memory list --zombie <id>`, matching M14_001's CLI intent) or under the `zombie` noun — follow the tree convention the nearest multi-noun command uses; (b) how the mirrored commands resolve the workspace for a `--zombie` argument. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — `zombiectl memory search --zombie z-123 "acme"` answers in one command whether the zombie ever stored the fact: present (it knows it), absent (it never stored it — fix SKILL.md discipline), or present-but-`daily` (it expired by design).
2. **Preserved user behaviour** — read-only: no write verb exists or is added (the tenant plane stays read-only by architecture); existing commands, auth, and config are untouched.
3. **Optimal-way check** — a thin client over an endpoint that already works is the whole job; the unconstrained-optimal adds an interactive browser, rejected as decoration until someone asks.
4. **Rebuild-vs-iterate** — iterate: mirror the existing read-command pattern; zero new infrastructure.
5. **What we build** — two verbs (`list`, `search`) with `--zombie`, `--category`, `--limit`; human table + auto-JSON when piped; structured errors with suggestions.
6. **What we do NOT build** — write/edit/forget verbs, export/import (M14-era deferral, still parked), an interactive TUI, any client-side ranking (raw entries, newest-first — the reader judges relevance).
7. **Fit with existing features** — the inspection half of M91_001's counters (counter says "loss happened", CLI shows what survived); displays the categories M91_002's tiers run on; renders M91_003's numeric timestamps; the future dashboard (M14_003, still ghost) reads the same endpoint and stays read-only.
8. **Surface order** — CLI first, by repo posture and by this spec; UI later, same endpoint; both read-only.
9. **Dashboard restraint** — unchanged by this workstream: no UI ships; the CLI proving the read shape is the prerequisite the dashboard waits on.
10. **Confused-user next step** — this IS the confused user's next step. Its own confusion case (empty result) renders a plain "no memories matched" with the hygiene-docs pointer — exit 0, because an empty store is an answer, not an error.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE CLI** (command conventions), **RULE UFS** (preview-length, default/maximum limit constants single-sourced — client mirrors the server's published limits), **RULE NDC** (no speculative flags or dead options).
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE DECISION at PLAN; `const`/import discipline; Bun primitives.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — consumed side: the client binds to the published OpenAPI shapes, no undocumented params.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TS FILE SHAPE (write_ts_adhere_bun) | yes — new `*.ts` command module | shape verdict at PLAN; command → handler → render split per the 7 Pillars |
| UFS | yes — limits, preview length | named constants; server limits referenced, not re-invented |
| LENGTH | yes — new files | command module split if approaching the cap |
| UI / DESIGN TOKEN | no | terminal output only; no React surface |
| ZIG / SCHEMA / ERROR REGISTRY | no | no server-side change of any kind |

---

## Overview

**Goal (testable):** `zombiectl memory list --zombie <id>` prints that zombie's entries newest-first (key, category, updated, content preview) and `zombiectl memory search --zombie <id> <query>` prints substring matches — human table on a terminal, stable JSON when piped — with exit 0 on empty results and a structured suggestion-bearing error on unknown zombie; proven end-to-end by subprocess tests against the built binary.

**Problem:** the tenant memory endpoint shipped in M84_005's read-only form, but nothing consumes it: zombiectl has zero memory verbs and no UI exists. Operators cannot see what their zombie knows, so memory failures are undiagnosable and the workaround (stuffing facts into SKILL.md) bloats every run and defeats the feature.

**Solution summary:** two read verbs in a new zombiectl command module mirroring the existing workspace-scoped read commands, passing `query`/`category`/`limit` through to the endpoint, rendering via the output-as-a-service pattern (human vs auto-JSON), and mapping the endpoint's error codes to structured CLI errors with actionable suggestions. No server changes.

---

## Prior-Art / Reference Implementations

- **CLI** → the "7 Pillars" (`docs/TEMPLATE.md` Prior-Art): command → handler → errors split; handler purity (no `console.log`/`process.exit` in handlers); output as a service (renderer chooses human vs JSON); structured JSON errors with `suggestion`; auto-JSON when stdout is piped; 3-tier test pyramid. Full alignment; no divergence.
- **Nearest commands** → `zombie_list.ts` (workspace-scoped listing, table render) and `zombie_events.ts` (per-zombie read with filters). The memory verbs are their composition.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/memory.ts` | CREATE | the two verbs: argument parsing, handler, endpoint call |
| `zombiectl/src/program/` (tree registration file per Handshake a) | EDIT | register the `memory` noun + verbs |
| `zombiectl/src/commands/types.ts` | EDIT | response/row types for the memory endpoint (if the convention keeps shared types here) |
| zombiectl test tree (mirroring the nearest command's unit + subprocess e2e files) | CREATE | handler unit tests + built-binary e2e |
| `zombiectl/README.md` (or the help surface the repo maintains) | EDIT | document the verbs |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one command module, two verbs, shared handler core (list and search differ by one query param).
- **Alternatives considered:** (1) `zombie memory` sub-noun vs top-level `memory` noun — resolved at Handshake (a) by tree convention, not preference. (2) client-side relevance ranking for search — rejected: `direction.md:20` posture; raw entries newest-first, the reader judges. (3) waiting for the dashboard instead — rejected: CLI-first is the repo posture and the dashboard has no spec.
- **Patch-vs-refactor verdict:** **patch** — additive command following an established pattern.

---

## Sections (implementation slices)

### §1 — `memory list`

Newest-first listing with optional `--category` and `--limit` (client validates positive integer; server clamps to its recall maximum — the client mirrors the published default/max as constants, it does not invent its own).

- **Dimension 1.1** — list renders key/category/updated/preview newest-first → `test_memory_list_table_newest_first`
- **Dimension 1.2** — `--category` filters; `--limit` caps; invalid limit fails client-side with usage text → `test_memory_list_flags_and_validation`
- **Dimension 1.3** — empty store: friendly "no memories" line, exit 0 → `test_memory_list_empty_exit_zero`

### §2 — `memory search`

Positional query → endpoint `?query=`; matches across key and content (server semantics); newest-first; same render path as list.

- **Dimension 2.1** — matching query prints only matching rows → `test_memory_search_matches`
- **Dimension 2.2** — no-match: "no memories matched" + hygiene-docs pointer, exit 0 → `test_memory_search_empty_exit_zero`

### §3 — Output as a service

Terminal → aligned table with content preview truncated at a named constant (full content never lost: JSON mode carries it verbatim); piped stdout → stable JSON array of full entries; `updated_at` rendered as local time in the table, raw value in JSON. **Implementation default:** isolate `updated_at` parsing in a single helper — M91_003 flips the wire type from string to number, and this PR rebases that one spot when it lands second (see Batch).

- **Dimension 3.1** — piped invocation emits parseable JSON with full untruncated content → `test_memory_output_json_when_piped`
- **Dimension 3.2** — table preview truncates at the constant; multibyte content never splits a UTF-8 sequence → `test_memory_preview_truncation_utf8_safe`

### §4 — Errors with suggestions

Endpoint error codes map to structured CLI errors: unknown zombie (`UZ-MEM-002`) suggests `zombiectl zombie list`; memory backend unavailable (`UZ-MEM-003`) suggests retry; auth failures follow the existing login-flow suggestions. Nonzero exit for errors only — never for empty results.

- **Dimension 4.1** — unknown zombie: structured error, named code, suggestion, nonzero exit → `test_memory_unknown_zombie_error_shape`
- **Dimension 4.2** — network failure: structured error, retry suggestion, nonzero exit → `test_memory_network_error_shape`

### §5 — End-to-end against the built binary

The repo's subprocess harness (`test-unit-zombiectl` spawns `dist/bin/zombiectl.js`) walks `--help`, flag validation, and render paths for both verbs — the user-centric e2e the template mandates for CLI categories.

- **Dimension 5.1** — `zombiectl memory --help` and per-verb help render the documented grammar → `test_memory_help_e2e`
- **Dimension 5.2** — list + search subprocess runs against a stubbed/sandboxed endpoint render table and JSON correctly → `test_memory_e2e_list_search`

---

## Interfaces

```
zombiectl memory list   --zombie <id> [--category <name>] [--limit <n>] [--workspace <ws>]
zombiectl memory search --zombie <id> <query> [--limit <n>] [--workspace <ws>]
```

- Wire: `GET /v1/workspaces/{ws}/zombies/{zid}/memories` with `query`/`category`/`limit` passthrough; no new server surface.
- JSON output (piped): array of `{ key, content, category, updated_at }` — full content, raw numeric `updated_at`.
- Exit codes: `0` success (including zero results); nonzero per the existing CLI error-mapping convention for auth/not-found/network failures.
- Workspace resolution: identical to the mirrored commands (flag > config default), locked at Handshake (b).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| unknown zombie | bad id / wrong workspace | structured error with `UZ-MEM-002`, suggestion `zombiectl zombie list`, nonzero exit |
| memory backend unavailable | server-side `UZ-MEM-003` | structured error, retry suggestion, nonzero exit |
| auth missing/expired | no login | existing auth-error path + login suggestion |
| network failure / timeout | connectivity | structured error, retry suggestion, nonzero exit; no partial table |
| invalid `--limit` | user input | client-side usage error before any request |
| oversized content | 16 KiB entries | table preview truncated at the constant (UTF-8-safe); JSON carries it whole |
| empty result | nothing stored / nothing matched | informative line + docs pointer, exit 0 — an answer, not an error |

---

## Invariants (Hard Guardrails)

1. **Read-only client** — no code path issues anything but GET; enforced by the module exposing no mutating verb and e2e asserting the request method.
2. **Handler purity** — no `console.log`/`process.exit` inside handlers; renderer and exit mapping own the edges (7 Pillars); enforced by the existing zombiectl lint conventions + unit tests calling handlers directly.
3. **Piped output is machine-stable** — JSON mode never mixes human text onto stdout; enforced by `test_memory_output_json_when_piped` parsing stdout strictly.
4. **Empty ≠ error** — exit 0 on zero results; enforced by Dimensions 1.3 / 2.2.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_memory_list_table_newest_first` | three fixture rows → table rows in `updated_at` desc order |
| 1.2 | unit | `test_memory_list_flags_and_validation` | `--category daily` filters; `--limit 0` → usage error, no request |
| 1.3 | e2e | `test_memory_list_empty_exit_zero` | empty store → "no memories" on stdout, exit 0 |
| 2.1 | unit | `test_memory_search_matches` | query hits one of three rows → only that row rendered |
| 2.2 | e2e | `test_memory_search_empty_exit_zero` | no match → message + docs pointer, exit 0 |
| 3.1 | integration | `test_memory_output_json_when_piped` | piped run → `JSON.parse` succeeds; content byte-identical to fixture |
| 3.2 | unit | `test_memory_preview_truncation_utf8_safe` | multibyte content at the boundary → no broken sequence in the table |
| 4.1 | e2e | `test_memory_unknown_zombie_error_shape` | bad id → structured error with code + suggestion, nonzero exit |
| 4.2 | integration | `test_memory_network_error_shape` | unreachable endpoint → retry-suggestion error, nonzero exit |
| 5.1 | e2e | `test_memory_help_e2e` | `--help` for noun + both verbs renders documented grammar |
| 5.2 | e2e | `test_memory_e2e_list_search` | subprocess against stub → table and JSON paths both correct |

Regression: existing command help trees unchanged (`test_memory_help_e2e` asserts additive registration only). Negative paths: 1.2, 4.1, 4.2.

---

## Acceptance Criteria

- [ ] Both verbs work end-to-end against the built binary — verify: `make test-unit-zombiectl`
- [ ] Piped output is strict JSON with full content — verify: `make test-unit-zombiectl` (3.1)
- [ ] Empty results exit 0; errors exit nonzero with suggestions — verify: `make test-unit-zombiectl` (1.3, 2.2, 4.1)
- [ ] No server-side diff — verify: `git diff --name-only origin/main | grep -v zombiectl | grep -v '\.md$'` → empty
- [ ] `make lint-zombiectl` clean · zombiectl build green (`cd zombiectl && bun install && bun run build`)
- [ ] `gitleaks detect` clean · no file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: Build the CLI (mandatory before its test tier)
cd zombiectl && bun install && bun run build && cd ..
# E2: CLI unit + subprocess e2e
make test-unit-zombiectl 2>&1 | tail -5
# E3: Lint (CLI tier)
make lint-zombiectl 2>&1 | tail -3
# E4: Server untouched (expect empty)
git diff --name-only origin/main | grep -v zombiectl | grep -v '\.md$'
# E5: Help surface registered
./zombiectl/dist/bin/zombiectl.js memory --help | head -10
# E6: Gitleaks + 350-line gate
gitleaks detect 2>&1 | tail -2; git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted; additive command module (RULE NDC holds: no speculative flags ship).

---

## Discovery (consult log)

- **Consults** — (empty at creation; Handshake (a)/(b) resolutions land here)
- **Skill chain outcomes** — (`/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`)
- **Deferrals** — none; any deferral needs an Indy-acked verbatim quote here.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |
| After every push | `kishore-babysit-prs` | final report in Discovery |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| CLI build | `cd zombiectl && bun run build` | | |
| CLI tests | `make test-unit-zombiectl` | | |
| CLI lint | `make lint-zombiectl` | | |
| Server untouched | Eval E4 | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- Write verbs (`store`/`forget`) — the tenant plane is read-only by architecture; a fenced operator-write design would be its own milestone.
- Export/import to markdown — the parked M14-era scope; revisit on ask.
- Interactive browser / TUI, client-side ranking, pagination beyond `--limit` — wait for demand; the endpoint has no cursor yet.
- Dashboard memory view (M14_003 ghost) — reads this same endpoint when it happens; blocked on M91_001 baselines per Product Clarity restraint.
