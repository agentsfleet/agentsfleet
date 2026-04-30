# M44_001: Install Contract Alignment + Doctor Extension + AUTH_EXEMPT Fix

**Prototype:** v2.0.0
**Milestone:** M44
**Workstream:** 001
**Date:** Apr 25, 2026 (amended Apr 27, 2026)
**Status:** DONE
**Priority:** P1 — launch-blocking. Three load-bearing bugs in shipped code that block the wedge end-to-end. Codex outside-voice review surfaced these as P0 contract mismatches.
**Categories:** API, CLI
**Batch:** B1 — parallel with M40, M41, M42, M43, M45.
**Branch:** feat/m44-install-contract
**Depends on:** none — these are bug fixes + a deterministic preflight on existing surfaces.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §8.2 (Installing the zombie), §12 step 5 (innerCreateZombie atomic publish — depends on M40 too but the contract fix is independent).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/zombie.js:83` — current install POST shape
2. `src/http/handlers/zombies/create.zig:34` — current API `CreateBody` (the mismatch site; `api.zig` re-exports)
3. `src/zombie/config_parser.zig:108` — `tools:` vs `skills:` key disagreement
4. `src/zombie/config_markdown.zig` + `src/zombie/yaml_frontmatter.zig` — server-side TRIGGER.md → ZombieConfig pipeline (already exists, wire it in)
5. `zombiectl/src/cli.js:36` — `AUTH_EXEMPT_ROUTES` list (the security hole)
6. `zombiectl/src/commands/core-ops.js:9` — existing `commandDoctor` (extend, don't replace)

---

## Overview

**Goal (testable):** `zombiectl install --from samples/platform-ops/` succeeds end-to-end against a fresh workspace. The install POST body carries `{trigger_markdown, source_markdown}`; the API parses the YAML frontmatter server-side via the existing `parseZombieFromTriggerMarkdown` pipeline to derive both `name` and `config_json`. The parser accepts the shipped sample's `tools:` key without modification. `AUTH_EXEMPT_ROUTES` shrinks to `{"login"}` — both `install` and `doctor` now require a valid local token, since both interact with workspace state. `zombiectl doctor` returns a structured pass/fail report covering: (1) `server_reachable`, (2) `workspace_selected`, (3) `workspace_binding_valid`. (`auth_token_present` is dropped — the auth guard already enforces it before doctor runs.) The install-skill (M49) calls `zombiectl doctor --json` first (post-`auth login`); on any fail, it surfaces the doctor output and aborts.

**Problem (three concrete bugs Codex caught):**

1. **Install API contract mismatch.** `zombiectl install` POSTs `{source_markdown, trigger_markdown}`. API expects `{name, config_json, source_markdown}`. Result: API returns 400; user sees "bad request" with no clear path forward.
2. **Parser key mismatch.** Shipped `samples/platform-ops/TRIGGER.md` declares `tools: [...]`. The parser at `src/zombie/config_parser.zig:108` requires the key `skills:`. Result: even if the contract were fixed, the parser rejects the sample.
3. **Auth-exempt install (and doctor).** `zombiectl/src/cli.js:36` lists `zombie.install` and `doctor` in `AUTH_EXEMPT_ROUTES`. Install creates a tenant-bound `core.zombies` row and must require auth. Doctor's checks (workspace_selected, workspace_binding_valid) only have meaning post-auth — running unauthenticated yields a confusing partial report instead of a clean "log in first" message.

**Solution summary:** Three small, scoped fixes + one CLI extension.

- **Fix 1 (install contract — server-side parse)**: change the CLI POST body to `{trigger_markdown, source_markdown}` and have the API run `parseZombieFromTriggerMarkdown(trigger_markdown)` to derive `name` + `config_json` before INSERT. The repo already ships the parser pipeline (`src/zombie/config_markdown.zig` + `src/zombie/yaml_frontmatter.zig`); no JS YAML parser is added. Failure of the YAML/frontmatter parse maps to a 400 with the underlying `ZombieConfigError`. No backward compat needed — today's contract is broken end-to-end. *Design note:* an earlier draft prescribed CLI-side YAML parsing; we rejected that because it would duplicate ~200 lines of YAML→JSON logic across Zig + JS for zero validation win. Bun's `src/md/*` was evaluated as a vendor source and ruled out — it parses CommonMark body content, not YAML frontmatter. Reserved for a future renderer-vendor milestone.
- **Fix 2 (parser)**: rename parser key from `skills` to `tools` in `src/zombie/config_parser.zig:108`. Cascade: `config_validate.zig`, `config_types.zig` (`ZombieConfig.skills` → `tools`), `config_markdown_test.zig` fixture, and the two `error_entries.zig` messages that reference "skills list". Single canonical key.
- **Fix 3 (auth)**: shrink `AUTH_EXEMPT_ROUTES` to `{"login"}`. Both install and doctor require valid local credentials per the auth-guard at `cli.js:93`. Doctor's purpose narrows to a post-auth health verification; the bootstrap "am I logged in" question is answered by `auth login` itself.
- **Doctor extension**: extend the existing `commandDoctor` (now `core-ops.js:9`) to verify three conditions: `server_reachable`, `workspace_selected`, `workspace_binding_valid`. JSON output mode is **already shipped** today (the function honors `ctx.jsonMode`); harden the schema and add the binding check. Existing `healthz`/`readyz`/`credentials`/`workspace` checks fold into the new three or are dropped (see §4).

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie.js` | EDIT | Install POST body becomes `{trigger_markdown, source_markdown}`; remove the `name` regex and the `bundle.name` heuristic |
| `zombiectl/src/lib/load-skill-from-path.js` | EDIT | Drop the `name` regex (now server-derived); return `{skill_md, trigger_md}` only |
| `src/http/handlers/zombies/create.zig` | EDIT | Accept `{trigger_markdown, source_markdown}`; call `parseZombieFromTriggerMarkdown`; serialize `ZombieConfig` → `config_json`; INSERT |
| `src/zombie/config_parser.zig` | EDIT | Line 108: rename parser key `skills` → `tools`; rename `parseSkillsField` → `parseToolsField` |
| `src/zombie/config_validate.zig` | EDIT | Rename `skills` parameter / `validateSkillsAndCredentials` accordingly |
| `src/zombie/config_types.zig` | EDIT | `ZombieConfig.skills` → `.tools` (cascade through all consumers) |
| `src/zombie/config_markdown_test.zig` | EDIT | Update fixture from `skills:` to `tools:` |
| `src/errors/error_entries.zig` | EDIT | Update messages that reference "skills list" / "TRIGGER.md skills:" |
| `zombiectl/src/cli.js` | EDIT | `AUTH_EXEMPT_ROUTES = new Set(["login"])` — remove both `"doctor"` and `"zombie.install"` |
| `zombiectl/src/commands/core-ops.js` | EDIT | Replace 4 checks with 3: `server_reachable`, `workspace_selected`, `workspace_binding_valid`; tighten schema; per-check 5s timeout |
| `zombiectl/test/zombie-install-from-path.unit.test.js` | EDIT | Cover the new POST body shape |
| `zombiectl/test/doctor-json.test.js` | EDIT | Cover the 3 checks (pass + fail variants) and JSON schema |
| `samples/platform-ops/TRIGGER.md` | NO EDIT | Already uses `tools:` — that's the spec, parser conforms to sample |

---

## Sections (implementation slices)

### §1 — Install POST contract alignment (server-side parse)

**CLI side** (`zombiectl/src/commands/zombie.js` + `lib/load-skill-from-path.js`):

```
1. Read SKILL.md and TRIGGER.md from --from <path> (existing loader)
2. POST { trigger_markdown: <TRIGGER.md raw>, source_markdown: <SKILL.md raw> }
3. Render server's 201 response; on 400, surface server's structured error code/hint
```

The CLI does no YAML parsing. The current `name` regex in `load-skill-from-path.js` is removed — `name` is now derived server-side from the parsed frontmatter, which is the single source of truth.

**API side** (`src/http/handlers/zombies/create.zig`):

```
1. Accept { trigger_markdown, source_markdown }
2. Validate: source_markdown non-empty + ≤MAX_SOURCE_LEN; trigger_markdown non-empty + ≤MAX_SOURCE_LEN
3. config = parseZombieFromTriggerMarkdown(alloc, trigger_markdown)
   - On error → 400 with the parser's ZombieConfigError mapped to ec.ERR_ZOMBIE_INVALID_CONFIG
4. name = config.name (already validated by parser: 1..64 chars, /^[a-z0-9-]+$/)
5. config_json = serialize(config) — JSON encoding of the parsed ZombieConfig
6. INSERT core.zombies(workspace_id, name, source_markdown, config_json, ...) → status=active
7. M40's atomic publish handles XGROUP CREATE + XADD zombie:control
8. Return 201 { zombie_id, status }
```

**Implementation default**: if `parseZombieFromTriggerMarkdown` rejects the input, return 400 with `ERR_ZOMBIE_INVALID_CONFIG` and a hint identifying the missing/invalid field (use the existing `ZombieConfigError` variants — `MissingRequiredField`, parse failures, etc.). Make the failure self-documenting.

**Serialization helper**: if a `ZombieConfig` → JSON serializer doesn't exist yet, add a focused `zombieConfigToJson(alloc, config)` in `src/zombie/config.zig` (or a sibling). Use `std.json.stringifyAlloc` over a shaped struct that mirrors what the parser accepts (round-trip property: `parse(serialize(c)) == c`). One round-trip test pins this.

### §2 — Parser key rename

Rename the canonical key `skills` → `tools` across the parser layer. Single canonical key, no dual-accept.

**Files** (cascade):

- `src/zombie/config_parser.zig:108` — `root.get("skills")` → `root.get("tools")`. Rename the helper `parseSkillsField` → `parseToolsField`.
- `src/zombie/config_parser.zig:53,66,73` — local `skills` → `tools`; pass to `validateToolsAndCredentials` (rename).
- `src/zombie/config_validate.zig:20-45` — rename param `skills` → `tools`, function `validateSkillsAndCredentials` → `validateToolsAndCredentials`, loop var `skill` → `tool`.
- `src/zombie/config_types.zig:84,98` — `ZombieConfig.skills` → `.tools`. Update `freeStringSlice` call site.
- `src/zombie/config.zig` — re-export `validateZombieSkills` becomes `validateZombieTools`.
- `src/zombie/config_markdown_test.zig:14` — fixture string `"skills:\n  - agentmail"` → `"tools:\n  - agentmail"`.
- `src/errors/error_entries.zig:161,163,183` — message text "skills list" / "TRIGGER.md skills:" → "tools list" / "TRIGGER.md tools:". Error codes themselves are not renamed (codes are stable identifiers; messages are user-facing prose).
- All other consumers — `grep -rn "\.skills\b\|\bskills:" src/` post-edit must return zero hits outside of historical/test-snapshot files.

**Implementation default**: do NOT support both keys (no `tools` OR `skills`). One canonical key. If a legacy spec uses `skills:`, it errors with the existing `MissingRequiredField` for `tools` — the message points the user to the canonical key.

### §3 — Shrink AUTH_EXEMPT_ROUTES to {login}

`zombiectl/src/cli.js:36`:

```diff
-const AUTH_EXEMPT_ROUTES = new Set(["login", "doctor", "zombie.install"]);
+const AUTH_EXEMPT_ROUTES = new Set(["login"]);
```

Also update the comment at `cli.js:92` from "skip for login, doctor, help, version" to "skip for login only".

Doctor and install both now flow through `requireAuth(ctx)` at `cli.js:93`. Tests:

1. `zombiectl install --from <path>` with no token → fails with `AUTH_REQUIRED` before any HTTP.
2. `zombiectl doctor` with no token → same.
3. `zombiectl login` with no token → still works (only exempt route).

### §4 — Doctor extension: 3 checks

`zombiectl/src/commands/core-ops.js` (extend existing `commandDoctor`):

Each check returns `{name: string, ok: boolean, detail: string}`. Aggregate result is `{ok: boolean, api_url: string, checks: [...]}` (matches today's shape; consumers don't need to relearn).

| Check | What it verifies | Pass condition |
|---|---|---|
| `server_reachable` | `GET /healthz` against the configured API URL | 200 within 5s timeout, body `{status: "ok"}` |
| `workspace_selected` | Local config has a selected workspace_id | Non-empty `workspaces.current_workspace_id` |
| `workspace_binding_valid` | The token is bound to the selected workspace (server confirms) | `GET /v1/workspaces/{ws}/zombies` returns 200 (the canonical workspace-scoped read; `GET /v1/workspaces/{ws}` does not exist as a standalone endpoint pre-v2.0) |

`auth_token_present` is dropped — the CLI auth guard already enforces it before doctor runs (see §3). The existing `readyz` and `credentials` checks are dropped: `readyz` overlaps `server_reachable`; `credentials` is now covered by the auth guard.

**Output modes**: `--json` (already shipped via `ctx.jsonMode`) emits the structured object. Human mode keeps today's `[OK]/[FAIL]` per-check format.

**Implementation default**: per-check 5s timeout via `AbortController`; aggregate cap 20s. If any check fails, exit 1. The `workspace_binding_valid` check skips (records `ok: false, detail: "no workspace selected"`) when `workspace_selected` already failed — no point hitting the server with an empty workspace id.

### §5 — Cleanup chore (optional in this milestone)

If trivial: namespace `login`/`logout` under `zombiectl auth login` / `zombiectl auth logout`. Keep `login`/`logout` as deprecated aliases that print a one-line warning and forward. Defer if it expands diff materially.

**Implementation default**: defer the rename to a follow-up if it touches >5 files. The auth subcommand namespace is a small UX polish, not load-bearing for the wedge.

---

## Interfaces

```
CLI:
  zombiectl install --from <path>
    POST /v1/workspaces/{ws}/zombies
      body: { trigger_markdown, source_markdown }
      → 201 { zombie_id, status }
      → 400 ERR_ZOMBIE_INVALID_CONFIG with hint on parse failure

  zombiectl doctor [--json]
    runs 3 checks, exit 0/1
    --json: stdout = { ok: bool, api_url: string, checks: [{name, ok, detail}] }
    requires auth (no longer exempt)
    workspace_binding probe: GET /v1/workspaces/{ws}/zombies

  zombiectl auth login   (optional, this milestone or next)
  zombiectl auth logout  (optional)

CLI auth guard:
  - AUTH_EXEMPT_ROUTES = { "login" }
  - All other routes (including doctor + install) require valid local token before HTTP call

Parser (server-side, single source of truth):
  TRIGGER.md frontmatter key:
    tools: [string]    ← canonical (not "skills")
  parseZombieFromTriggerMarkdown(markdown) → ZombieConfig
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| `zombiectl install`/`doctor` without local auth | User skipped `auth login` | Fail BEFORE HTTP call: AUTH_REQUIRED, "Run `zombiectl login` first." |
| Install TRIGGER.md frontmatter has no `name:` | User omitted required field | 400 `ERR_ZOMBIE_INVALID_CONFIG`, hint identifying the missing field |
| Install TRIGGER.md uses legacy `skills:` key | Old sample on disk | 400 `ERR_ZOMBIE_INVALID_CONFIG` — `MissingRequiredField` for `tools` |
| Install TRIGGER.md missing `---` fences | User pasted bare YAML | 400 `ERR_ZOMBIE_INVALID_CONFIG` — frontmatter scanner returns null |
| Doctor `server_reachable` fails | API URL wrong or service down | Exit 1; detail prints the URL tried + the error |
| Doctor `workspace_binding_valid` fails | Token revoked or workspace deleted | Exit 1; detail says "run `zombiectl workspace list` to reset" |

---

## Invariants

1. **`AUTH_EXEMPT_ROUTES = {"login"}`.** Every other command — including doctor and install — requires local credentials before any HTTP call.
2. **One canonical install POST shape.** `{trigger_markdown, source_markdown}`. The server is the single parser of YAML frontmatter. No backward-compat for the old shape — today's contract is broken end-to-end.
3. **One canonical parser key.** `tools:`. The shipped sample is the spec; the parser conforms.
4. **Doctor is idempotent and read-only.** Running it 100 times in a row makes no state changes; safe to call from skills repeatedly.
5. **No YAML parser in the CLI.** TRIGGER.md frontmatter parsing lives only in `src/zombie/yaml_frontmatter.zig` + `src/zombie/config_markdown.zig`. Adding a JS YAML parser is a regression.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_install_post_canonical_shape` | CLI sends `{trigger_markdown, source_markdown}` only (no `name`, no `config_json`) |
| `test_install_missing_name_400` | TRIGGER.md without `name:` → 400 ERR_ZOMBIE_INVALID_CONFIG |
| `test_install_no_local_auth_fails_before_http` | No token + `zombiectl install` → AUTH_REQUIRED, exits 1, no HTTP fired |
| `test_doctor_no_local_auth_fails_before_http` | No token + `zombiectl doctor` → AUTH_REQUIRED, exits 1, no HTTP fired |
| `test_parser_tools_key_accepted` | Shipped `samples/platform-ops/TRIGGER.md` parses cleanly via `parseZombieFromTriggerMarkdown` |
| `test_parser_legacy_skills_key_rejected` | Synthetic spec with `skills:` → `MissingRequiredField` |
| `test_zombie_config_json_round_trip` | `parse(serialize(config)) == config` — pins the new serializer |
| `test_doctor_all_pass` | Valid token + reachable server + selected ws + valid binding → exit 0, 3 checks all `ok: true` |
| `test_doctor_unreachable_fail` | API URL pointed at unreachable host → `server_reachable: false`, exit 1 |
| `test_doctor_no_workspace_fail` | No workspace selected → `workspace_selected: false`, binding check short-circuits |
| `test_doctor_invalid_binding_fail` | Token bound to workspace A, current selection is workspace B → `workspace_binding_valid: false` |
| `test_doctor_json_output` | `--json` emits `{ok, api_url, checks: [{name, ok, detail}, …]}` |
| `test_e2e_install_then_steer` | login → doctor → install (sample) → steer round-trip succeeds against test server |

---

## Acceptance Criteria

- [ ] `make test` passes new unit tests (zombiectl + Zig parser/serializer)
- [ ] `make test-integration` passes the install + doctor E2E tests
- [ ] Manual smoke: fresh laptop with no zombiectl state → `zombiectl login` → `zombiectl doctor` (3 checks green) → `zombiectl install --from samples/platform-ops/` → success
- [ ] Manual smoke: same flow without `login` → both `doctor` and `install` fail with `AUTH_REQUIRED` before any HTTP
- [ ] Codex P0 findings 1, 2, 3 (install contract, parser, auth-exempt) all resolved verifiably
- [ ] No YAML parser added to `zombiectl/` (no new dep, no hand-rolled YAML→JSON in JS)
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
- [ ] `make check-pg-drain` clean (no schema changes here, but baseline)
