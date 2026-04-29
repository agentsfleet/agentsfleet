# M46_001: Frontmatter Schema — Authoring Metadata + `x-usezombie:` Runtime Block, TRIGGER.md → SKILL.md Merge

**Prototype:** v2.0.0
**Milestone:** M46
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — packaging-blocking. The install-skill (M49) consumes a single SKILL.md; users authoring zombies in Claude Code want one file, not two. The current bespoke top-level frontmatter collides with skill-host tooling. Splitting authoring metadata (top-level) from runtime config (`x-usezombie:`) is a one-shot schema decision the ecosystem inherits forever.
**Categories:** SKILL, DOCS, API
**Batch:** B2 — after M40-M45 substrate lands, before M49 install-skill.
**Branch:** feat/m46-frontmatter-schema
**Depends on:** M44_001 (parser conforms to canonical key shape — `tools:` etc.). M45_001 (credential `type` discriminator referenced from `x-usezombie.credentials`).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §8.1 (Authoring), §10 (capabilities — TRIGGER.md row mentions merged frontmatter).

---

## Decisions amended at PLAN (2026-04-29)

After review with the user, the spec was trimmed and tightened before EXECUTE began:

1. **Schema trim — keep only what the runtime needs before the LLM runs.** The non-negotiables under `x-usezombie:` are `trigger`, `tools`, `credentials`, `network`, `budget`. Drop `context:` (let the runtime compute defaults; per-skill tuning is premature). Drop `cron:` schedule blocks under `trigger:` (the agent self-schedules via the `cron_add` tool — keeping a parallel declarative path doubles the surface). Rationale: the YAML is for security/cost boundaries and identity; everything else belongs in the body prose where the LLM can act on it.
2. **Two-struct split.** Parser returns `(SkillMetadata, ZombieRuntime)`. Top-level metadata (`name`, `description`, `version`, optional `tags`/`author`/`model`/`when_to_use`) is one struct; `x-usezombie:` runtime config is the other. Different callers want different halves — splitting prevents drift.
3. **Validation policy split by ownership.** Top-level keys: **permissive** — accept unknown silently (we don't own the namespace; future skill-hosts may add fields). `x-usezombie:` block: **rigid** — unknown subkey is an error, not a warning. Typos must fail loud since we own this namespace.
4. **Hard break, no legacy shim.** v2.0.0 is the first release; there is no pre-v2 user base to preserve. zombiectl install rejects two-file (SKILL.md + TRIGGER.md) layouts with a clear error pointing at the schema doc. No backward-compat merge path.
5. **Fixtures path.** Drop the milestone-prefixed directory name. Use `samples/fixtures/frontmatter/`.
6. **Mintlify doc bundled.** `~/Projects/docs/snippets/skill-frontmatter.mdx` (or equivalent existing path — confirm during EXECUTE) is updated as part of this spec, at CHORE(close). It carries the user-facing "why YAML and not pure prose" rationale (sandbox boundaries, credential scope, pre-LLM decisions, auditability, why-not-prose) — content the user explicitly asked to preserve from chat.
7. **Top-level field surface — minimal binding.** Required: `name`, `description`, `version`. Optional pass-through (parsed but not validated or used by runtime): `tags`, `author`, `model`, `when_to_use`. usezombie does not depend on any cross-host skill convention; the split exists for cleanliness and forward compat, not to bind to a specific external ecosystem.

---

## Implementing agent — read these first

1. The amended Decisions block above — trim, two-struct split, validation policy, hard break.
2. `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` — current shipped state (two files, runtime keys at top level) — the migration target.
3. OpenAPI `x-*` extension convention — the pattern `x-usezombie:` borrows from. Vendor-namespaced under-key, ignored by tooling that doesn't recognize it.
4. `src/zombie/config_parser.zig` + `src/zombie/yaml_frontmatter.zig` + `src/zombie/config_markdown.zig` — current parser pipeline (markdown → YAML→JSON → struct). M46 adds one nesting level under `x-usezombie:` and splits the struct.
5. `src/zombie/event_loop.zig:75` — the production caller that consumes parsed config; signature changes ripple here.

---

## Overview

**Goal (testable):** A zombie is defined by ONE file (`SKILL.md`) with frontmatter that splits cleanly:
- **Top-level keys** carry authoring metadata (`name`, `description`, `version` + optional pass-through fields). Permissive — unknown keys pass through silently, leaving room for other skill-host vendors to add their own fields.
- **`x-usezombie:` block** namespaces all usezombie-specific runtime config (`trigger`, `tools`, `credentials`, `network`, `budget`). Rigid — unknown subkey is an error so typos fail loud.

After M46, `samples/platform-ops/` ships as a single `SKILL.md` (with `README.md` for human docs) — `TRIGGER.md` is gone. Existing `tools:` / `credentials:` / `network:` / `budget:` keys move from top-level into `x-usezombie:`. Parser conforms.

**Problem:** Today the shipped `samples/platform-ops/TRIGGER.md` declares `tools:`, `credentials:`, `network:`, `budget:` at top level. Top-level should be reserved for authoring metadata that other tooling can read; mixing runtime config there blocks portability and produces noisy "unknown key" warnings in skill-aware hosts. Two files (SKILL + TRIGGER) also doubles the authoring surface.

**Solution summary:** Define the canonical schema (top-level metadata vs `x-usezombie:` runtime block). Migrate the shipped sample to a single SKILL.md. Update the parser to return `(SkillMetadata, ZombieRuntime)` separately. Document the schema in the repo (`docs/SKILL_FRONTMATTER_SCHEMA.md`) and on `docs.usezombie.com` so M49 install-skill generates conformant files.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docs/SKILL_FRONTMATTER_SCHEMA.md` | NEW | Canonical schema reference: every key, every type, every default |
| `src/zombie/config_types.zig` | EDIT | Add `SkillMetadata` struct; rename `ZombieConfig`→`ZombieRuntime` (or split fields out); update deinit |
| `src/zombie/config_parser.zig` | EDIT | Parse top-level metadata + `x-usezombie:` subtree; return `(metadata, runtime)` pair |
| `src/zombie/config_validate.zig` | EDIT | Validate runtime block; rigid unknown-key check inside `x-usezombie:` |
| `src/zombie/config_markdown.zig` | EDIT | Update `parseZombieFromTriggerMarkdown` (rename or new entry) to return both halves |
| `src/zombie/config.zig` | EDIT | Re-export new types/entry points |
| `src/zombie/yaml_frontmatter.zig` | EDIT | Support 2 levels of nesting (under `x-usezombie:`); reject duplicate keys |
| `src/zombie/event_loop.zig` | EDIT | Adapt caller to new parser shape |
| `samples/platform-ops/SKILL.md` | EDIT | Merge TRIGGER.md frontmatter under `x-usezombie:` |
| `samples/platform-ops/TRIGGER.md` | DELETE | Folded into SKILL.md |
| `samples/platform-ops/README.md` | EDIT | Update to reference single SKILL.md |
| `zombiectl/src/commands/zombie.js` | EDIT | Install reads single SKILL.md; reject legacy two-file layout with hard error pointing at schema doc |
| `tests/integration/frontmatter_schema_test.zig` | NEW | Parse canonical sample, reject legacy, error on missing required fields |
| `samples/fixtures/frontmatter/` | NEW | 5 test SKILL.md files: minimal, full, legacy, broken_no_name, broken_unknown_x_usezombie |
| `~/Projects/docs/snippets/skill-frontmatter.mdx` (cross-repo) | EDIT/NEW | User-facing schema reference + "why YAML not prose" rationale; updated at CHORE(close) |

---

## Sections (implementation slices)

### §1 — Canonical schema definition

`docs/SKILL_FRONTMATTER_SCHEMA.md`:

```yaml
---
# Top-level — authoring metadata. Permissive: unknown keys pass through silently.
name: platform-ops-zombie              # required, /^[a-z0-9-]+$/, 1-64 chars
description: <one-line>                # required, <= 200 chars
version: 0.1.0                         # required, semver
when_to_use: <one-line>                # optional, pass-through (not validated, not used by runtime)
tags: [platform-ops, diagnostics]      # optional, pass-through
author: usezombie                      # optional, pass-through
model: claude-sonnet-4-6               # optional, pass-through hint

# x-usezombie: — runtime config. Rigid: unknown keys are an error.
x-usezombie:
  trigger:
    types: [chat]                          # one or more of: chat, webhook, cron, api, chain
  tools:
    - http_request
    - memory_store
    - memory_recall
    - cron_add
    - cron_list
    - cron_remove
  credentials:
    - fly       # name, must exist in vault under same name with M45 structured type
    - upstash
    - slack
  network:
    allow:
      - api.machines.dev
      - api.upstash.com
      - slack.com
  budget:
    daily_dollars: 1.00
    monthly_dollars: 8.00
---

# Body of SKILL.md is plain markdown — the agent's reasoning prompt. Schedules,
# trigger filters, and run-time tuning live here as prose; the agent acts on
# them via the cron_add / memory_* tools.
```

**Validation policy:**
- Top-level: required = `name`, `description`, `version`. Optional pass-through (parsed but not validated): `when_to_use`, `tags`, `author`, `model`. Unknown top-level keys: silent pass-through.
- `x-usezombie:` block: required = `trigger`, `tools`, `credentials`, `network`, `budget`. Unknown subkey: **error** (`unknown_runtime_key`). The block itself is required.
- Other `x-*:` blocks (e.g. `x-amp:`): parsed, ignored — belong to other vendors.

### §2 — Parser update

`src/zombie/config_parser.zig`:

1. Parse YAML frontmatter once.
2. Validate required top-level keys (`name`, `description`, `version`).
3. Extract `x-usezombie:` subtree as `runtime_config`.
4. If `runtime_config` missing → error `usezombie_block_required`.
5. Validate `runtime_config` structure (each key, type).
6. Return `(metadata: gstack_meta, runtime: usezombie_runtime)` pair.

**Implementation default**: use existing YAML library. Reject duplicate keys at any level.

### §3 — Migration: TRIGGER.md → SKILL.md folder

For shipped samples (only `samples/platform-ops/` today; future samples follow the same merge pattern):

1. Read existing TRIGGER.md.
2. Read existing SKILL.md frontmatter.
3. Move TRIGGER's top-level fields under `x-usezombie:` in SKILL.md.
4. Delete TRIGGER.md.

For user repos: v2.0.0 is the first release; there is no pre-v2 user state to migrate. `zombiectl install --from <path>` rejects two-file (SKILL.md + TRIGGER.md) layouts with a hard error pointing at `docs/SKILL_FRONTMATTER_SCHEMA.md`. No backward shim. (M49 install-skill generates conformant single-file output from inception.)

### §4 — Shipped sample update

`samples/platform-ops/SKILL.md` — merge TRIGGER.md content under `x-usezombie:`. Keep README.md updated to reference single file. Test: `zombiectl install --from samples/platform-ops/` succeeds against fresh workspace.

### §5 — Schema doc + examples

`docs/SKILL_FRONTMATTER_SCHEMA.md`: human-readable reference. Each key documented with type, default, example, where it's enforced (LLM prompt vs sandbox boundary). This is what M49 install-skill reads to generate user files.

`samples/fixtures/frontmatter/`:
- `minimal.md` — just required keys (top-level + minimal `x-usezombie:`)
- `full.md` — every optional top-level key set + full `x-usezombie:` block
- `legacy.md` — bespoke top-level (`tools:`, `credentials:` at top) → parser rejects with `legacy_top_level_runtime`
- `broken_no_name.md` — missing required field → `missing_required_field`
- `broken_unknown_x_usezombie.md` — unknown subkey under runtime config → `unknown_runtime_key` (rigid, not a warning)

---

## Interfaces

```
Parser API (Zig):
  parseSkillMd(alloc, content: []const u8) → ParsedSkill {
    metadata: SkillMetadata { name, description, version, when_to_use?, tags?, author?, model? },
    runtime: ZombieRuntime { trigger, tools, credentials, network, budget },
  } | ParseError

ParseError variants:
  - missing_required_field: { field: string }       // top-level or runtime
  - usezombie_block_required                        // x-usezombie: missing
  - unknown_runtime_key: { key: string }            // unknown subkey under x-usezombie:
  - duplicate_key: { path: string }
  - legacy_top_level_runtime: { key: string }       // tools/credentials/etc at top level

CLI behavior:
  zombiectl install --from <path>
    if path contains BOTH SKILL.md and TRIGGER.md:
      hard error: "Two-file layout (SKILL.md + TRIGGER.md) is not supported.
                   See docs/SKILL_FRONTMATTER_SCHEMA.md for the canonical single-file schema."
      exit non-zero
    else if only SKILL.md:
      parse normally, POST canonical body
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Missing `name:` | User wrote SKILL without it | `missing_required_field` with hint |
| `x-usezombie:` block missing | Plain markdown without runtime config | `usezombie_block_required` with hint pointing at schema doc |
| Unknown top-level key | User used `tools:` at top instead of under `x-usezombie:` | Error: "Move `tools:` under `x-usezombie:` — see schema doc" |
| Both SKILL.md and TRIGGER.md present | Legacy on-disk state | Hard error pointing at schema doc; no auto-merge |
| Unknown subkey under `x-usezombie:` | Typo or future-version key | `unknown_runtime_key` error |
| Credential name in `x-usezombie.credentials:` not in vault | Operator forgot to add credential | Install succeeds, but first event fails with `secret_not_found` (M41 surfaces this) |

---

## Invariants

1. **One file per zombie.** Post-M46, a zombie is defined by one SKILL.md. TRIGGER.md exists only in legacy on-disk state.
2. **Runtime config is namespaced.** All usezombie-specific keys under `x-usezombie:`. No top-level pollution.
3. **gstack-aware tools see only metadata.** `name`, `description`, `tags`, etc. — usable as a gstack skill.
4. **Schema docs match parser.** If schema doc says key X exists, parser accepts it. Tested.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_parse_canonical_full` | `samples/platform-ops/SKILL.md` parses cleanly into metadata + runtime |
| `test_parse_minimal` | Just required keys → metadata populated, runtime has only required fields |
| `test_missing_name_rejected` | No `name:` → ParseError.missing_required_field |
| `test_legacy_top_level_tools_rejected` | `tools:` at top level → error with migration hint |
| `test_legacy_two_files_rejected` | SKILL.md + TRIGGER.md both present → zombiectl install hard-errors, points at schema doc |
| `test_unknown_x_usezombie_key_errors` | Unknown subkey → `unknown_runtime_key` error |
| `test_unknown_top_level_key_passes` | Unknown top-level key → silent pass-through, parse succeeds |
| `test_install_canonical_sample` | E2E: `zombiectl install --from samples/platform-ops/` against fresh ws → success |
| `test_credentials_referenced_must_exist_at_run` | Sample references `slack` credential not in vault → install OK; first event fails `secret_not_found` |

---

## Acceptance Criteria

- [ ] `make test` passes new parser unit tests
- [ ] `make test-integration` passes the install + legacy-shim tests
- [ ] `samples/platform-ops/` ships as ONE SKILL.md (TRIGGER.md deleted)
- [ ] `docs/SKILL_FRONTMATTER_SCHEMA.md` written and reviewed
- [ ] M49 install-skill generates conformant SKILL.md per the schema
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
- [ ] No regressions in shipped sample integration tests
- [ ] `~/Projects/docs/snippets/skill-frontmatter.mdx` (or repo-confirmed path) updated with end-user schema reference + "why YAML, not pure prose" rationale (sandbox boundaries, credential scope, pre-LLM decisions, auditability, why-not-prose)
- [ ] Trim invariant: no `context:` or `cron:` schedule keys appear in schema doc, sample, fixtures, or parser surface
