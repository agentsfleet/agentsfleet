# M9_000: Agent ID And Config Version Rename

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 000
**Date:** Mar 14, 2026
**Completed:** Mar 15, 2026
**Status:** DONE
**Priority:** P0 — prerequisite rename required before the rest of M9 can align on agent terminology
**Batch:** B0 — must land before M9 persistence, context injection, and auto-improvement workstreams
**Depends on:** Nothing

---

## 1.0 Schema Rename

**Status:** DONE

Rename the existing harness-control-plane identifiers from profile-oriented names to agent/config-oriented names.

**Dimensions:**
- 1.1 DONE Rename `agent_profiles.profile_id` → `agent_id` and keep it as the primary key
- 1.2 DONE Rename `agent_profile_versions` → `agent_config_versions`
- 1.3 DONE Rename `agent_profile_versions.profile_version_id` → `config_version_id` and `agent_profile_versions.profile_id` → `agent_id`
- 1.4 DONE Rename downstream FK columns: `workspace_active_profile.profile_version_id` → `config_version_id`, `profile_compile_jobs.requested_profile_id` → `requested_agent_id`, and all other M9-referenced config-version links

---

## 2.0 Source Code Rename

**Status:** DONE

Update Zig and CLI code to use the new names consistently without leaving mixed terminology in active paths.

**Dimensions:**
- 2.1 DONE Update harness control plane queries, types, and handler responses to use `agent_id` / `config_version_id`
- 2.2 DONE Update worker/runtime paths that currently pass `profile_id` or `profile_version_id`
- 2.3 DONE Update CLI payload keys, display labels, and parsing that currently use profile terminology
- 2.4 DONE Keep public API naming coherent: user-facing M9 routes and payloads say `agent`, not `profile`

---

## 3.0 Verification

**Status:** DONE

Prove the rename is complete and does not leave broken FK references or mixed contracts.

**Dimensions:**
- 3.1 DONE Canonical migrations bootstrap cleanly with the renamed schema
- 3.2 DONE Harness compile/activate/active flows pass against renamed columns and tables — 53/53 CLI tests pass, `zig build` clean
- 3.3 DONE Grep-based verification shows no remaining active-path `profile_id` / `profile_version_id` references in `src/`, `zombiectl/src/`, or `config/` — only `ProfileDoc.profile_id` (JSON deserialisation struct, not a harness-control-plane table) and test fixture JSON strings remain
- 3.4 DONE M9_002 spec already references M9_000 as prerequisite source of truth

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 The database schema no longer uses `profile_id` or `profile_version_id` in active harness-control-plane tables
- [x] 4.2 Active source code paths use `agent_id` and `config_version_id` consistently
- [x] 4.3 Existing harness workflows still work after the rename — build clean, 53/53 tests pass
- [x] 4.4 M9_001–M9_004 can reference this spec instead of carrying rename details inline

---

## 5.0 Completion Notes

**Mar 15, 2026: Delivered on branch `feat/m9-000-rename`.**

### Schema (already complete before this workstream)
- `schema/006_harness_control_plane.sql`: `agent_profiles.agent_id`, `agent_config_versions`, `config_version_id`, `workspace_active_config`, `config_compile_jobs.requested_agent_id` — all correct
- `schema/007_rls_tenant_isolation.sql`: RLS policies use new table names throughout
- `schema/009_profile_linkage_audit.sql`: `config_linkage_audit_artifacts` with `config_version_id` FK

### Zig (27 files, 207 substitutions)
- `src/pipeline/topology.zig`: `Profile.profile_id` → `agent_id`
- `src/pipeline/worker_claim.zig`: `effective_profile.profile_id` → `agent_id`
- `src/pipeline/worker.zig`: log labels updated
- `src/harness/control_plane.zig`: `CompileOutcome` / `ProfileOut` struct fields
- `src/harness/control_plane/util.zig`: `generateProfileId` → `generateAgentId`
- `src/types/id_format.zig`: `generateProfileId` → `generateAgentId`
- `src/observability/posthog_events.zig`: event struct fields and property keys
- `src/state/entitlements.zig`: `Observed.profile_version_id` → `config_version_id`
- `src/audit/profile_linkage.zig`: temp table refs → `agent_config_versions`, `config_compile_jobs`
- `src/db/pool.zig`: temp table init — `agent_config_versions`, `config_compile_jobs`, `workspace_active_config`
- `src/http/handlers/runs/start.zig` + `get.zig`: SQL table/column refs
- `src/pipeline/worker_profile_tests.zig` + `src/http/handlers/harness_control_plane/tests.zig`: test fixture temp tables and column names

### CLI JS
- `harness_compile.js`, `harness_source.js`, `harness_activate.js`, `harness_active.js`: request body keys and output labels
- All corresponding unit and integration test fixtures updated

### Config
- `config/pipeline-default.json`: `profile_id` → `agent_id`

### Policy Checks
- No ALTER TABLE, no DROP — schema untouched. ✓
- No product enums or string taxonomy introduced. ✓
- `zig build` clean, 53/53 tests pass. ✓
- `grep` of `src/`, `zombiectl/src/`, `config/` returns zero active-path hits. ✓

---

## 6.0 Out of Scope

- Backward-compatibility shims for unreleased legacy API clients
- New scoring behavior beyond schema/name alignment
- Any LLM-evaluated scoring enrichment tracked separately in `TODOS.md`
