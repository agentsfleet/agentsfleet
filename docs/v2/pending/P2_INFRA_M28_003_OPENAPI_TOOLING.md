# P2_INFRA_M28_003: OpenAPI Split, Bundle, and Sync Gate

**Prototype:** v0.18.0
**Milestone:** M28
**Workstream:** 003
**Date:** Apr 18, 2026
**Status:** PENDING
**Priority:** P2 — `public/openapi.json` is currently 4,323 lines of hand-edited JSON. M28_002 pushes it past 4,500. Mintlify reads it directly from `main` for `docs.usezombie.com`. Drift between the Zig handlers and the hand-edited spec is a matter of time, not probability. This spec removes that risk with a split-source + bundler + CI gate.
**Batch:** B3 (blocked on M28_001 + M28_002 landing — avoid conflicts in openapi.json during split)
**Branch:** `feat/m28-openapi-tooling` (when started)
**Depends on:** M28_001 (webhook auth middleware), M28_002 (tenant API key management)

---

## Overview

**Goal (testable):** `public/openapi.json` becomes a build artifact produced by bundling YAML files under `public/openapi/`. Hand-editing `public/openapi.json` directly is forbidden and blocked by CI. A new `make check-openapi-sync` target asserts path parity between `src/http/router.zig` and `public/openapi.json`, and runs on every PR. Mintlify continues to read `public/openapi.json` from `main`, so `docs.usezombie.com` is unaffected.

**Problem (three observable symptoms):**
1. `public/openapi.json` is 4,323 lines and growing by 50–200 lines per feature PR. Reviews can't practically catch OpenAPI-side bugs at that size — people skim and merge.
2. Mintlify reads it from raw GitHub main (`docs.json:76`), so `docs.usezombie.com` is exactly whatever is on `main/public/openapi.json`. Preview PRs do not reflect API-doc changes, so reviewers cannot catch doc regressions before merge.
3. There is no gate that asserts the Zig handlers and `openapi.json` describe the same set of endpoints. A handler renamed to `/v1/workspaces/{id}/runs` while OpenAPI still says `/v1/workspaces/{id}/run` would ship, break the SDK explorer, and get caught only when a customer files a bug.

**Solution summary:** Split `public/openapi.json` into a directory of small YAML files (`root.yaml` + per-tag `paths/*.yaml` + shared `components/*.yaml`). Bundle with `@redocly/cli` via a `make openapi` target; the bundled JSON is still committed so Mintlify can read it. Add `make check-openapi-sync` — a small Python script that asserts every path in `router.zig` has a matching entry in `openapi.json` and vice versa. Wire both into CI. The existing `make check-openapi-errors` lint is folded into the Redocly lint config.

**Not a token-efficiency project.** The YAML split reduces *human* review surface and makes *agent* edits reliable (localized files + CI gates). It does not change what machine consumers ingest — they still read bundled `public/openapi.json`. If a future LLM/SDK pipeline needs per-tag filtered slices, build it as a separate workstream on top of this one (`npx @redocly/cli bundle --filter tag=…`); do not fold it in here.

**Out-of-scope for this spec (deliberate):** generating OpenAPI directly from Zig handler types via comptime reflection is a separate project — larger scope, touches every handler, and only justified if drift still slips past this gate after real usage. Do not fold it in here.

