# P1_API_CLI_INFRA_M30_001: Tear down `skill-secret` — deprecated per-skill credential surface

**Prototype:** v0.21.0
**Milestone:** M30
**Workstream:** 001
**Date:** Apr 19, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing. The `zombiectl skill-secret` command and its HTTP surface are currently shipped but no zombie flow exercises them. Removing dead product-surface reduces attack surface, reduces user confusion ("which credential command do I use?"), and removes a parallel table (`vault.workspace_skill_secrets`) from the pre-v2 schema.
**Batch:** B1 — standalone; no blocking deps on M29.
**Branch:** feat/m30-skill-secret-teardown (added when work begins)
**Depends on:** None. The tables and handlers can be removed directly in the pre-v2 teardown era.

---

## Overview

**Goal (testable):** After this workstream, `zombiectl skill-secret` does not exist, `/v1/workspaces/{workspace_id}/skills/{skill_ref}/secrets/{key}` returns 404, `vault.workspace_skill_secrets` is not present in the schema, and a fresh `make down && make up && make test-integration` passes. Zero references to `workspace_skill_secrets` or `skill-secret put/delete` survive in any non-historical file (`docs/v*/done/**` and `docs/changelog.mdx` are historical and stay untouched).

**Problem:** Three symptoms, one root cause.

1. **Two credential surfaces, one real use.** `zombiectl credential add` writes to `vault.secrets` (keyed by `(workspace_id, key_name)`); the zombie flow reads from `vault.secrets` via bare key names in `TRIGGER.md`. `zombiectl skill-secret put` writes to a parallel `vault.workspace_skill_secrets` (keyed by `(workspace_id, skill_ref, key_name)`); nothing reads it back in the zombie path. Grep confirms: the HTTP handler at `src/http/handlers/skill_secrets.zig`, the CLI subcommand at `zombiectl/src/commands/core-ops.js`, and the table writes at `src/secrets/crypto_store.zig` are all self-contained — no zombie-config parser, webhook handler, or event-loop worker reads the table.
2. **Schema drag.** `vault.workspace_skill_secrets` carries six KEK fields per row. Every migration re-run against a fresh database provisions the table. For pre-v2 teardown we want zero dead tables. Per the Schema Table Removal Guard, removal is: delete the `CREATE TABLE` block from `schema/004_vault_schema.sql`, remove any references from `schema/embed.zig`, remove any migration array entry in `src/cmd/common.zig`.
3. **Documentation drag.** `cli/zombiectl.mdx` in the docs repo currently carries a disclaimer-laden section explaining how `skill-secret` is "distinct from and unrelated to the standard zombie credential flow". That is the prose form of this exact problem — the surface exists but shouldn't. Remove the command, remove the section.

**Solution summary:** One commit on `feat/m30-skill-secret-teardown` that (a) deletes the `vault.workspace_skill_secrets` table definition from `schema/004_vault_schema.sql` per the Schema Table Removal Guard, (b) deletes `src/http/handlers/skill_secrets.zig` and unwires it from the route table, (c) deletes `zombiectl/src/commands/core-ops.js`'s `skill-secret` branches and the route entry in the CLI command registry, (d) deletes `src/secrets/crypto_store.zig`'s INSERT/DELETE queries for the table (and related functions if they become dead), (e) updates the docs repo in a companion PR to remove the `skill-secret` section from `cli/zombiectl.mdx`.

---

## Files Changed (blast radius)

Usezombie repo (`<usezombie worktree>`):

