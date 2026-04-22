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

1. **Rename / re-describe a zombie** (`PATCH /v1/workspaces/{ws}/zombies/{zombie_id}` with partial body). UI surface: inline edit in `ZombieConfig.tsx`.
2. **Pause a zombie** (new trigger deliveries rejected) and **resume it** (`POST :pause` / `:resume`). Kill (the existing `/stop`) stays separate; pause blocks new triggers, stop halts the running action.
3. **Set or clear a cron schedule** (`POST .../schedule` with a cron expression; `DELETE .../schedule` to clear). UI surface: cron editor in `TriggerPanel.tsx` (replacing the current "CLI-only for V1" placeholder in the schedule tab).
4. **Read and edit firewall rules** (`GET` / `PUT .../firewall`). UI surface: replace the entire `FirewallRulesEditor.tsx` placeholder with a rule list + inline add/edit/delete, matching the layout M19_001 §3 originally proposed.
5. **Pick a skill template when installing** (`GET /v1/skills` returns the catalog of skills parsed from the repo's `samples/` folder at server build time). UI surface: the template-picker row above the name/description/skill fields in `InstallZombieForm.tsx`.

**What "DONE" means per surface:** every operation has (a) a backend handler registered in the route manifest and backed by an integration test, (b) an OpenAPI schema entry, (c) a `zombiectl` subcommand with a unit test, and (d) the matching UI surface replacing the M19_001 placeholder.

---

## Files Changed (blast radius — predicted)

Backend (Zig):

| File | Action | Why |
|------|--------|-----|
| `src/http/route_manifest.zig` | MODIFY | Register `PATCH /zombies/{id}`, `:pause`, `:resume`, `POST\|DELETE /schedule`, `GET\|PUT /firewall`, `GET /skills`. |
| `src/http/router.zig` | MODIFY | Match the new methods + paths. |
| `src/http/route_table.zig` | MODIFY | Dispatch rows for the six new surfaces. |
| `src/http/handlers/zombies/mutations.zig` | CREATE | Rename / pause / resume handlers. ≤350 lines; keep each handler ≤~50. |
| `src/http/handlers/zombies/schedule.zig` | CREATE | Cron schedule set / clear. Cron expression validation at the boundary. |
| `src/http/handlers/zombies/firewall.zig` | CREATE | Firewall read + replace. Wraps existing `src/zombie/firewall/firewall.zig`. |
| `src/http/handlers/skills/catalog.zig` | CREATE | Enumerates `samples/*` at server start, caches the catalog in memory. |
| `src/zombie/zombie_store.zig` (or nearest) | MODIFY | `updateZombie`, `setPaused`, `setSchedule` facade functions. |
| `schema/NNN_zombie_mutations.sql` | CREATE | Columns for `paused_at`, `schedule_cron`, `description` if not already present. Schema Guard applies pre-v2.0. |
| `public/openapi/paths/zombies.yaml` | MODIFY | Row per new endpoint. |

CLI (JavaScript, `zombiectl/`):

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/zombie.js` | MODIFY | Add `update`, `pause`, `resume`, `schedule`, `triggers` subcommands. |
| `zombiectl/src/commands/zombie_firewall.js` | CREATE | `firewall list` / `firewall set` subcommands. |
| `zombiectl/test/zombie-mutations.unit.test.js` | CREATE | Covers each subcommand's success + error path. |

UI (`ui/packages/app/`):

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/api/zombies.ts` | MODIFY | Add `patchZombie`, `pauseZombie`, `resumeZombie`, `setSchedule`, `clearSchedule`, `getFirewall`, `putFirewall`. |
| `ui/packages/app/lib/api/skills.ts` | CREATE | `listSkills()` → `Skill[]`. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | MODIFY | Replace schedule placeholder with a real cron editor (preset buttons + free-text + validation + "next run" display). |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx` | MODIFY | Replace placeholder with rule table + inline add/edit/delete + confirm-delete. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx` | MODIFY | Add rename (inline edit) + pause/resume buttons. Delete stays as shipped. |
| `ui/packages/app/app/(dashboard)/zombies/new/InstallZombieForm.tsx` | MODIFY | Add template-picker row above name/description. |

---

## Sections (implementation slices)

### §1 — Rename, pause, resume

Smallest slice. Proves out the handler skeleton + OpenAPI + CLI + UI pattern that the rest of the sections copy.

**Dimensions:**

- 1.1 PENDING — target: `src/http/handlers/zombies/mutations.zig`
  - input: `PATCH /v1/workspaces/{ws}/zombies/{id}` with `{"name": "lead-collector-prod"}`
  - expected: 200 with updated zombie; subsequent GET returns new name
  - test_type: integration
- 1.2 PENDING — target: same handler
  - input: PATCH with no body fields
  - expected: 400 `UZ-ZOM-005 ERR_EMPTY_UPDATE`
  - test_type: integration
- 1.3 PENDING — target: pause/resume handler
  - input: `POST .../:pause` on an active zombie
  - expected: zombie's `paused_at` set; `/v1/webhooks/{id}` ingress returns 423 `UZ-ZOM-006 ERR_ZOMBIE_PAUSED`
  - test_type: integration
- 1.4 PENDING — target: `ZombieConfig.tsx`
  - input: user clicks inline rename, submits
  - expected: optimistic UI, `PATCH` fires, success toast; on 409 (name conflict) revert + error toast
  - test_type: unit (component test with MSW mock)
- 1.5 PENDING — target: `zombiectl zombie update lead-collector --name=lead-collector-prod`
  - expected: stdout "Renamed to lead-collector-prod"; non-zero exit on 409
  - test_type: unit (CLI test)

### §2 — Schedule (cron)

**Dimensions:**

- 2.1 PENDING — target: `src/http/handlers/zombies/schedule.zig`
  - input: `POST .../schedule` with `{"cron": "0 9 * * 2"}`
  - expected: stored; next run calculated; `GET` on the zombie returns `schedule_cron`
  - test_type: integration
- 2.2 PENDING — target: same
  - input: POST with `{"cron": "invalid"}`
  - expected: 422 `UZ-ZOM-003 ERR_INVALID_CRON` (already reserved in M19_001 error contracts)
  - test_type: integration
- 2.3 PENDING — target: `TriggerPanel.tsx` cron tab
  - input: user clicks "Every Tuesday 9am" preset, clicks Save
  - expected: POST fires, panel flips to "Next run: Tuesday …" read-state
  - test_type: unit

### §3 — Firewall rules

**Dimensions:**

- 3.1 PENDING — target: `src/http/handlers/zombies/firewall.zig`
  - input: `GET .../firewall` on a zombie with 2 rules
  - expected: 200 `{"rules": [...]}` matching stored shape
  - test_type: integration
- 3.2 PENDING — target: same
  - input: `PUT .../firewall` with an updated rules array
  - expected: 200; subsequent GET returns new array; outbound proxy now enforces new rules
  - test_type: integration (proxy end-to-end)
- 3.3 PENDING — target: `FirewallRulesEditor.tsx`
  - input: user adds a rule, saves
  - expected: PUT fires with merged array; on success table refreshes
  - test_type: unit

### §4 — Skills catalog + template picker

**Dimensions:**

- 4.1 PENDING — target: `src/http/handlers/skills/catalog.zig`
  - input: `GET /v1/skills`
  - expected: array of `{id, display_name, description}` matching what `samples/*` parses
  - test_type: integration
- 4.2 PENDING — target: `InstallZombieForm.tsx`
  - input: form renders; user clicks a template card
  - expected: name / description / skill inputs pre-filled
  - test_type: unit

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