**Agent-edit use cases in scope (§5):** renaming a path (e.g. `/external-agents` → `/agent-keys`), updating an operation's description/summary, removing an endpoint, appending a new endpoint. These are the daily edits an autonomous agent must perform without corrupting the spec. The split YAML + sync gate + bundle-in-sync gate make these edits safe; §5 documents the recipe.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `public/openapi/root.yaml` | CREATE | New source-of-truth entrypoint (info, servers, tags, `$ref` to paths + components) |
| `public/openapi/paths/api-keys.yaml` | CREATE | Split: all `/v1/api-keys*` operations |
| `public/openapi/paths/workspaces.yaml` | CREATE | Split: all `/v1/workspaces/*` operations |
| `public/openapi/paths/zombies.yaml` | CREATE | Split: all `/v1/workspaces/{id}/zombies*` operations |
| `public/openapi/paths/webhooks.yaml` | CREATE | Split: all webhook endpoints |
| `public/openapi/paths/runs.yaml` | CREATE | Split: all `/v1/runs*` operations |
| `public/openapi/paths/integrations.yaml` | CREATE | Split: all `/v1/integrations*` operations |
| `public/openapi/paths/*.yaml` (remaining tags) | CREATE | One file per remaining tag — aim for ≤ 400 lines each |
| `public/openapi/components/schemas.yaml` | CREATE | Shared request/response schemas (Zombie, Workspace, ApiKey, etc.) |
| `public/openapi/components/responses.yaml` | CREATE | Shared responses (ErrorBody, Paginated, NotFound, Forbidden) |
| `public/openapi/components/security.yaml` | CREATE | Security schemes (bearerAuth, webhookHmac, slackSignature) |
| `public/openapi.json` | REGENERATE | Now a bundled build artifact; still committed so Mintlify main-branch fetch keeps working |
| `.redocly.yaml` | CREATE | Lint config: require `operationId`, `summary`, `tags`, `ErrorBody` for 4xx/5xx (subsumes existing `check_openapi_errors.py`) |
| `scripts/check_openapi_sync.py` | CREATE | Router ↔ OpenAPI path-parity gate (≤ 60 lines Python; matches existing `scripts/check_openapi_errors.py` pattern) |
| `Makefile` / `make/quality.mk` | MODIFY | Add `make openapi` (bundle + lint) and `make check-openapi-sync` (parity gate); fold existing `check-openapi-errors` into `make openapi` Redocly lint |
| `.github/workflows/ci.yml` | MODIFY | Wire `make check-openapi-sync` into the lint job; add "bundled artifact is in sync" check that re-bundles and asserts `public/openapi.json` is unchanged |
| `docs/REST_API_DESIGN_GUIDELINES.md` | MODIFY | Document the split-file workflow: "edit YAML under `public/openapi/`, run `make openapi`. Never hand-edit `public/openapi.json`." |
| `public/AGENTS.md` | PRE-EXISTING | Index of agent-facing public surfaces (llms.txt, skill.md, agent-manifest.json, heartbeat, openapi.json). Created as a prerequisite to this spec; referenced from `public/openapi/AGENTS.md`. |
| `public/openapi/AGENTS.md` | CREATE | Agent-edit recipe for the split OpenAPI: rename-path / append-path / remove-path / update-description step-by-step, plus the "never hand-edit `public/openapi.json`" rule. Replaces the originally proposed `README.md`. |
| `scripts/check_openapi_errors.py` | DELETE | Superseded by Redocly lint rules in `.redocly.yaml` |

## Applicable Rules

- RULE HGD — every new handler must follow api_handler_guide.md; this spec enforces the OpenAPI side of that at CI time
- RULE ORP — orphan sweep: once the YAML split is authoritative, any stray reference to editing `public/openapi.json` directly is an orphan
- RULE FLL — file length ≤ 350 lines for .zig / .js; this spec doesn't touch those but `paths/*.yaml` files should target ≤ 400 lines each for reviewability (advisory, not a hard rule)

---

## Sections (implementation slices)

### §1 — YAML split (source of truth)

**Status:** PENDING

Convert the current 4,323-line monolithic `public/openapi.json` into a directory of small YAML files. Shape:

```
public/openapi/
├── root.yaml                # ~30 lines — openapi, info, servers, tags, $ref tree
├── paths/
│   ├── api-keys.yaml        # ≤ 400 lines — /v1/api-keys/*
│   ├── workspaces.yaml
│   ├── zombies.yaml
│   ├── runs.yaml
│   ├── integrations.yaml
│   └── webhooks.yaml
└── components/
    ├── schemas.yaml
    ├── responses.yaml       # ErrorBody, pagination envelope, common 4xx/5xx
    └── security.yaml
```

**`root.yaml` shape:**

