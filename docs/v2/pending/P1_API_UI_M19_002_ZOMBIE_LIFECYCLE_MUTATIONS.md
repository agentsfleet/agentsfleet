# M19_002: Zombie Lifecycle Mutations — Backend Mutation Endpoints + UI Surfaces

**Prototype:** v2
**Milestone:** M19
**Workstream:** 002
**Date:** Apr 22, 2026
**Status:** PENDING
**Priority:** P1 — Completes the dashboard lifecycle surface M19_001 scoped down to "install + delete + webhook + exhaustion." Operators still need CLI for every mutation beyond delete.
**Batch:** TBD — schedule alongside or after M27_001 (dashboard pages owner) once M26_001 unblocks it; the `/zombies/[id]/page.tsx` host file will already exist.
**Branch:** feat/m19-002-zombie-mutations (not yet created)
**Depends on:** M19_001 (DONE — lifecycle UI shell + panel stubs), M11_006 (DONE once merged — tenant billing `is_exhausted` is surfaced; this spec doesn't touch billing), M26_001 (unified design-system — so we're not rebuilding panels twice)

---

## Why this workstream exists

M19_001 verified the backend route manifest and discovered that its original dimension list assumed endpoints that do not exist. Scope was reduced to the buildable subset (install, delete, webhook URL, exhaustion banner, CLI `Woohoo!` line) and the remainder was deferred here with a single named pointer. The deferred items all share the same shape: **one thin backend mutation endpoint + one UI surface that M19_001 already stubbed.** Grouping them into a single workstream is correct because (a) they share boilerplate (route table + handler skeleton + OpenAPI row + test fixtures) so the second one is mostly free after the first, and (b) M19_001 already shipped user-visible "CLI-only for V1" placeholders that should all flip to "works" together rather than in four staggered releases.

The CLI gap is symmetric: `zombiectl zombie schedule`, `zombie firewall set`, `zombie triggers list`, `zombie update`, `zombie pause`, `zombie resume` are referenced in M19_001's DX paths table but do not exist in `zombiectl/src/commands/zombie.js`. They need to ship alongside — a UI-only surface without the CLI equivalent would break the "every action has a UI and a CLI" invariant.

---

## §0 — Pre-EXECUTE Verification

**Status:** PENDING

Before touching any file, confirm M19_001's surface is still in place and that no other workstream has started filling the gap in parallel. Re-run the backend route manifest grep against the list below; if any row now shows ✓, remove it from this spec's scope before EXECUTE.

```bash
# E0.1: Confirm the endpoints this spec intends to add are still missing.
grep -n -E "zombies.*(pause|resume|schedule|firewall)|/v1/skills" \
  src/http/route_manifest.zig src/http/router.zig
```

Expected: 0 matches (if any match, the scope shrinks — update this spec first).

```bash
# E0.2: Confirm M19_001's panel stubs are unchanged so this spec's UI edits land cleanly.
ls ui/packages/app/app/\(dashboard\)/zombies/\[id\]/components/
# expect: FirewallRulesEditor.tsx  TriggerPanel.tsx  ZombieConfig.tsx
```

Expected: those three files exist and still contain placeholder copy.

---

## Goal (testable)

An operator — from the dashboard, the CLI, or a scripted API client — can:

1. **Mutate any scalar field on a zombie** through a single `PATCH /v1/workspaces/{ws}/zombies/{zombie_id}` with a partial body. Rename, re-describe, pause/resume (`{"paused": true|false}`), and set/clear a cron schedule (`{"schedule_cron": "0 9 * * 2"}` or `null`) all flow through this one endpoint. UI surfaces: inline edit / pause-resume button / cron editor land in their respective M19_001 panels (`ZombieConfig.tsx`, `TriggerPanel.tsx`).
2. **Read and edit firewall rules** (`GET` / `PUT /v1/workspaces/{ws}/zombies/{zombie_id}/firewall`). Firewall rules are a rich sub-resource (array of structs), so they stay on their own path. UI surface: replace the entire `FirewallRulesEditor.tsx` placeholder with a rule list + inline add/edit/delete.
(A skill-template picker UI was originally sketched here as a third surface. After an API-design pass we decided operators can type the skill identifier the same way they type any other resource name — the picker is UX polish, not core functionality, and a `GET /v1/skills` catalog endpoint just to power it isn't carrying enough weight. If a template picker turns out to be operator-essential later, it opens as its own workstream; this spec no longer promises it.)

### URL-design rationale (`:pause` / `:resume` / `/schedule` explicitly rejected)

`docs/REST_API_DESIGN_GUIDELINES.md` §7 forbids action verbs in paths. The original M19_001 spec and M11_006 both leaned on the `:pause` / `:resume` suffix style (Google-cloud-ish). The existing `/stop` endpoint (M12) is a pre-v2 violation of the same rule; don't propagate it. Pause / resume / rename / describe / schedule are all **state transitions on the zombie resource** — that's a textbook `PATCH` body. One endpoint, one handler, one integration-test matrix, one OpenAPI row. Firewall stays separate because rules are a collection with their own add/remove semantics, not a scalar state bit.

**Side benefit:** the UI only needs one `patchZombie(workspaceId, zombieId, partial)` API-client call to serve rename + pause/resume + schedule. No six-method client module.

**What "DONE" means per surface:** every operation has (a) a backend handler registered in the route manifest and backed by an integration test, (b) an OpenAPI schema entry, (c) a `zombiectl` subcommand with a unit test, and (d) the matching UI surface replacing the M19_001 placeholder.

---

## Files Changed (blast radius — predicted)

Backend (Zig):

| File | Action | Why |
|------|--------|-----|
| `src/http/route_manifest.zig` | MODIFY | Register `PATCH /zombies/{id}` and `GET\|PUT /zombies/{id}/firewall`. Two routes total. |
| `src/http/router.zig` | MODIFY | Match the new methods + paths. |
| `src/http/route_table.zig` | MODIFY | Dispatch rows for the two new surfaces. |
| `src/http/handlers/zombies/patch.zig` | CREATE | Single PATCH handler. Accepts partial `{name, description, paused, schedule_cron}`; each field validated at the handler boundary (cron expression → `UZ-ZOM-003`; name conflict → 409). ≤350 lines. |
| `src/http/handlers/zombies/firewall.zig` | CREATE | Firewall read + replace. Wraps existing `src/zombie/firewall/firewall.zig`. |
| `src/zombie/zombie_store.zig` (or nearest) | MODIFY | A single `patchZombie(tenant_id, zombie_id, Patch)` facade — `Patch` is a struct with optional fields per scalar. Internal to the store, any write cascade (e.g. pause → cancel pending deliveries) lives in this one function so it can't drift between handlers. |
| `schema/NNN_zombie_mutations.sql` | CREATE | Columns for `paused_at`, `schedule_cron`, and `description` (if not already present on the zombies table). Schema Guard applies pre-v2.0. |
| `public/openapi/paths/zombies.yaml` | MODIFY | One new PATCH row + the firewall GET/PUT pair + the skills GET. |

CLI (JavaScript, `zombiectl/`):

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/zombie.js` | MODIFY | Add `update` subcommand — a thin wrapper around PATCH accepting `--name`, `--description`, `--pause`, `--resume`, `--schedule`, `--clear-schedule`. One CLI subcommand, one HTTP method, no separate `pause` / `resume` / `schedule` commands to maintain. |
| `zombiectl/src/commands/zombie_firewall.js` | CREATE | `firewall list` / `firewall set` subcommands. |
| `zombiectl/test/zombie-mutations.unit.test.js` | CREATE | Covers each flag's success + error path. |

UI (`ui/packages/app/`):

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/api/zombies.ts` | MODIFY | Add a single `patchZombie(workspaceId, zombieId, patch: Partial<ZombiePatch>)` that serves rename + pause/resume + schedule. Plus `getFirewall` / `putFirewall` for the sub-resource. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | MODIFY | Replace schedule placeholder with a real cron editor (preset buttons + free-text + validation + "next run" display). |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx` | MODIFY | Replace placeholder with rule table + inline add/edit/delete + confirm-delete. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx` | MODIFY | Add rename (inline edit) + pause/resume buttons. Delete stays as shipped. |

---

## Sections (implementation slices)

### §1 — Zombie PATCH (rename, describe, pause, resume, schedule)

The whole scalar-state surface of a zombie behind one endpoint. Do this section first — it unblocks the rename / pause/resume / cron UI all at once and proves out the PATCH pattern that the rest of the codebase can copy next time a resource grows mutable fields.

**Dimensions:**

- 1.1 PENDING — target: `src/http/handlers/zombies/patch.zig`
  - input: `PATCH /v1/workspaces/{ws}/zombies/{id}` body `{"name": "lead-collector-prod"}`
  - expected: 200 with the updated zombie; subsequent GET reflects the new name
  - test_type: integration
- 1.2 PENDING — target: same handler
  - input: PATCH with an empty body `{}`
  - expected: 400 `UZ-ZOM-005 ERR_EMPTY_UPDATE` — PATCH must name at least one field to change
  - test_type: integration
- 1.3 PENDING — target: same handler
  - input: PATCH body `{"paused": true}` on an active zombie
  - expected: `paused_at` set to now; `POST /v1/webhooks/{id}` subsequently returns 423 `UZ-ZOM-006 ERR_ZOMBIE_PAUSED`. A follow-up PATCH with `{"paused": false}` clears it. Running actions are NOT affected (that's `/stop`, out of scope).
  - test_type: integration
- 1.4 PENDING — target: same handler
  - input: PATCH body `{"schedule_cron": "0 9 * * 2"}`
  - expected: stored; zombie GET returns `schedule_cron` + a server-computed `next_run_at`; subsequent PATCH with `{"schedule_cron": null}` clears both.
  - test_type: integration
- 1.5 PENDING — target: same handler
  - input: PATCH body `{"schedule_cron": "not a cron"}`
  - expected: 422 `UZ-ZOM-003 ERR_INVALID_CRON` (already reserved in M19_001's error contracts)
  - test_type: integration
- 1.6 PENDING — target: `ZombieConfig.tsx` rename + pause/resume
  - input: user inline-renames; then clicks Pause, then Resume
  - expected: optimistic UI, `patchZombie` fires with the partial body, success toast on each action; on 409 name conflict the rename reverts + error toast
  - test_type: unit (component test with MSW mock)
- 1.7 PENDING — target: `TriggerPanel.tsx` schedule tab
  - input: user picks "Every Tuesday 9am" preset, clicks Save
  - expected: `patchZombie` fires with `{schedule_cron}`; panel renders "Next run: …" using the server-returned `next_run_at`
  - test_type: unit
- 1.8 PENDING — target: `zombiectl zombie update lead-collector --pause`
  - expected: exit 0, terse success line; `--name`, `--description`, `--schedule`, `--clear-schedule`, `--resume` flags all resolve to the same PATCH call with different bodies
  - test_type: unit (CLI)

### §2 — Firewall rules

**Dimensions:**

- 2.1 PENDING — target: `src/http/handlers/zombies/firewall.zig`
  - input: `GET .../firewall` on a zombie with 2 rules
  - expected: 200 `{"rules": [...]}` matching stored shape
  - test_type: integration
- 2.2 PENDING — target: same
  - input: `PUT .../firewall` with an updated rules array
  - expected: 200; subsequent GET returns new array; outbound proxy now enforces new rules
  - test_type: integration (proxy end-to-end)
- 2.3 PENDING — target: `FirewallRulesEditor.tsx`
  - input: user adds a rule, saves
  - expected: PUT fires with merged array; on success table refreshes
  - test_type: unit

<!-- §3 (skills catalog + template picker) removed: the picker is UX polish
and operators can type the skill identifier. A `/v1/skills` catalog
endpoint just to power that picker isn't carrying enough weight. If a
template picker becomes operator-essential later, it opens as its own
workstream. -->


---

## Out of Scope

- Webhook secret rotation from the UI — follow-up milestone.
- Firewall rule *templates* / preset libraries — deliberate; operators hand-write rules today.
- Skill template *editor* (editing the YAML/prompt) — CLI-only forever in V1.
- Zombie cloning / duplication.
- Multi-zombie bulk operations.

---

## Discovery

(to be filled during EXECUTE)

---

## Notes for the picker-upper

- The M19_001 panel files contain user-facing placeholder copy that points at CLI commands. When replacing with the real editor, delete the placeholder prose entirely; don't leave "formerly CLI-only" copy.
- The `@usezombie/design-system` `Dialog` primitive is the right confirm-delete surface for firewall rule deletes (M19_001's `ZombieConfig` inline-confirm pattern is fine for one-off actions but repeats awkwardly in a rule table).
- CLI success lines for mutations should follow the M19_001 `Woohoo!` pattern where operator confirmation matters (`schedule` set, `firewall` replaced) but stay terse for idempotent operations (`pause`, `resume`, `update`).
- The cron preset list in `TriggerPanel.tsx` should come from a shared helper, not hardcoded — the website and the app both show presets, and drift between them is a regression risk.