| File | Action | Why |
|------|--------|-----|
| `schema/004_vault_schema.sql` | MODIFY | Delete the `CREATE TABLE vault.workspace_skill_secrets` block and its grants. Per Schema Guard: pre-v2, no ALTER/DROP ceremony. |
| `src/secrets/crypto_store.zig` | MODIFY | Delete INSERT/DELETE SQL for `vault.workspace_skill_secrets` and any helper functions that become dead. |
| `src/http/handlers/skill_secrets.zig` | DELETE | HTTP handler for `/v1/workspaces/{id}/skills/{ref}/secrets/{key}`. |
| `src/http/handlers/<route-table>.zig` | MODIFY | Unwire the skill_secrets route. |
| `src/http/rbac_http_integration_test.zig` | MODIFY | Remove skill_secret_url test fixtures (lines ≈ 296; full integration body that touches the table). |
| `src/db/pool_test.zig` | MODIFY | Remove `{ .schema_name = "vault", .table_name = "workspace_skill_secrets" }` from the expected-tables set (line ≈ 369). |
| `src/errors/error_registry_test.zig` | MODIFY | Remove any error code that only applies to skill_secrets. |
| `zombiectl/src/commands/core-ops.js` | MODIFY | Remove `skill-secret put` / `skill-secret delete` action branches. |
| `zombiectl/src/program/command-registry.js` | MODIFY | Remove `skill-secret` route entry. |
| `zombiectl/src/program/routes.js` | MODIFY | Remove `skill-secret` route if distinct from registry. |
| `zombiectl/test/` | MODIFY | Remove any `skill-secret` unit tests. |
| `public/openapi.json` | MODIFY | Remove paths and schemas tagged with the skill_secrets operation. |
| `docs/v2/pending/P1_API_CLI_INFRA_M30_001_SKILL_SECRET_TEARDOWN.md` | MOVE | pending → active → done per spec lifecycle. |

Docs repo (`<docs worktree>`) — companion PR:

| File | Action | Why |
|------|--------|-----|
| `cli/zombiectl.mdx` | MODIFY | Remove the `### zombiectl skill-secret` section under Operator commands. Remove the entry from the overview table. |
| `changelog.mdx` | MODIFY | New `<Update>` entry under the next usezombie version bump, tagged `Breaking` + `CLI` + `API`. |

## Applicable Rules

- **Schema Table Removal Guard** — pre-v2 (0.21.0 ≪ 2.0.0). Removal path: delete `CREATE TABLE` block from the SQL file (no `DROP TABLE` / `ALTER TABLE` / `SELECT 1;` ceremony). Re-print the guard output in the first commit message touching `schema/004_vault_schema.sql`.
- **RULE ORP (orphan sweep)** — after code removal, grep confirms zero `workspace_skill_secrets` or `skill-secret` references in non-historical files.
- **RULE FLL (350-line gate)** — `src/http/handlers/skill_secrets.zig` is being deleted outright; any file that loses lines must remain under 350.
- **zig-pg-drain** — any modified `*.zig` file using `conn.query()` must `.drain()` in the same function before `deinit()`. `make check-pg-drain` hard gate.
- **Cross-compile** — `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` after any `*.zig` edit.

---

## Sections (implementation slices)

### §1 — Schema teardown

**Status:** PENDING

