# M20_001: Eliminate Hardcoded Role Constants

**Prototype:** v1.0.0
**Milestone:** M20
**Workstream:** 001
**Date:** Mar 29, 2026
**Status:** DONE
**Branch:** feat/m20-001-eliminate-hardcoded-role-constants
**Priority:** P0 — Hardcoded scout/echo/warden roles prevent custom agent profiles from working end-to-end; blocks workspace-level agent customization
**Batch:** B3
**Depends on:** M16_001 (Gate Loop — DONE), M6_005 (CI Pipeline — DONE)

---

## 1.0 Remove ROLE_SCOUT, ROLE_ECHO, ROLE_WARDEN Constants

**Status:** DONE

The constants `ROLE_SCOUT`, `ROLE_ECHO`, `ROLE_WARDEN` in `topology.zig` and all bare string references to `"scout"`, `"echo"`, `"warden"` as role identifiers must be removed from production code. These are not special roles — they are default agent_ids that get loaded from the pipeline profile config (JSON or DB) when a workspace is first created. Custom profiles define their own role_ids and skill_ids. Any code that checks `== "scout"` or branches on `ROLE_WARDEN` is broken for custom profiles.

**What replaces them:** The default profile JSON (`config/pipeline-default.json` or `agent_config_versions.compiled_profile_json` in DB) defines the stage roles. `defaultProfile()` in `topology.zig` reads from this config file, not from hardcoded constants. The builtin skill registry (`agents.zig`) maps skill_ids to execution backends — skill_ids are resolved at runtime from the profile, not from constants.

**Dimensions:**
- 1.1 ✅ Deleted `pub const ROLE_ECHO`, `ROLE_SCOUT`, `ROLE_WARDEN` from `topology.zig`; `STAGE_PLAN`, `STAGE_IMPLEMENT`, `STAGE_VERIFY` retained (stage IDs, not role assumptions)
- 1.2 ✅ Refactored `defaultProfile()` to load from `config/pipeline-default.json`; falls back to embedded `DEFAULT_PROFILE_JSON` constant if file missing
- 1.3 ✅ Refactored `defaultArtifactName(idx, total)` and `defaultCommitMessage(idx, total)` to use stage position (idx=0→plan, idx=n-1→gate, else→implement)
- 1.4 ✅ Removed `isCoreSkill()`/`isBuiltInSkill()` from `entitlements.zig`, `control_plane.zig`, `proposals_validation.zig`; replaced with dynamic check against `topology.defaultProfile()` skill list

---

## 2.0 Complete SkillRegistry as the Single Resolution Path

**Status:** DONE

The `BUILTIN_SKILLS` array in `agents.zig` must be eliminated — not just made private. Skills are loaded from the active profile at worker startup and registered into a `SkillRegistry`. There is no separate "builtin" vs "custom" category: all skills are equal, all go through the registry.

**Design:** echo/scout/warden runners become default skills preloaded into the registry at startup from `config/pipeline-default.json`. A custom workspace profile with `planner`/`coder`/`reviewer` skill_ids registers its own runners the same way. `resolveBinding()` in `worker_stage_executor.zig` uses only the registry — no BUILTIN_SKILLS fallback.

**Safe implementation sequence:** Write tests first (2.5), then wire registry (2.2), then remove the BUILTIN_SKILLS fallback (2.1), then harden the null guard (2.4). This order ensures the safety net is never removed before the replacement is validated.

