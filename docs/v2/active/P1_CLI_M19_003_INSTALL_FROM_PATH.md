# M19_003: `zombiectl zombie install --from <path>` — Install a Zombie from a Local Directory

**Prototype:** v2.0.0
**Milestone:** M19
**Workstream:** 003
**Date:** Apr 23, 2026
**Status:** PENDING
**Priority:** P1 — unblocks the platform-ops sample's acceptance script. `samples/platform-ops/README.md` tells operators to run this exact command; it does not exist today.
**Batch:** B2 — schedule before M37_001 can close its §4 dims.
**Branch:** feat/m19-003-install-from-path (not yet created)
**Depends on:** M19_001 (DONE — `POST /v1/workspaces/{ws}/zombies` accepts a zombie body).

---

## Why this workstream exists

M19_001 shipped the **web** install form that POSTs to `POST /v1/workspaces/{ws}/zombies`. The platform-ops sample's acceptance script in `samples/platform-ops/README.md` step 2 calls `zombiectl zombie install --from samples/platform-ops` — a CLI that reads a local `SKILL.md` + `TRIGGER.md` pair and submits them to the same endpoint. No spec owns that CLI command today. M37_001 §4 blocks on it.

Keep it small: no template picker, no `--template` flag here (that's the bundled-templates path M33_001 owns). Just "read two markdown files, POST their content."

---

## Goal (testable)

From a fresh checkout with a seeded vault:

```
$ zombiectl zombie install --from samples/platform-ops
🎉 Woohoo! Your zombie is installed and ready to run.
Webhook: https://api-dev.usezombie.com/v1/webhooks/zom_01xyz
```

Same success line M19_001 already asserts for the CLI's existing install path, same `POST` endpoint, same 201 → `{id, webhook_url}` response shape.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie_install.js` | EDIT | Add `--from <path>` option to the existing `zombie install` subcommand. When set, skip the bundled-template lookup path entirely. |
| `zombiectl/src/lib/load_skill_from_path.js` | CREATE | Reads `<path>/SKILL.md` + `<path>/TRIGGER.md`, validates both files exist and parse, returns the request body shape `POST /v1/workspaces/{ws}/zombies` already accepts. ≤120 lines. |
| `zombiectl/test/zombie-install-from-path.unit.test.js` | CREATE | Unit tests per dim below. |

**Explicitly out of scope in this file-list:** new backend endpoints, new OpenAPI rows, new CLI subcommands, changes to M19_001's install form.

---

## Sections

### §1 — CLI flag + loader

**Dimensions:**

- 1.1 PENDING — target: `zombiectl zombie install --from samples/platform-ops` in a tmpdir with seeded auth
  - expected: reads both files, POSTs, prints `Woohoo!` + webhook URL, exits 0
  - test_type: unit (API mocked)
- 1.2 PENDING — target: `--from ./missing-dir`
  - expected: exits non-zero with `ERR_PATH_NOT_FOUND: <path> (expected SKILL.md + TRIGGER.md)`; no POST fired
  - test_type: unit
- 1.3 PENDING — target: `--from <path>` where only `SKILL.md` exists
  - expected: exits non-zero with `ERR_TRIGGER_MISSING: <path>/TRIGGER.md`; no POST fired
  - test_type: unit
- 1.4 PENDING — target: `--from <path>` with a SKILL.md that fails frontmatter parse
  - expected: exits non-zero with the parser's own error (propagated, not wrapped); no POST fired
  - test_type: unit

### §2 — Conflict / 409 behaviour

**Dimensions:**

- 2.1 PENDING — target: install twice from the same path in the same workspace
  - expected: second call surfaces the server's 409 `UZ-ZOM-002 ERR_ZOMBIE_NAME_CONFLICT` as a clear CLI error pointing at the existing zombie name; exit code 1
  - test_type: unit (API mock returns 409)

---

## Acceptance Criteria

- [ ] `zombiectl zombie install --from samples/platform-ops` produces a 201 + webhook URL against a running `zombied` — manual smoke
- [ ] All §1 + §2 dims green — `bun test` in `zombiectl/`
- [ ] M37_001 §4.1 (install from samples/platform-ops) can now execute — confirms this workstream unblocks it
- [ ] No changes to `POST /v1/workspaces/{ws}/zombies` shape — `grep -n "POST.*zombies" public/openapi/paths/zombies.yaml` shows unchanged

---

## Out of Scope

- Bundled templates (`--template <name>`) — M33_001's territory.
- Multi-file skill bundles (`SKILL.md` + child files). Homelab is two files; that's the MVP surface.
- Validating cron / tool / credential names client-side. The backend is already the authority.

---

## Eval Commands

```bash
cd zombiectl && bun test test/zombie-install-from-path.unit.test.js
cd zombiectl && bun bin/zombiectl.js zombie install --from ../samples/platform-ops  # smoke
```
