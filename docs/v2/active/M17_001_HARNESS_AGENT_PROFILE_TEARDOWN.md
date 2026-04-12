---
Milestone: M17
Workstream: M17_001
Name: HARNESS_AGENT_PROFILE_TEARDOWN
Status: IN_PROGRESS
Priority: P1 — removes ~2,400 lines of orphaned infrastructure with zero runtime consumers
Created: Apr 12, 2026
Started: Apr 12, 2026
Branch: feat/m17-001-harness-teardown
Depends on: none
---

# M17_001 — Harness & Agent Profile Dead Code Teardown

## Overview

**Goal (testable):** After this workstream, `grep -rn "agent_profile\|harness_http\|harness_control_plane\|workspace_active_config\|config_compile_job" src/ --include="*.zig"` returns 0 matches, all 4 agent schema tables are dropped, and `zig build && zig build test` passes.

**Problem:** The zombie system (M2_002+) took a clean path with its own config in `core.zombies` (inline `config_json`, `source_markdown`, `trigger_markdown`). The entire harness compilation pipeline (`agent.agent_profiles` → `agent_config_versions` → `workspace_active_config` → `config_compile_jobs`) plus HTTP handlers (`/harness/*`) and core logic (`src/harness/`) has zero runtime consumers. It's ~2,400 lines of dead infrastructure that increases build time, confuses new contributors, and creates false grep hits during debugging.