```yaml
openapi: 3.1.0
info:
  title: usezombie API
  version: 0.18.0
servers:
  - url: https://api.usezombie.com
tags:
  - name: API Keys
  - name: Workspaces
  - name: Zombies
paths:
  /v1/api-keys:
    $ref: './paths/api-keys.yaml#/root'
  /v1/api-keys/{id}:
    $ref: './paths/api-keys.yaml#/item'
components:
  $ref: './components/schemas.yaml'
```

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `public/openapi/` directory | Split existing 4.3K-line `openapi.json` into YAML files by tag | Every path in the original `openapi.json` appears in exactly one `paths/*.yaml`; every shared schema appears in `components/schemas.yaml` | manual diff |
| 1.2 | PENDING | `public/openapi/root.yaml` | YAML parse | Parses cleanly; all `$ref` targets resolve to existing files | unit (redocly lint) |
| 1.3 | PENDING | `make openapi` after split | Run bundler against split source | `public/openapi.json` byte-diff against the pre-split version is empty after `jq -S` canonicalization (key ordering may differ; content must not) | integration |

### §2 — Bundler + lint (`make openapi`)

**Status:** PENDING

New make target that runs `@redocly/cli` to (a) bundle the YAML tree into `public/openapi.json`, (b) lint the bundled output. Replaces the existing `check-openapi-errors` target by folding its rules into the Redocly config.

**`make openapi`:**

```make
openapi:  ## Bundle public/openapi/ into public/openapi.json and lint
	npx @redocly/cli@latest bundle public/openapi/root.yaml -o public/openapi.json
	npx @redocly/cli@latest lint public/openapi.json --config .redocly.yaml
```

**`.redocly.yaml` (minimum):**

```yaml
extends: [recommended]
rules:
  operation-operationId: error
  operation-summary: error
  operation-tag-defined: error
  # Subsumes check_openapi_errors.py: every 4xx/5xx uses ErrorBody
  operation-4xx-response: error
  no-invalid-media-type-examples: error
```

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `make openapi` | clean YAML source | Exits 0; `public/openapi.json` regenerated; lint passes | integration |
| 2.2 | PENDING | `make openapi` with a bad 4xx response | Edit a path to reference a schema that is not `ErrorBody` | Redocly lint fails with the specific path + rule name | integration |
| 2.3 | PENDING | `make openapi` idempotency | Run `make openapi` twice in a row | Second run produces byte-identical output to the first | unit |

### §3 — Router ↔ OpenAPI sync gate (`make check-openapi-sync`)

**Status:** PENDING

Python script (~60 lines, matches existing `scripts/check_openapi_errors.py` pattern) that asserts path parity between `src/http/router.zig` and `public/openapi.json`. Runs in CI on every PR.

**`scripts/check_openapi_sync.py` (sketch):**

```python
#!/usr/bin/env python3
"""Fail if router.zig paths and openapi.json paths diverge.

ASSUMPTION: src/http/router.zig registers every /v1 route as a LITERAL string
match arm of the form `"/v1/..." =>`. This script's regex cannot see through
string interpolation, variable-based path construction, comptime concatenation,
or macro-expanded route tables. If router.zig is ever refactored to register
routes dynamically, this gate will silently drop coverage — it will still exit
0 but compare an incomplete set.

DEFENSIVE FLOOR: the script asserts a minimum known-good path count. If the
extracted router set drops below MIN_EXPECTED_PATHS, we fail the build and
force a human review of the regex. Bump MIN_EXPECTED_PATHS in lockstep when
new endpoints land.
"""
import json, re, sys, pathlib

# Floor — bump when new endpoints land. If the regex silently breaks, this
# fires before path parity does.
MIN_EXPECTED_PATHS = 30

ROUTER = pathlib.Path("src/http/router.zig").read_text()
SPEC = json.loads(pathlib.Path("public/openapi.json").read_text())

# Extract "/v1/..." literals from router.zig `match()` arms.
router_paths = set(re.findall(r'"(/v1/[^"]+)"\s*=>', ROUTER))
# Normalize :id → {id} for OpenAPI-style path params.
router_paths = {re.sub(r':(\w+)', r'{\1}', p) for p in router_paths}

if len(router_paths) < MIN_EXPECTED_PATHS:
    print(
        f"FAIL: router_paths set has {len(router_paths)} entries, "
        f"below floor of {MIN_EXPECTED_PATHS}. "
        "The regex may have silently stopped matching — review "
        "scripts/check_openapi_sync.py against the current router.zig shape."
    )
    sys.exit(1)

spec_paths = set(SPEC.get("paths", {}).keys())

missing_in_spec = router_paths - spec_paths
missing_in_router = spec_paths - router_paths

if missing_in_spec or missing_in_router:
    if missing_in_spec:
        print(f"FAIL: router has paths not in openapi.json: {sorted(missing_in_spec)}")
    if missing_in_router:
        print(f"FAIL: openapi.json has paths not in router: {sorted(missing_in_router)}")
    sys.exit(1)

print(f"OK: router ↔ openapi.json path parity ({len(router_paths)} paths)")
```

