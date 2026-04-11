---
Milestone: M17
Workstream: M17_001
Name: HARNESS_AGENT_PROFILE_TEARDOWN
Status: PENDING
Priority: P1 — removes ~2,400 lines of orphaned infrastructure with zero runtime consumers
Created: Apr 12, 2026
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
| `src/audit/profile_linkage.zig` | EVALUATE | FK to agent_config_versions — drop or decouple |
| `src/db/test_fixtures.zig` | MODIFY | Remove agent.* table fixture inserts |
| `src/db/test_fixtures_uc2.zig` | MODIFY | Remove harness_change_log references |
| `src/db/pool_test.zig` | MODIFY | Remove agent table references in migration tests |
| `src/main.zig` | MODIFY | Remove harness test discovery import |
| `schema/NNN_drop_harness_tables.sql` | CREATE | DROP TABLE migration for 4 agent tables |
| `ui/packages/website/src/components/FeatureFlow.tsx` | MODIFY | Remove harness marketing copy |
| `ui/packages/website/src/pages/Home.tsx` | MODIFY | Remove harness marketing copy |

## Applicable Rules

- RULE NDC — no dead code (this entire spec is NDC enforcement)
- RULE ORP — cross-layer orphan sweep after deletion
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files

## Sections

### §1.0 — Drop schema tables

| Dim | Status | Check |
|-----|--------|-------|
| 1.1 | PENDING | New migration drops `agent.workspace_active_config` (FK dep first) |
| 1.2 | PENDING | Migration drops `agent.agent_config_versions` |
| 1.3 | PENDING | Migration drops `agent.agent_profiles` |
| 1.4 | PENDING | Migration drops `agent.config_compile_jobs` |

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
| 4.2 | PENDING | `profile_linkage.zig` — evaluate: drop FK to agent_config_versions or delete file |
| 4.3 | PENDING | Handler entitlement error paths — remove `EntitlementProfileLimit` if harness-only |

### §5.0 — Clean tests + fixtures + frontend

| Dim | Status | Check |
|-----|--------|-------|
| 5.1 | PENDING | `router_test.zig` — remove harness route tests |
| 5.2 | PENDING | `rbac_http_integration_test.zig` — remove harness RBAC tests |
| 5.3 | PENDING | `test_fixtures*.zig` — remove agent.* table inserts |
| 5.4 | PENDING | `FeatureFlow.tsx`, `Home.tsx` — remove harness marketing copy |

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

**2. Orphaned references:**

| Symbol | Grep command | Expected |
|--------|-------------|----------|
| `harness_http` | `grep -rn "harness_http" src/ --include="*.zig"` | 0 |
| `harness_control_plane` | `grep -rn "harness_control_plane" src/ --include="*.zig"` | 0 |
| `agent_profiles` | `grep -rn "agent_profiles" src/ --include="*.zig"` | 0 |
| `workspace_active_config` | `grep -rn "workspace_active_config" src/ --include="*.zig"` | 0 |
| `countWorkspaceProfiles` | `grep -rn "countWorkspaceProfiles" src/ --include="*.zig"` | 0 |

## Acceptance Criteria

- [ ] `src/harness/` directory does not exist
- [ ] `src/http/handlers/harness_control_plane/` directory does not exist
- [ ] `src/http/handlers/harness_http.zig` does not exist
- [ ] Zero grep hits for `agent_profiles`, `harness_http`, `harness_control_plane` in src/
- [ ] New migration drops all 4 agent schema tables
- [ ] `zig build && zig build test` passes
- [ ] `make lint-zig` passes
- [ ] Cross-compiles for x86_64-linux and aarch64-linux
- [ ] Entitlement profile-limit enforcement removed or decoupled
- [ ] Frontend harness marketing copy updated

## Out of Scope

- Zombie system changes (M17_002 handles worker/executor alignment)
- Entitlement system redesign for zombies (future milestone)
- OpenAPI spec updates for removed endpoints (separate workstream)
- Schema file deletion (keep as version markers per RULES.md rule 41)
