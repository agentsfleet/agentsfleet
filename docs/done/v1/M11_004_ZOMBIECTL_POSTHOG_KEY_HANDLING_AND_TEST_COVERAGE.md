# M11_004: `zombiectl` PostHog Key Handling And Test Coverage

**Prototype:** v1.0.0
**Milestone:** M11
**Workstream:** 004
**Date:** Mar 22, 2026
**Status:** ✅ DONE
**Priority:** P1 — CLI analytics UX + deterministic threat model
**Batch:** B1
**Depends on:** M5_005 (Website PostHog integration), M5_006 (zombied PostHog integration), M4_001 (`zombiectl` runtime)

---

## 1.0 Singular Function

**Status:** ✅ DONE

Ship `zombiectl` with a bundled default PostHog project key, move persisted CLI state to `~/.config/zombiectl`, keep analytics key handling out of auth state, and lock the CLI analytics orchestration behind direct tests.

**Dimensions:**
- 1.1 ✅ DONE Bundle the default PostHog project key in `zombiectl` so `npm install -g zombiectl` works with no analytics setup
- 1.2 ✅ DONE Move persisted CLI state to `~/.config/zombiectl/{credentials,workspaces}.json` with no backward-compatibility shim for pre-launch code
- 1.3 ✅ DONE Keep analytics key handling outside `credentials.json` / auth state; only env opt-out/override knobs remain
- 1.4 ✅ DONE Document the threat model explicitly: bundled key is public/write-scoped and abuse impacts analytics integrity only

---

## 2.0 Verification Units

**Status:** ✅ DONE

**Dimensions:**
- 2.1 ✅ DONE Unit test: `resolveConfig()` uses the bundled default key when no env override exists
- 2.2 ✅ DONE Unit test: `resolveConfig()` still honors env override and opt-out semantics
- 2.3 ✅ DONE Unit test: CLI state paths default to `~/.config/zombiectl` and still honor `ZOMBIE_STATE_DIR`
- 2.4 ✅ DONE CLI orchestration tests cover `cli_command_started`, `cli_command_finished`, `user_authenticated`, `workspace_created`, and `cli_error`
- 2.5 ✅ DONE CLI orchestration tests confirm analytics remains fail-open and shutdown still runs when capture throws

---

## 3.0 Acceptance Criteria

**Status:** ✅ DONE

- [x] 3.1 End users do not need to set `ZOMBIE_POSTHOG_KEY` for default CLI telemetry
- [x] 3.2 CLI state now persists under `~/.config/zombiectl` by default
- [x] 3.3 Analytics keys are not persisted in `credentials.json` or any auth-state file
- [x] 3.4 Docs describe the real threat model and operator knobs without treating the key as a secret
- [x] 3.5 CLI analytics behavior is directly tested at the `runCli()` orchestration boundary

---

## 4.0 Out of Scope

- Replacing direct CLI -> PostHog with a backend proxy
- Changing `zombied` PostHog key handling