**Regex coverage invariant:** the pattern `"/v1/..." =>` matches every literal
route arm in `router.zig` today. If a future refactor moves routes into a
variable-driven table or a comptime-built dispatch map, this script will miss
them. The `MIN_EXPECTED_PATHS` floor is the canary — it fails loudly if the
regex stops matching enough routes. Update it in the same commit that adds a
new literal route.

**`make check-openapi-sync`:**

```make
check-openapi-sync:  ## Assert router.zig ↔ openapi.json path parity
	python3 scripts/check_openapi_sync.py
```

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `make check-openapi-sync` on a clean tree | main at merge time | Exits 0; prints `OK: router ↔ openapi.json path parity (N paths)` | integration |
| 3.2 | PENDING | `make check-openapi-sync` with a synthetic drift | Add a router arm without an OpenAPI entry | Exits 1; prints the unmatched path | integration |
| 3.3 | PENDING | `make check-openapi-sync` with a reverse drift | Add an OpenAPI path without a router arm | Exits 1; prints the unmatched path | integration |
| 3.4 | PENDING | CI workflow | `.github/workflows/ci.yml` adds `make check-openapi-sync` to the lint job | A synthetic drift PR fails CI before merge | manual |
| 3.5 | PENDING | CI "bundle is in sync" check | Re-run `make openapi` in CI and assert `git diff --exit-code public/openapi.json` | A PR that edited YAML but forgot to re-bundle fails CI | manual |

### §4 — Docs update + `public/openapi.json` hand-edit prevention

**Status:** PENDING

Update `docs/REST_API_DESIGN_GUIDELINES.md` to document the split workflow. Add `public/openapi/AGENTS.md` explaining "edit the YAML, run `make openapi`, never hand-edit `public/openapi.json`". Optionally, add a pre-commit hook (advisory — not a hard gate) that warns when `public/openapi.json` is staged without corresponding YAML changes.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `docs/REST_API_DESIGN_GUIDELINES.md` | Read the "Adding a new endpoint" section | Documents: (1) create YAML under `public/openapi/paths/`, (2) run `make openapi`, (3) commit both YAML and bundled JSON | manual |
| 4.2 | PENDING | `public/openapi/AGENTS.md` | Read | States: source of truth is YAML; `public/openapi.json` is a build artifact; direct edits will be lost on the next `make openapi`. Links back to `public/AGENTS.md` for the full public-surface index. | manual |

### §5 — Agent-edit ergonomics (`public/openapi/AGENTS.md` recipe)

**Status:** PENDING

Autonomous agents are first-class editors of this spec. Daily operations: rename a path, append a path, remove a path, update a description. The split YAML localises each edit to a single small file (≤ 400 lines), and the CI gates (§2, §3) catch the common failure modes — hand-editing the bundled JSON, renaming the OpenAPI path without the router arm, or vice versa.

`public/openapi/AGENTS.md` is the deterministic recipe agents follow. It MUST include:

**Hard rule (top of file):** `public/openapi.json` is a build artifact. Never edit it directly — edits are wiped on the next `make openapi` and CI's "bundle in sync" gate will fail the PR.

**Recipes (copy-paste-ready):**

```
Rename a path (e.g. /external-agents → /agent-keys):
  1. Find the owning file: grep -rl "external-agents:" public/openapi/paths/
  2. Rename the path key in that YAML file
  3. Rename the corresponding match arm in src/http/router.zig
  4. make openapi && make check-openapi-sync

Append a new path:
  1. Pick (or create) public/openapi/paths/<tag>.yaml — aim for ≤ 400 lines
  2. Add the operation block (operationId, summary, tags, responses using $ref: '#/components/responses/ErrorBody' for 4xx/5xx)
  3. If new file: add a $ref from public/openapi/root.yaml
  4. Add the router arm in src/http/router.zig
  5. make openapi && make check-openapi-sync

Remove a path:
  1. Delete the operation block in public/openapi/paths/<tag>.yaml
     (if the file becomes empty, delete the file AND its $ref in root.yaml)
  2. Remove the router arm in src/http/router.zig
  3. make openapi && make check-openapi-sync

Update a description or summary:
  1. Edit the field in public/openapi/paths/<tag>.yaml
  2. make openapi   (no router change → sync check not required, but safe to run)
```