Delete the `vault.workspace_skill_secrets` table from `schema/004_vault_schema.sql` (lines ≈ 77–106 per today's layout). Re-print the Schema Guard output in the commit message. Grants referencing the table disappear with the block.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `schema/004_vault_schema.sql` | after commit | no `CREATE TABLE vault.workspace_skill_secrets` block | contract (grep) |
| 1.2 | PENDING | `make down && make up` | fresh DB provision | completes with no reference to the dropped table | integration |
| 1.3 | PENDING | `src/db/pool_test.zig` | expected-tables set | no `workspace_skill_secrets` entry | unit |

### §2 — HTTP handler + route teardown

**Status:** PENDING

Delete `src/http/handlers/skill_secrets.zig` outright. Remove the route registration wherever it lives (router table, serve_webhook_lookup, middleware stack). Remove any error codes in `src/errors/error_registry_test.zig` that are unique to this handler. Delete the INSERT/DELETE SQL blocks in `src/secrets/crypto_store.zig` and any helper functions they touched (sweep for dead callers).

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `src/http/handlers/skill_secrets.zig` | filesystem | file does not exist | contract |
| 2.2 | PENDING | `curl -X PUT /v1/workspaces/{id}/skills/{ref}/secrets/{key}` | any request | `404 Not Found` | integration |
| 2.3 | PENDING | `rg -l workspace_skill_secrets src/` | full source tree | no matches | unit (grep) |
| 2.4 | PENDING | `make memleak` | server lifecycle after route removal | zero new leaks | integration |

### §3 — CLI teardown

**Status:** PENDING

Delete the `skill-secret put` / `skill-secret delete` action branches in `zombiectl/src/commands/core-ops.js`. Remove the route in the registry. Remove matching unit tests.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `zombiectl skill-secret put ...` | any invocation | `unknown command` error, exit 2 | unit |
| 3.2 | PENDING | `zombiectl --help` | output | no `skill-secret` line | unit |
| 3.3 | PENDING | `rg -l skill-secret zombiectl/` | full tree | no matches | unit (grep) |
| 3.4 | PENDING | `cd zombiectl && bun test` | full suite | pass, no regressions | unit |

### §4 — OpenAPI + docs

**Status:** PENDING

Remove skill_secrets paths/schemas from `public/openapi.json`. In the companion docs PR, delete the `### zombiectl skill-secret` section from `cli/zombiectl.mdx` and the overview table entry. Add a `<Update>` changelog block tagged `Breaking` + `CLI` + `API`.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `public/openapi.json` | validated | no `skill_secret` tag or paths | contract |
| 4.2 | PENDING | docs-repo `make lint` | full run | exit 0 | integration |
| 4.3 | PENDING | docs-repo `cli/zombiectl.mdx` | file | no `skill-secret` references | contract (grep) |
| 4.4 | PENDING | docs-repo `changelog.mdx` | file | new `<Update>` block tagged `Breaking` + `CLI` + `API` | contract |

---

## Interfaces

**Status:** PENDING

N/A — this workstream is entirely deletion; no new interfaces. The deleted interfaces, for the record:

- CLI: `zombiectl skill-secret put|delete --workspace-id ... --skill-ref ... --key ... [--value ... --scope ...]` — deleted.
- HTTP: `PUT /v1/workspaces/{workspace_id}/skills/{skill_ref}/secrets/{key}`, `DELETE /v1/workspaces/{workspace_id}/skills/{skill_ref}/secrets/{key}` — deleted; future requests 404.
- DB: `vault.workspace_skill_secrets` table — deleted; queries fail with `relation does not exist` (expected — no caller).

### Error Contracts

| Error condition | Behavior | Caller sees |
|-----------------|----------|-------------|
| Client hits deleted `PUT/DELETE /v1/workspaces/{id}/skills/{ref}/secrets/{key}` | Route not registered | `404 Not Found`, no body (pre-v2.0 — no 410 stubs per `feedback_pre_v2_api_drift`) |
| Client invokes deleted `zombiectl skill-secret` | CLI registry has no entry | exit 2, `unknown command: skill-secret` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Operator has stored skill-scoped secrets today | Data exists in `vault.workspace_skill_secrets` at the moment §1 drops the table | Secrets are destroyed with the table. Pre-v2.0, this is the accepted teardown cost. | Any downstream operator tooling that relied on these secrets fails. Recovery: store as workspace credentials via `zombiectl credential add`. |
| External operator tooling calls the deleted HTTP endpoint | Someone's script hits PUT/DELETE on the old URL | 404 | Script fails loudly. Migration path: use `POST /v1/workspaces/{id}/credentials` (the workspace vault surface). |
| Changelog entry omitted | Author skips §4.3 | docs/changelog.mdx carries no breaking-change announcement | Users miss the removal until they hit the 404 or `unknown command`. CI gate: the docs CHORE(close) diff must include a new `<Update>` block. |

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Zero `workspace_skill_secrets` hits in non-historical files | `rg -l workspace_skill_secrets -- . ':!docs/v*/done' ':!docs/changelog.mdx'` returns 0 matches |
| Zero `skill-secret` (CLI form) hits in non-historical files | `rg -l 'skill-secret' -- . ':!docs/v*/done' ':!docs/changelog.mdx'` returns 0 matches |
| Cross-compile clean | `zig build -Dtarget=x86_64-linux` and `zig build -Dtarget=aarch64-linux` both exit 0 |
| `make check-pg-drain` green on every `*.zig` touched | pre-push hook |
| `make test`, `make test-integration` green | pre-push hook |
| Docs-repo `make lint` green | docs-side CI |
| Schema Guard output printed in the commit touching `schema/004_vault_schema.sql` | manual inspection of commit message |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | `vault.workspace_skill_secrets` does not exist after this workstream | `psql -c "\dt vault.workspace_skill_secrets"` returns "Did not find any relation" |
| 2 | No source file outside `docs/v*/done/` + `docs/changelog.mdx` references `workspace_skill_secrets` or `skill-secret` | CI grep lint |
| 3 | The zombie credential flow (`zombiectl credential add` / `vault.secrets` / bare-name refs in `TRIGGER.md`) remains untouched and passing | `make test-integration` |

---

## Test Specification

**Status:** PENDING

### Contract Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| table does not exist | 1.1 | `schema/004_vault_schema.sql` | file | no `CREATE TABLE vault.workspace_skill_secrets` |
| handler file deleted | 2.1 | filesystem | `ls src/http/handlers/skill_secrets.zig` | `No such file or directory` |
| no source refs | 2.3, 3.3 | full tree | `rg -l workspace_skill_secrets` + `rg -l skill-secret` | 0 matches outside historical dirs |
| openapi clean | 4.1 | `public/openapi.json` | jq query for skill_secret tag | empty |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|--------------|-------|----------|
| fresh DB provision succeeds | 1.2 | docker compose | `make down && make up` | exit 0 |
| HTTP endpoint 404s | 2.2 | running server | PUT `/v1/workspaces/{id}/skills/{ref}/secrets/{key}` | `404`, no body |
| zombie credential flow unaffected | invariant #3 | running server | `zombiectl credential add` + `zombiectl up` with a lead-collector template | zombie deploys; event processing reads from `vault.secrets` as before |
| docs make lint green | 4.2 | node 24 | `make lint` in docs repo | exit 0 |

### Negative Tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| `zombiectl skill-secret put ...` rejected | 3.1 | any args | exit 2, `unknown command` |
| `zombiectl skill-secret --help` rejected | 3.1 | help request | exit 2, no help text for a non-existent command |
| PUT on deleted route | 2.2 | any body | 404 |

### Regression Tests

| Test name | What it guards | File |
|-----------|---------------|------|
| `zombiectl credential add/list` unaffected | standard zombie credential flow | `zombiectl/test/zombie.unit.test.js` |
| `vault.secrets` table writes/reads unaffected | workspace credential path | `src/secrets/crypto_store_test.zig` (if exists) |
| `zombiectl up` for `lead-collector` + `slack-bug-fixer` templates end-to-end | template regression | existing integration tests under `src/http/` |

### Leak Detection Tests

Run `make memleak` after removing the HTTP handler to ensure no allocator is left dangling on the route registration path.

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "`zombiectl skill-secret` does not exist" | `zombiectl skill-secret put ...` exit 2 + `rg -l skill-secret zombiectl/` empty | unit + contract |
| "`/v1/workspaces/.../skills/.../secrets/...` returns 404" | integration test against running server | integration |
| "`vault.workspace_skill_secrets` not in schema" | `\dt` + `rg -l workspace_skill_secrets` | contract + integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | CHORE(open): move spec pending → active, branch `feat/m30-skill-secret-teardown`, worktree at `../usezombie-m30-skill-secret-teardown`. | `pwd` inside the worktree, spec on IN_PROGRESS, committed |
| 2 | Schema teardown (§1). Print Schema Guard output. Remove `CREATE TABLE vault.workspace_skill_secrets` from `schema/004_vault_schema.sql`. Update `src/db/pool_test.zig` expected-tables set. | `make down && make up && make test-integration` passes on fresh DB |
| 3 | Handler + route teardown (§2). Delete `src/http/handlers/skill_secrets.zig`, unwire route, purge `src/secrets/crypto_store.zig` queries. | `rg -l workspace_skill_secrets src/` empty. `make memleak` clean. |
| 4 | CLI teardown (§3). Remove branches in `zombiectl/src/commands/core-ops.js`, registry entry, unit tests. | `cd zombiectl && bun test` passes. `rg -l 'skill-secret' zombiectl/` empty. |
| 5 | OpenAPI + integration test regression sweep. Update `public/openapi.json`. Run `make check-pg-drain`, cross-compile both targets. | All green. |
| 6 | Companion docs PR: delete `### zombiectl skill-secret` section from `cli/zombiectl.mdx`, add `<Update>` changelog block. | `make lint` in docs repo exits 0. |
| 7 | CHORE(close): mark all dimensions DONE, move spec active → done, Ripley's Log in `docs/nostromo/`. Update VERSION (minor bump for pre-v2 breaking). | `git status` clean; spec in done/; PR opened on each repo. |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `schema/004_vault_schema.sql` no longer contains `vault.workspace_skill_secrets` — verify: `rg workspace_skill_secrets schema/` returns 0
- [ ] `src/http/handlers/skill_secrets.zig` does not exist — verify: `test ! -f src/http/handlers/skill_secrets.zig`
- [ ] `zombiectl skill-secret` exits 2 with `unknown command` — verify: run it
- [ ] `rg -l workspace_skill_secrets -- . ':!docs/v*/done' ':!docs/changelog.mdx'` → 0
- [ ] `rg -l 'skill-secret' -- . ':!docs/v*/done' ':!docs/changelog.mdx'` → 0
- [ ] `make test && make test-integration` pass
- [ ] `make memleak` clean
- [ ] `make check-pg-drain` clean
- [ ] Cross-compile both Linux targets clean
- [ ] Docs-repo `make lint` exit 0
- [ ] Changelog `<Update>` block tagged `Breaking` + `CLI` + `API`
- [ ] VERSION bumped (minor, pre-v2)

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Source tree clean of the dropped surface
rg -l workspace_skill_secrets -- . ':!docs/v*/done' ':!docs/changelog.mdx' | wc -l   # expect 0
rg -l 'skill-secret' -- . ':!docs/v*/done' ':!docs/changelog.mdx' | wc -l           # expect 0

# E2: Handler file gone
test ! -f src/http/handlers/skill_secrets.zig && echo PASS || echo FAIL

# E3: CLI command gone
zombiectl skill-secret put --workspace-id x --skill-ref y --key z --value w 2>&1 | grep -q 'unknown command' && echo PASS || echo FAIL

# E4: Schema table gone
psql "$HANDLER_DB_TEST_URL" -c '\dt vault.workspace_skill_secrets' 2>&1 | grep -q 'Did not find any relation' && echo PASS || echo FAIL

# E5: Full test suites
make test && make test-integration && echo PASS || echo FAIL
make memleak && echo PASS || echo FAIL

# E6: Docs lint (in the docs repo worktree — set DOCS_REPO_ROOT to your checkout,
# or rely on the sibling-worktree default `../docs`)
(cd "${DOCS_REPO_ROOT:-../docs}" && make lint) && echo PASS || echo FAIL
```

---

## Dead Code Sweep

**Status:** PENDING

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|---------------|----------------|
| `src/http/handlers/skill_secrets.zig` | `test ! -f src/http/handlers/skill_secrets.zig` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| table `vault.workspace_skill_secrets` | `rg -l workspace_skill_secrets` | 0 matches outside historical dirs |
| CLI command `skill-secret` | `rg -l 'skill-secret'` | 0 matches outside historical dirs |
| handler symbol from `skill_secrets.zig` | follow compile errors | 0 |

**3. main.zig test discovery — update imports.**

After deleting `src/http/handlers/skill_secrets.zig`, remove any `_ = @import("skill_secrets.zig")` from `src/main.zig` or its sub-discovery modules.

---

## Out of Scope

- Migration path for operators currently using `zombiectl skill-secret` — there is no expected live user (grep confirms no product-level caller). If an operator is using it and hasn't told us, they will learn via the 404 / `unknown command` error; the documented recovery is to use `zombiectl credential add` against `vault.secrets` with the same key name.
- Introducing a new skill-scoped credential surface. If multi-skill-same-name-different-value is a real future requirement, that is a new workstream with a new design — do not resurrect `vault.workspace_skill_secrets`.
- Any change to `vault.secrets` (the standard zombie credential vault). That table stays.
