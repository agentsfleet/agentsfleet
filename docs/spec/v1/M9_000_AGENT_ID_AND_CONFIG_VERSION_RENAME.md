# M9_000: Agent ID And Config Version Rename

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 000
**Date:** Mar 14, 2026
**Status:** PENDING
**Priority:** P0 — prerequisite rename required before the rest of M9 can align on agent terminology
**Batch:** B0 — must land before M9 persistence, context injection, and auto-improvement workstreams
**Depends on:** Nothing

---

## 1.0 Schema Rename

**Status:** PENDING

Rename the existing harness-control-plane identifiers from profile-oriented names to agent/config-oriented names.

**Dimensions:**
- 1.1 PENDING Rename `agent_profiles.profile_id` → `agent_id` and keep it as the primary key
- 1.2 PENDING Rename `agent_profile_versions` → `agent_config_versions`
- 1.3 PENDING Rename `agent_profile_versions.profile_version_id` → `config_version_id` and `agent_profile_versions.profile_id` → `agent_id`
- 1.4 PENDING Rename downstream FK columns: `workspace_active_profile.profile_version_id` → `config_version_id`, `profile_compile_jobs.requested_profile_id` → `requested_agent_id`, and all other M9-referenced config-version links

---

## 2.0 Source Code Rename

**Status:** PENDING

Update Zig and CLI code to use the new names consistently without leaving mixed terminology in active paths.

**Dimensions:**
- 2.1 PENDING Update harness control plane queries, types, and handler responses to use `agent_id` / `config_version_id`
- 2.2 PENDING Update worker/runtime paths that currently pass `profile_id` or `profile_version_id`
- 2.3 PENDING Update CLI payload keys, display labels, and parsing that currently use profile terminology
- 2.4 PENDING Keep public API naming coherent: user-facing M9 routes and payloads say `agent`, not `profile`

---

## 3.0 Verification

**Status:** PENDING

Prove the rename is complete and does not leave broken FK references or mixed contracts.

**Dimensions:**
- 3.1 PENDING Canonical migrations bootstrap cleanly with the renamed schema
- 3.2 PENDING Harness compile/activate/active flows pass against renamed columns and tables
- 3.3 PENDING Grep-based verification shows no remaining active-path `profile_id` / `profile_version_id` references except explicitly preserved historical docs or compatibility notes
- 3.4 PENDING M9 follow-on specs reference this file as the prerequisite source of truth

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 The database schema no longer uses `profile_id` or `profile_version_id` in active harness-control-plane tables
- [ ] 4.2 Active source code paths use `agent_id` and `config_version_id` consistently
- [ ] 4.3 Existing harness workflows still work after the rename
- [ ] 4.4 M9_001–M9_004 can reference this spec instead of carrying rename details inline

---

## 5.0 Out of Scope

- Backward-compatibility shims for unreleased legacy API clients
- New scoring behavior beyond schema/name alignment
- Any LLM-evaluated scoring enrichment tracked separately in `TODOS.md`