**Cross-reference:** Links to `public/AGENTS.md` for the full agent-facing public-surface index (llms.txt / skill.md / agent-manifest.json / heartbeat / openapi.json). Any path rename/add/remove that affects the operation set MUST also be reflected in `public/llms.txt` and `public/skill.md` operation tables.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `public/openapi/AGENTS.md` | Read | Contains all four recipes (rename / append / remove / update description), the hard rule, and a link to `public/AGENTS.md` | manual |
| 5.2 | PENDING | Rename rehearsal | Follow the rename recipe on a throwaway path in a scratch branch | `make openapi` + `make check-openapi-sync` both pass; `git diff` shows exactly 2 changed files (one YAML, `router.zig`) plus the regenerated `openapi.json` | integration (manual) |
| 5.3 | PENDING | Append rehearsal | Follow the append recipe for a dummy `GET /v1/_probe` | Bundle succeeds; sync gate passes; Redocly lint does not complain about missing `operationId`/`tags`/`ErrorBody` | integration (manual) |
| 5.4 | PENDING | Hand-edit detection | Edit `public/openapi.json` directly and commit without running `make openapi` | CI "bundle in sync" check fails with the diff; the error message points the agent at `public/openapi/AGENTS.md` | manual (CI rehearsal) |

---

## Interfaces

No new Zig interfaces. The contract surface is:
- `make openapi` — bundle + lint. Exit 0 on success.
- `make check-openapi-sync` — parity gate. Exit 0 on parity, 1 on drift.
- `public/openapi/root.yaml` — entrypoint for the bundler.
- `public/openapi.json` — bundled artifact; Mintlify's read target.

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| YAML `$ref` points at a non-existent file | Typo in `root.yaml` | Redocly bundle fails with file-not-found; `make openapi` exits non-zero | CI red; error message cites the missing ref |
| New router arm added without OpenAPI entry | Handler merged without updating YAML | `make check-openapi-sync` fails in CI | CI red; script prints unmatched path |
| OpenAPI path added without router arm | YAML edited without wiring handler | `make check-openapi-sync` fails in CI | CI red; script prints unmatched path |
| YAML edited but bundled JSON not re-committed | Developer forgot `make openapi` | CI "bundle in sync" check fails (re-bundle produces different output than committed) | CI red; diff of the un-bundled changes |
| Direct hand-edit to `public/openapi.json` | Developer bypasses YAML | Next `make openapi` regenerates the file and wipes the edit; if committed directly, CI "bundle in sync" catches it | CI red; advisory pre-commit hook warns if enabled |
| `npx @redocly/cli` network flake | CI container without cached npm | Bundle step fails with network error | CI red; retry or pin via `.npmrc` / Dockerfile |

**Platform constraints:**
- CI image must have `node` (for `npx @redocly/cli`) and `python3` on PATH. Both already present in the existing CI image.
- Mintlify reads `public/openapi.json` from raw GitHub main; the file MUST remain committed.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| Every endpoint in router.zig has a matching OpenAPI path | `make check-openapi-sync` |
| Every OpenAPI path has a matching router arm | `make check-openapi-sync` |
| Every operation has `operationId`, `summary`, `tags` | Redocly lint (`.redocly.yaml`) |
| Every 4xx/5xx response uses `$ref: '#/components/responses/ErrorBody'` | Redocly lint (subsumes `check_openapi_errors.py`) |
| `public/openapi.json` is a build artifact, never hand-edited | CI "bundle in sync" check + `public/openapi/README.md` policy |
| `make openapi` is idempotent | Dim 2.3 — byte-identical output across two runs |
| Mintlify continues to render `docs.usezombie.com/api-reference` identically | Manual before/after comparison on a staging Mintlify env |

---