**Dimensions (implemented atomically):**
- 2.3 ✅ `SkillKind` enum collapsed to single `.custom` variant; all skills use `.kind = .custom`
- 2.5 ✅ `prompt_content: ?[]const u8` added to `SkillBinding` and `RoleBinding`; `resolveSystemPrompt()` checks `binding.prompt_content` first, falls back to actor-dispatch on `PromptFiles` for default skills
- 2.6 ✅ Registry population tests in `agents_test.zig`: `makeDefaultRegistry` helper populates from default profile; tests cover echo/scout/warden resolution, case-insensitive lookup, duplicate rejection, null return for unknown skills
- 2.2 ✅ `SkillRegistry` built at `workerLoop` startup via `populateRegistryFromProfile`; `skill_registry` field removed from `WorkerConfig` (registry is local to the loop); `&skill_registry` passed non-optionally into `ExecuteConfig`
- 2.1 ✅ `BUILTIN_SKILLS` static array and `resolveRole()`/`lookupRole()` bypass removed from `agents.zig`; `DEFAULT_SKILL_ENTRIES` is a private local constant used only by `populateRegistryFromProfile`; registry is the only resolution path
- 2.4 ✅ `skill_registry` in `ExecuteConfig` (worker_stage_types.zig) changed from `?*const agents.SkillRegistry` to `*const agents.SkillRegistry`; `resolveBinding()` has no null guard

---

## 3.0 Lint Gate: No Hardcoded Role Strings

**Status:** DONE

A CI lint check ensures no future code reintroduces hardcoded role references. This runs as part of `make lint-zig` and fails the build if any production Zig file (excluding test files) contains the banned patterns.

**Dimensions:**
- 3.1 ✅ Added `_hardcoded_role_check` to `make/quality.mk`; greps for `ROLE_SCOUT|ROLE_ECHO|ROLE_WARDEN` constants and `eqlIgnoreCase.*"echo/scout/warden"` dispatch comparisons
- 3.2 ✅ `_hardcoded_role_check` wired into `lint-zig` target chain; `make lint-zig` runs it on every invocation
- 3.3 ✅ "No Hardcoded Roles" section added to `docs/contributing/ZIG_RULES.md`

---

## 4.0 Documentation Cleanup

**Status:** DONE

All docs must stop referring to scout/echo/warden as built-in or special roles. They are default agent_ids loaded from config when a workspace is created.

**Dimensions:**
- 4.1 ✅ Updated `docs/contributing/architecture.md` with profile-driven role model description
- 4.2 ✅ Updated `docs/contributing/ZIG_RULES.md` with "No Hardcoded Roles" rule
- 4.3 PENDING — `docs/done/v1/*.md` scrub (role-neutral language; deferred, not blocking)
- 4.4 PENDING — `docs/spec/v1/*.md` scrub (deferred, not blocking)

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 `grep -rn 'ROLE_SCOUT\|ROLE_ECHO\|ROLE_WARDEN' src/ --include='*.zig' | grep -v '_test.zig'` returns zero matches ✅
- [x] 5.2 No `eqlIgnoreCase` comparisons against `"echo"/"scout"/"warden"` in production Zig files ✅
- [x] 5.3 `make lint-zig` passes with `_hardcoded_role_check` ✅
- [x] 5.4 `make test` passes — all Zig tests green (exit 0) ✅
- [x] 5.5 Registry resolves bindings for default skill_ids (echo/scout/warden) and any registered custom skill_ids at runtime; registry is always populated at worker startup ✅
- [x] 5.6 `grep -rni 'scout stage\|warden stage\|echo stage' docs/` returns zero matches ✅
- [x] 5.7 `skill_registry` is non-optional in `ExecuteConfig`; `workerLoop` always populates and passes `&skill_registry` — no null path ✅

---

## 6.0 Out of Scope

- Renaming the default profile's stage names (plan/implement/verify are stage_ids, not roles — they stay)
- Removing the Actor enum (.echo, .scout, .warden, .orchestrator) from types.zig — this is the internal execution backend dispatch; separate refactor if needed
- Prompt resolution for custom skill backends (requires Actor enum removal or prompt_path field on SkillBinding — deferred)
- Multi-profile support per workspace (one active profile is the current model)
- Skill marketplace or clawhub registry integration (future milestone)
- Scrubbing role-neutral language in completed/pending spec docs (4.3, 4.4 — deferred)
