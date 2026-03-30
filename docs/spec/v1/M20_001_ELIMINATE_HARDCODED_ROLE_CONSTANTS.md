# M20_001: Eliminate Hardcoded Role Constants

**Prototype:** v1.0.0
**Milestone:** M20
**Workstream:** 001
**Date:** Mar 29, 2026
**Status:** PENDING
**Priority:** P0 — Hardcoded scout/echo/warden roles prevent custom agent profiles from working end-to-end; blocks workspace-level agent customization
**Batch:** B3
**Depends on:** M16_001 (Gate Loop — DONE), M6_005 (CI Pipeline — DONE)

---

## 1.0 Remove ROLE_SCOUT, ROLE_ECHO, ROLE_WARDEN Constants

**Status:** PENDING

The constants `ROLE_SCOUT`, `ROLE_ECHO`, `ROLE_WARDEN` in `topology.zig` and all bare string references to `"scout"`, `"echo"`, `"warden"` as role identifiers must be removed from production code. These are not special roles — they are default agent_ids that get loaded from the pipeline profile config (JSON or DB) when a workspace is first created. Custom profiles define their own role_ids and skill_ids. Any code that checks `== "scout"` or branches on `ROLE_WARDEN` is broken for custom profiles.

**What replaces them:** The default profile JSON (`config/pipeline-default.json` or `agent_config_versions.compiled_profile_json` in DB) defines the stage roles. `defaultProfile()` in `topology.zig` reads from this config file, not from hardcoded constants. The builtin skill registry (`agents.zig`) maps skill_ids to execution backends — skill_ids are resolved at runtime from the profile, not from constants.

**Dimensions:**
- 1.1 PENDING Delete `pub const ROLE_ECHO`, `ROLE_SCOUT`, `ROLE_WARDEN` from `topology.zig`; delete `STAGE_PLAN`, `STAGE_IMPLEMENT`, `STAGE_VERIFY` if they encode role assumptions
- 1.2 PENDING Refactor `defaultProfile()` to load from `config/pipeline-default.json` (file-based config) instead of hardcoding stage definitions inline; fall back to a minimal embedded default only if the config file is missing
- 1.3 PENDING Refactor `defaultArtifactName()` and `defaultCommitMessage()` to use stage position (plan=0, implement=1..n-1, gate=last) instead of matching on role string
- 1.4 PENDING Remove `isBuiltinSkill()` checks in `entitlements.zig`, `control_plane.zig`, and `proposals_validation.zig` — all skills are equal; entitlement checks use the profile's skill list, not a hardcoded allowlist

---

## 2.0 Update Agent Skill Registry

**Status:** PENDING

The `BUILTIN_SKILLS` array in `agents.zig` hardcodes echo/scout/warden as the only known skills. This must become a dynamic registry loaded from config, with the three defaults registered at startup from the default profile — not from constants.

**Dimensions:**
- 2.1 PENDING Replace `BUILTIN_SKILLS` constant array with a `SkillRegistry` that is populated at worker startup from the active profile's skill_ids
- 2.2 PENDING `resolveRole()` falls back to a generic executor dispatch when the skill_id is not in the registry (custom skills are first-class, not errors)
- 2.3 PENDING The `SkillKind` enum (`echo`, `scout`, `warden`, `custom`) is replaced by a single kind: all skills are `custom` from the registry's perspective; the execution backend (prompt file, tool set) is determined by the skill's config, not its name

---

## 3.0 Lint Gate: No Hardcoded Role Strings

**Status:** PENDING

A CI lint check ensures no future code reintroduces hardcoded role references. This runs as part of `make lint-zig` and fails the build if any production Zig file (excluding test files) contains the banned patterns.

**Dimensions:**
- 3.1 PENDING Add `_hardcoded_role_check` target to `make/quality.mk` that greps `src/` for `ROLE_SCOUT`, `ROLE_ECHO`, `ROLE_WARDEN`, and bare `"scout"`/`"echo"`/`"warden"` as role/skill identifiers; excludes test files (`*_test.zig`) and the config loader that reads the default values from JSON
- 3.2 PENDING Wire `_hardcoded_role_check` into `lint-zig` target chain so it runs on every `make lint`
- 3.3 PENDING Add the policy to `docs/ZIG_RULES.md` under a "No Hardcoded Roles" section

---

## 4.0 Documentation Cleanup

**Status:** PENDING

All docs must stop referring to scout/echo/warden as built-in or special roles. They are default agent_ids loaded from config when a workspace is created.

**Dimensions:**
- 4.1 PENDING Update `docs/contributing/architecture.md` to describe the profile-driven role model: workspaces get a default profile with three stages (plan, implement, verify) whose role_ids and skill_ids come from config — not hardcoded constants
- 4.2 PENDING Update `docs/contributing/ZIG_RULES.md` with "No Hardcoded Roles" rule: never use string literals for role/skill identification in production code; always read from the profile
- 4.3 PENDING Scrub `docs/done/v1/*.md` completed specs: replace references to "the scout stage" or "the warden stage" with "the implement stage" or "the verification stage" (role-neutral language)
- 4.4 PENDING Scrub `docs/spec/v1/*.md` pending specs for the same pattern

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 `grep -rn 'ROLE_SCOUT\|ROLE_ECHO\|ROLE_WARDEN' src/ --include='*.zig' | grep -v '_test.zig'` returns zero matches
- [ ] 5.2 `grep -rn '"scout"\|"echo"\|"warden"' src/ --include='*.zig' | grep -v '_test.zig' | grep -v 'config.*json\|pipeline.*default'` returns zero matches (excluding config file readers)
- [ ] 5.3 `make lint-zig` passes with the new `_hardcoded_role_check` target
- [ ] 5.4 `make test` passes — all existing tests work with config-loaded defaults
- [ ] 5.5 A custom profile with role_ids `planner`/`coder`/`reviewer` works end-to-end: profile compiles, run executes all stages, gate repair uses the correct implement stage role
- [ ] 5.6 `grep -rni 'scout stage\|warden stage\|echo stage' docs/` returns zero matches (role-neutral language in docs)

---

## 6.0 Out of Scope

- Renaming the default profile's stage names (plan/implement/verify are stage_ids, not roles — they stay)
- Removing the Actor enum (.echo, .scout, .warden, .orchestrator) from types.zig — this is the internal execution backend dispatch, not user-facing role identity; separate refactor if needed
- Multi-profile support per workspace (one active profile is the current model)
- Skill marketplace or clawhub registry integration (future milestone)