## Test Specification

**Status:** PENDING

### Integration Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| Bundle reproduces pre-split JSON | 1.3 | Split complete; run `make openapi` | `jq -S . public/openapi.json > /tmp/new.json && jq -S . pre-split.json > /tmp/old.json && diff /tmp/old.json /tmp/new.json` → empty |
| `make openapi` fails on bad $ref | 2.1 | Break a `$ref` in a paths file | Exit non-zero; error cites the broken ref |
| `make openapi` fails on missing ErrorBody | 2.2 | Reference a non-ErrorBody schema for a 4xx response | Redocly lint fails with rule name |
| `make openapi` is idempotent | 2.3 | Run twice | Second run's output byte-identical to first |
| `make check-openapi-sync` passes on main | 3.1 | Clean tree | Exit 0; path count printed |
| `make check-openapi-sync` detects handler-without-spec | 3.2 | Add unmatched router arm | Exit 1; unmatched path printed |
| `make check-openapi-sync` detects spec-without-handler | 3.3 | Add unmatched OpenAPI path | Exit 1; unmatched path printed |

### Negative Tests

| Test name | Input | Expected error |
|-----------|-------|---------------|
| Direct edit to `public/openapi.json` caught by CI | Commit that changes `openapi.json` without a corresponding YAML change | CI "bundle in sync" step fails with a diff |
| Missing `operationId` | Add a path with no `operationId` | Redocly lint fails |
| Missing `tags` | Add a path with no `tags` | Redocly lint fails |

### Regression Tests

| Test name | What it guards |
|-----------|---------------|
| Mintlify renders every pre-split endpoint | Spot-check 5 endpoints on `docs.usezombie.com` before and after merge — identical rendering |
| Existing `make check-openapi-errors` behavior preserved | Every rule it enforced is now enforced by `.redocly.yaml` |

---

## Execution Plan (Ordered)

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Set up `public/openapi/` directory structure (root.yaml + empty paths/ + components/) | `ls public/openapi/` returns the expected tree |
| 2 | Extract `components/schemas.yaml` + `components/responses.yaml` + `components/security.yaml` from the monolithic `openapi.json`; reference via `$ref` in `root.yaml` | `npx @redocly/cli bundle public/openapi/root.yaml -o /tmp/test.json` succeeds |
| 3 | Split paths into `paths/*.yaml` by tag; wire `$ref` in `root.yaml` | `make openapi` produces `public/openapi.json`; `jq -S` diff against pre-split is empty |
| 4 | Write `.redocly.yaml` with lint rules that subsume `check_openapi_errors.py` | `npx @redocly/cli lint public/openapi.json --config .redocly.yaml` passes |
| 5 | Add `make openapi` target | `make openapi` exits 0; re-run is idempotent |
| 6 | Write `scripts/check_openapi_sync.py` | `python3 scripts/check_openapi_sync.py` exits 0 on clean tree |
| 7 | Add `make check-openapi-sync` target | Fails on synthetic drift; passes on clean tree |
| 8 | Wire both targets into CI (`.github/workflows/ci.yml`) | Synthetic drift PR fails CI |
| 9 | Delete `scripts/check_openapi_errors.py` and its make target | `grep -rn check_openapi_errors` returns zero hits outside historical files |
| 10 | Update `docs/REST_API_DESIGN_GUIDELINES.md` + add `public/openapi/AGENTS.md` (four recipes + hard rule + link to `public/AGENTS.md`) | Both documents land in the same PR; `public/AGENTS.md` is already in place from the prerequisite step |
| 11 | Rehearse recipes from `public/openapi/AGENTS.md`: run the rename + append recipes on a throwaway path in a scratch commit, confirm `make openapi` + `make check-openapi-sync` pass, then revert | Dim 5.2 and Dim 5.3 pass |
| 12 | Manual Mintlify regression check: compare 5 rendered endpoints before and after on a staging env | Rendering is identical |

---

## Acceptance Criteria