**Solution summary:** Delete the harness control plane (handlers, core logic, routes), drop the 4 agent schema tables via a new migration, clean entitlement profile-limit enforcement (harness-only), remove test fixtures and marketing copy referencing harness, and sweep all orphaned references.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/harness/control_plane.zig` | DELETE | Dead harness compilation logic (340L) |
| `src/harness/control_plane_test.zig` | DELETE | Tests for dead code (349L) |
| `src/http/handlers/harness_http.zig` | DELETE | Dead HTTP handler facade (263L) |
| `src/http/handlers/harness_control_plane.zig` | DELETE | Dead handler re-export (23L) |
| `src/http/handlers/harness_control_plane/activate.zig` | DELETE | Dead activate handler (119L) |
| `src/http/handlers/harness_control_plane/compile.zig` | DELETE | Dead compile handler (192L) |
| `src/http/handlers/harness_control_plane/get_active.zig` | DELETE | Dead get-active handler (50L) |
| `src/http/handlers/harness_control_plane/put_source.zig` | DELETE | Dead source upload handler (73L) |
| `src/http/handlers/harness_control_plane/tests.zig` | DELETE | Dead handler tests (203L) |
| `src/http/handlers/harness_control_plane/tests_extended.zig` | DELETE | Dead extended tests (383L) |
| `src/http/handlers/harness_control_plane/types.zig` | DELETE | Dead type defs (60L) |
| `src/http/handlers/harness_control_plane/util.zig` | DELETE | Dead utils (54L) |
| `src/http/handler.zig` | MODIFY | Remove harness handler imports + exports |
| `src/http/router.zig` | MODIFY | Remove harness route enum fields + matchers |
| `src/http/server.zig` | MODIFY | Remove harness route dispatch arms |
| `src/http/router_test.zig` | MODIFY | Remove harness route test cases |
| `src/http/rbac_http_integration_test.zig` | MODIFY | Remove harness RBAC test cases |
| `src/http/handlers/m5_handler_changes_test.zig` | MODIFY | Remove harness import resolution test |
| `src/state/entitlements.zig` | MODIFY | Remove profile-limit enforcement (harness-only) |
| `src/audit/profile_linkage.zig` | DELETE | FK to agent_config_versions — entire file is harness-only audit |
| `src/db/test_fixtures.zig` | MODIFY | Remove agent.* table fixture inserts |
| `src/db/test_fixtures_uc2.zig` | MODIFY | Remove harness_change_log references |
| `src/db/pool_test.zig` | MODIFY | Remove agent table references in migration tests |
| `src/main.zig` | MODIFY | Remove harness test discovery import |
| `schema/008_harness_control_plane.sql` | DELETE | Pre-v2.0 full teardown (RULE SCH); removes agent_profiles, agent_config_versions, workspace_active_config, config_compile_jobs |
| `schema/011_profile_linkage_audit.sql` | DELETE | Pre-v2.0 full teardown; removes config_linkage_audit_artifacts |
| `schema/009_rls_tenant_isolation.sql` | MODIFY | Strip agent.* ALTER TABLE + POLICY lines; keep core.prompt_lifecycle_events + vault.workspace_skill_secrets |
| `schema/embed.zig` | MODIFY | Remove `harness_control_plane_sql` + `profile_linkage_audit_sql` constants |
| `src/cmd/common.zig` | MODIFY | Remove versions 8 and 11 migration array entries; shrink array `[21]` → `[19]` |
| `src/db/pool_test.zig` | MODIFY | Remove obsolete `migrations[6]` assertion on `CREATE TABLE harness_change_log` (indexes shift) |
| `ui/packages/website/src/components/FeatureFlow.tsx` | MODIFY | Remove harness marketing copy |
| `ui/packages/website/src/pages/Home.tsx` | MODIFY | Remove harness marketing copy |
| `zombiectl/src/commands/harness.js` | DELETE | Dead CLI command (calls removed /harness/* endpoints) |
| `zombiectl/src/commands/harness_activate.js` | DELETE | Dead CLI command |
| `zombiectl/src/commands/harness_active.js` | DELETE | Dead CLI command |
| `zombiectl/src/commands/harness_compile.js` | DELETE | Dead CLI command |
| `zombiectl/src/commands/harness_source.js` | DELETE | Dead CLI command |
| `zombiectl/src/commands/agent_harness.js` | DELETE | Dead CLI command (agent harness revert) |
| `zombiectl/test/harness_*.unit.test.js` + `harness-*.test.js` + `agent_harness.unit.test.js` | DELETE | 9 dead test files |
| `zombiectl/src/cli.js` | MODIFY | Remove harness import + routing case |
| `zombiectl/src/program/command-registry.js` | MODIFY | Remove harness handler entry |
| `zombiectl/src/program/routes.js` | MODIFY | Remove "harness" route entry |
| `zombiectl/src/program/suggest.js` | MODIFY | Remove "harness" top-level + subcommand completions |
| `zombiectl/src/program/io.js` | MODIFY | Remove 5 harness help lines |
| `zombiectl/src/commands/agent.js` | MODIFY | Remove "agent harness revert" action block + usage line + import |

## Applicable Rules

- RULE NDC — no dead code (this entire spec is NDC enforcement)
- RULE ORP — cross-layer orphan sweep after deletion
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files

## Sections

### §1.0 — Schema teardown (pre-v2.0, full teardown — RULE SCH)

Pre-v2.0 (`VERSION=0.5.0`) teardown-rebuild era: no production data to protect,
no `ALTER`, no `DROP TABLE`, no `SELECT 1;` markers, no version-marker files.
Remove tables fully by deleting the SQL file + embed constant + migration array
entry. Slot numbers are not sacred; gaps are fine because the DB is wiped on
every rebuild. FK drop order is irrelevant because the entire CREATE set is
removed as source code.

| Dim | Status | Check |
|-----|--------|-------|
| 1.1 | PENDING | `schema/008_harness_control_plane.sql` deleted (removes agent_profiles, agent_config_versions, workspace_active_config, config_compile_jobs) |
| 1.2 | PENDING | `schema/011_profile_linkage_audit.sql` deleted (removes config_linkage_audit_artifacts) |
| 1.3 | PENDING | `schema/009_rls_tenant_isolation.sql` — agent.* ALTER TABLE + POLICY lines removed; core.prompt_lifecycle_events + vault.workspace_skill_secrets retained |
| 1.4 | PENDING | `schema/embed.zig` — `harness_control_plane_sql` and `profile_linkage_audit_sql` constants removed |
| 1.5 | PENDING | `src/cmd/common.zig` `canonicalMigrations` — entries for versions 8 and 11 removed; array length shrunk from `[21]` to `[19]` |
| 1.6 | PENDING | `src/db/pool_test.zig` — obsolete `migrations[6]` assertion on harness `CREATE TABLE` removed (indexes shift after array shrink) |

### §2.0 — Delete harness code

| Dim | Status | Check |
|-----|--------|-------|
| 2.1 | PENDING | `src/harness/` directory deleted (2 files, 689L) |
| 2.2 | PENDING | `src/http/handlers/harness_http.zig` deleted (263L) |
| 2.3 | PENDING | `src/http/handlers/harness_control_plane.zig` deleted (23L) |
| 2.4 | PENDING | `src/http/handlers/harness_control_plane/` directory deleted (8 files, 1,134L) |

### §3.0 — Clean route dispatch + imports

| Dim | Status | Check |
|-----|--------|-------|
| 3.1 | PENDING | `handler.zig` — remove 4 harness handler imports + pub exports |
| 3.2 | PENDING | `router.zig` — remove 4 route enum fields + matchers |
| 3.3 | PENDING | `server.zig` — remove 4 harness dispatch arms |
| 3.4 | PENDING | `main.zig` — remove harness test discovery import |

### §4.0 — Clean entitlements + audit

| Dim | Status | Check |
|-----|--------|-------|
| 4.1 | PENDING | `entitlements.zig` — remove `countWorkspaceProfiles()` and profile-limit enforcement |
| 4.2 | PENDING | `profile_linkage.zig` — delete file (harness-only audit, table dropped in §1.1) |
| 4.3 | PENDING | Handler entitlement error paths — remove `EntitlementProfileLimit` if harness-only |

### §5.0 — Clean tests + fixtures + frontend

| Dim | Status | Check |
|-----|--------|-------|
| 5.1 | PENDING | `router_test.zig` — remove harness route tests |
| 5.2 | PENDING | `rbac_http_integration_test.zig` — remove harness RBAC tests |
| 5.3 | PENDING | `test_fixtures*.zig` — remove agent.* table inserts |
| 5.4 | PENDING | `FeatureFlow.tsx`, `Home.tsx` — remove harness marketing copy |

### §6.0 — Delete zombiectl harness CLI surface

Server-side `/harness/*` endpoints are being removed; every zombiectl harness
command is a dead client. Ships together with server removal so the CLI never
calls 404 routes.

| Dim | Status | Check |
|-----|--------|-------|
| 6.1 | PENDING | 6 command files deleted (`harness.js`, `harness_activate.js`, `harness_active.js`, `harness_compile.js`, `harness_source.js`, `agent_harness.js`) |
| 6.2 | PENDING | 9 harness test files deleted under `zombiectl/test/` |
| 6.3 | PENDING | `cli.js` — `commandHarnessModule` import + routing case removed |
| 6.4 | PENDING | `command-registry.js` — `harness: handlers.harness` entry removed |
| 6.5 | PENDING | `routes.js` — `{ key: "harness", ... }` entry removed |
| 6.6 | PENDING | `suggest.js` — `"harness"` top-level + subcommand completions removed |
| 6.7 | PENDING | `io.js` — 5 harness help lines removed |
| 6.8 | PENDING | `agent.js` — `commandAgentHarness` import + `action === "harness"` block + usage line removed |
| 6.9 | PENDING | `grep -rn "harness" zombiectl/src/ --include="*.js"` returns 0 |
| 6.10 | PENDING | `cd zombiectl && npm test` passes |

## Eval Commands

```bash
# E1: No harness files exist
test ! -d src/harness && echo "PASS" || echo "FAIL: src/harness still exists"
test ! -f src/http/handlers/harness_http.zig && echo "PASS" || echo "FAIL"
test ! -d src/http/handlers/harness_control_plane && echo "PASS" || echo "FAIL"

# E2: Zero harness references in Zig source
count=$(grep -rn "harness_http\|harness_control_plane\|control_plane\.zig" src/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS" || echo "FAIL: $count stale refs"

# E3: Zero agent_profile table references (excluding schema version markers)
count=$(grep -rn "agent_profiles\|agent_config_versions\|workspace_active_config\|config_compile_jobs" src/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS" || echo "FAIL: $count stale refs"

# E4: Build + test
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"

# E5: Lint
make lint-zig 2>&1 | grep -E "✓|FAIL"

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E7: Gitleaks
gitleaks detect 2>&1 | tail -3

# E8: Line count on deleted code
echo "Lines removed: ~2400"
```

## Dead Code Sweep

**1. Orphaned files:**

| File/directory to delete | Verify |
|--------------------------|--------|
| `src/harness/` | `test ! -d src/harness` |
| `src/http/handlers/harness_http.zig` | `test ! -f src/http/handlers/harness_http.zig` |
| `src/http/handlers/harness_control_plane.zig` | `test ! -f src/http/handlers/harness_control_plane.zig` |
| `src/http/handlers/harness_control_plane/` | `test ! -d src/http/handlers/harness_control_plane` |
| `src/audit/profile_linkage.zig` | `test ! -f src/audit/profile_linkage.zig` |

**2. Orphaned references:**

| Symbol | Grep command | Expected |
|--------|-------------|----------|
| `harness_http` | `grep -rn "harness_http" src/ --include="*.zig"` | 0 |
| `harness_control_plane` | `grep -rn "harness_control_plane" src/ --include="*.zig"` | 0 |
| `agent_profiles` | `grep -rn "agent_profiles" src/ --include="*.zig"` | 0 |
| `workspace_active_config` | `grep -rn "workspace_active_config" src/ --include="*.zig"` | 0 |
| `config_linkage_audit` | `grep -rn "config_linkage_audit" src/ --include="*.zig"` | 0 |
| `profile_linkage` | `grep -rn "profile_linkage" src/ --include="*.zig"` | 0 |
| `countWorkspaceProfiles` | `grep -rn "countWorkspaceProfiles" src/ --include="*.zig"` | 0 |

## Acceptance Criteria

- [ ] `src/harness/` directory does not exist
- [ ] `src/http/handlers/harness_control_plane/` directory does not exist
- [ ] `src/http/handlers/harness_http.zig` does not exist
- [ ] Zero grep hits for `agent_profiles`, `harness_http`, `harness_control_plane` in src/
- [ ] New migration drops all 5 agent schema tables (audit_artifacts, active_config, compile_jobs, config_versions, profiles) in correct FK order
- [ ] `zig build && zig build test` passes
- [ ] `make lint-zig` passes
- [ ] Cross-compiles for x86_64-linux and aarch64-linux
- [ ] Entitlement profile-limit enforcement removed or decoupled
- [ ] `profile_linkage.zig` FK to `agent_config_versions` resolved (drop FK or delete file) — verify: `grep -rn "agent_config_versions" src/ --include="*.zig"` returns 0
- [ ] Frontend harness marketing copy updated
- [ ] All zombiectl harness commands + tests deleted; `grep -rn "harness" zombiectl/src/ --include="*.js"` returns 0
- [ ] `cd zombiectl && npm test` passes

## Out of Scope

- Zombie system changes (M17_002 handles worker/executor alignment)
- Entitlement system redesign for zombies (future milestone)
- OpenAPI spec updates for removed endpoints (separate workstream)
- Schema file deletion (keep as version markers per RULES.md rule 41)