- [ ] `public/openapi/root.yaml` + `paths/*.yaml` + `components/*.yaml` exist and together describe every endpoint currently in `public/openapi.json`.
- [ ] `make openapi` regenerates `public/openapi.json`; diff against the pre-split canonicalized (`jq -S`) version is empty.
- [ ] `make openapi` is idempotent (second run produces identical output to first).
- [ ] `make check-openapi-sync` passes on `main` and fails on a synthetic drift PR.
- [ ] Both targets are wired into CI; a drift PR cannot merge.
- [ ] `check_openapi_errors.py` is deleted; its rules are enforced by `.redocly.yaml`.
- [ ] `docs/REST_API_DESIGN_GUIDELINES.md` documents the split workflow; hand-editing `public/openapi.json` is called out as forbidden.
- [ ] `public/openapi/AGENTS.md` exists, contains all four agent-edit recipes (rename / append / remove / update description), states the hard rule, and links to `public/AGENTS.md`.
- [ ] `public/AGENTS.md` is in place as the public-surface index (done as a prerequisite, referenced from `public/openapi/AGENTS.md`).
- [ ] Rename and append recipes have been rehearsed successfully on a scratch branch (Dim 5.2, 5.3).
- [ ] Mintlify at `docs.usezombie.com/api-reference` renders identically before and after (manual spot-check of 5 endpoints).

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Split files parse cleanly and bundle
make openapi 2>&1 | tail -5; echo "bundle=$?"

# E2: Bundler idempotency
make openapi && cp public/openapi.json /tmp/first.json
make openapi && diff /tmp/first.json public/openapi.json
echo "idempotent=$?"  # must be 0

# E3: Content parity with pre-split version
jq -S . /tmp/pre-split-openapi.json > /tmp/old.canon
jq -S . public/openapi.json > /tmp/new.canon
diff /tmp/old.canon /tmp/new.canon
echo "parity=$?"  # must be 0

# E4: Router ↔ OpenAPI sync gate
make check-openapi-sync 2>&1 | tail -3; echo "sync=$?"

# E5: Synthetic drift detection (negative test — should fail loud)
# Temporarily add an unmatched route arm; sync gate should exit 1.
# (run manually; this is the PR-gate rehearsal)

# E6: Dead code sweep — old error-checker gone
grep -rn "check_openapi_errors" . --include="*.py" --include="Makefile" --include="*.mk" | head
# Expected: zero hits

# E7: Mintlify still reads the bundled artifact
curl -sI https://raw.githubusercontent.com/usezombie/usezombie/main/public/openapi.json | head -1
# Expected: HTTP 200
```

---

## Dead Code Sweep

**Status:** PENDING

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|---------------|----------------|
| `scripts/check_openapi_errors.py` | `test ! -f scripts/check_openapi_errors.py` |

**2. Orphaned references — zero remaining references.**

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| `check_openapi_errors` | `grep -rn "check_openapi_errors" . --include="*.py" --include="Makefile" --include="*.mk"` | 0 matches |
| `make check-openapi-errors` | `grep -rn "check-openapi-errors" . --include="Makefile" --include="*.mk" --include="*.yml"` | 0 matches |

---

## Out of Scope

- **Zig-native OpenAPI generation** (comptime reflection from handler types into OpenAPI schema) — separate future project. Only justified if drift still sneaks past the YAML + CI-gate approach after real usage. Not folded into this spec.
- **SDK generation** (Stainless / Speakeasy / OpenAPI Generator) — depends on this spec landing first. Separate workstream.
- **Migrating hand-written `api-reference/endpoint/*.mdx` pages** in the docs repo to auto-generated Mintlify pages — separate docs workstream.
- **Contract-test fuzzing** (Schemathesis, Dredd) — future observability workstream.
- **Pre-commit hook enforcement** of "never hand-edit `public/openapi.json`" — advisory only in this spec; can be promoted to a hard gate later if direct edits keep happening.

---

## Open Questions (must be resolved before CHORE open)

1. **Redocly vs swagger-cli** — pick one. Redocly has better lint + preview; swagger-cli is lighter. Recommendation: Redocly.
2. **Keep `public/openapi.json` committed?** Mintlify reads it from raw GitHub main, so yes — must be committed. Never gitignored.
3. **Mintlify cache TTL?** If `docs.usezombie.com` caches the spec, the post-merge API reference may lag the merge by minutes-to-hours. Verify and document in `docs/DOCS_RUNBOOK.md` (if it exists) or add a note to `api-reference/introduction.mdx`.
