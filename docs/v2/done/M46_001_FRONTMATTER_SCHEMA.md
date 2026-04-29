# M46_001: Frontmatter Schema — `x-usezombie:` Namespace + SKILL/TRIGGER Schema Cleanup

**Prototype:** v2.0.0
**Milestone:** M46
**Workstream:** 001
**Date:** Apr 25, 2026 (re-scoped Apr 29, 2026)
**Status:** DONE
**Priority:** P1 — packaging-blocking. M49 install-skill needs a stable, validated frontmatter contract to generate user files against. The current shape has runtime keys (`tools`, `credentials`, `network`, `budget`) at the top level of TRIGGER.md, which collides with any future skill-host that reads top-level metadata. Schema cleanup is a one-shot decision that everything downstream inherits.
**Categories:** SKILL, DOCS, API
**Batch:** B2 — after M40-M45 substrate lands, before M49 install-skill.
**Branch:** feat/m46-frontmatter-schema
**Depends on:** M44_001 (parser canonical key shape, `tools:`). M45_001 (credential structured types referenced from `x-usezombie.credentials`).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §8.1 (Authoring), §10 (capabilities — TRIGGER.md frontmatter entry).

---

## Decisions amended at PLAN (Apr 29, 2026)

After review with the user mid-PLAN, the spec was reshaped twice:

1. **Schema trim — keep only what the runtime needs before the LLM runs.** Non-negotiables under `x-usezombie:` are `trigger`, `tools`, `credentials`, `network`, `budget`. Drop `context:` (runtime defaults). Drop declarative `trigger.cron:` schedule blocks (agent self-schedules via `cron_add` tool — prose owns behavioral schedules). Rationale: the YAML is for security/cost boundaries and identity; everything else belongs in the body prose where the LLM can act on it.

2. **Two-file modality preserved (SOUL/contract split).** Initial spec proposed merging TRIGGER.md into SKILL.md frontmatter under `x-usezombie:`. Investigation showed the two-file model is wired end-to-end already: `loadSkillFromPath` reads both files, the install API takes `{trigger_markdown, source_markdown}` as separate fields, the DB stores them in separate columns, and only TRIGGER.md is ever parsed structurally — SKILL.md goes verbatim to the LLM as the system prompt. This maps cleanly to a SOUL (SKILL.md, agent prose) vs CONTRACT (TRIGGER.md, runtime config) separation worth preserving. **M46 keeps both files, applies schema discipline to each.**

3. **Validation policy split by ownership.** Top-level keys: **permissive** — accept unknown silently. `x-usezombie:` block: **rigid** — unknown subkey is an error. Typos in our own namespace must fail loud.

4. **Runtime keys forbidden at top level.** Parser rejects `tools:`/`credentials:`/`network:`/`budget:` at the top level of TRIGGER.md with `runtime_keys_outside_block` and an error pointing at the schema doc. v2.0.0 is the first release; the shipped sample is the only existing artifact and we update it in this spec.

5. **Cross-file invariant.** TRIGGER.md `name:` (top-level) must equal SKILL.md `name:` (top-level). Install rejects mismatch with `name_mismatch`. Single source of identity per zombie bundle.

6. **Fixtures path.** `samples/fixtures/frontmatter/` with `skill/` and `trigger/` subdirs. No milestone prefix in path.

7. **Mintlify doc bundled at CHORE(close).** `~/Projects/docs/snippets/skill-frontmatter.mdx` (or repo-confirmed path) gets the end-user schema reference + the "why YAML, not pure prose" rationale (sandbox boundaries / credential scope / pre-LLM decisions / auditability / why-not-prose). Content explicitly preserved from chat per user direction.

---

## Implementing agent — read these first

1. The Decisions block above — the two-file model is preserved; this is **schema cleanup only**, not a merger.
2. `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` — current shipped state. SKILL.md = soul (LLM prompt); TRIGGER.md = contract (runtime parses).
3. `src/zombie/config_markdown.zig:73` — `parseZombieFromTriggerMarkdown` parses ONLY trigger_markdown today. SKILL.md's frontmatter is currently decorative — runtime never reads it.
4. `src/http/handlers/zombies/create.zig:39-40` — install handler takes `{trigger_markdown, source_markdown}` as separate fields. M46 does not change this API shape.
5. `zombiectl/src/lib/load-skill-from-path.js` — reads both files. M46 extends to validate SKILL.md frontmatter (today it's read but not validated).
6. OpenAPI `x-*` extension convention — vendor-namespaced under-key, ignored by tooling that doesn't recognize it.

---

## Overview

**Goal (testable):** Two-file zombie bundle with disciplined frontmatter on each side.

- **SKILL.md** (the SOUL — LLM prompt). Top-level frontmatter carries authoring metadata only: `name`, `description`, `version` (required) + `tags`/`author`/`model`/`when_to_use` (optional pass-through). Body is prose. M46 adds parser validation; today the frontmatter is decorative.
- **TRIGGER.md** (the CONTRACT — runtime config). Top-level: `name` only (cross-file identity). All runtime keys (`trigger`, `tools`, `credentials`, `network`, `budget`) move under `x-usezombie:`. Runtime keys at the top level are rejected.

**Cross-file invariant:** SKILL.md `name:` == TRIGGER.md `name:`. Enforced at install.

**Problem solved:**
1. Today TRIGGER.md frontmatter is bespoke — runtime keys at the top level. Any skill-host that reads top-level metadata sees noise.
2. SKILL.md frontmatter is decorative — `name`/`description`/`version` aren't validated, drift silently between files.
3. No cross-file consistency check — a typo in SKILL.md's `name:` doesn't fail install.

**Solution:** clean schema for both files, namespaced runtime block in TRIGGER.md, cross-file invariant at install, mintlify doc updated at close.

**What this spec does NOT do** (deliberately):
- Does not merge SKILL.md and TRIGGER.md into one file.
- Does not change the install HTTP API (`{trigger_markdown, source_markdown}` stays).
- Does not change `core.zombies` table columns.
- Does not change `loadSkillFromPath` shape (still returns both halves).

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docs/SKILL_FRONTMATTER_SCHEMA.md` | NEW | Canonical schema reference: SKILL.md side + TRIGGER.md side + cross-file invariant |
| `src/zombie/yaml_frontmatter.zig` | EDIT | Support one extra nesting level (under `x-usezombie:`); reject duplicate keys |
| `src/zombie/config_parser.zig` | EDIT | Parse runtime keys from `x-usezombie:` subtree, not top level. Reject runtime keys at top level (`runtime_keys_outside_block`). Reject unknown subkeys (`unknown_runtime_key`) |
| `src/zombie/config_markdown.zig` | EDIT | Add `parseSkillMetadata` entry point for SKILL.md frontmatter. Existing `parseZombieFromTriggerMarkdown` stays — only the inner JSON shape it produces changes |
| `src/zombie/config_types.zig` | EDIT | Add `SkillMetadata` struct (small: name, description, version + optional pass-through). `ZombieConfig` unchanged in shape — only its source is now nested under `x-usezombie:` |
| `src/zombie/config_validate.zig` | EDIT | Add cross-field validators if needed; tighten rigid unknown-key check |
| `src/zombie/config.zig` | EDIT | Re-export `SkillMetadata` + `parseSkillMetadata` |
| `src/http/handlers/zombies/create.zig` | EDIT | Parse SKILL.md metadata + cross-file `name:` invariant. Reject `name_mismatch` |
| `samples/platform-ops/TRIGGER.md` | EDIT | Move runtime keys under `x-usezombie:`. Keep operator-facing comments. Keep `name:` at top level for cross-file check |
| `samples/platform-ops/SKILL.md` | EDIT (light) | Verify frontmatter has `name`, `description`, `version`. Already does — validate against schema |
| `samples/platform-ops/README.md` | EDIT (light) | Update any references to old key locations |
| `tests/integration/frontmatter_schema_test.zig` | NEW | Cover: x-usezombie: parse, runtime-keys-at-top-level rejection, unknown runtime key error, name_mismatch, SKILL metadata required fields |
| `samples/fixtures/frontmatter/skill/` | NEW | `minimal.md`, `missing_name.md`, `extra_top_level.md` (passes — permissive) |
| `samples/fixtures/frontmatter/trigger/` | NEW | `minimal.md`, `full.md`, `runtime_at_top_level.md` (rejected), `unknown_runtime_key.md` (rejected) |
| `samples/fixtures/frontmatter/bundles/` | NEW | `name_mismatch/` (SKILL+TRIGGER pair, install rejects) |
| `~/Projects/docs/snippets/skill-frontmatter.mdx` (cross-repo) | EDIT/NEW | End-user schema reference + "why YAML not prose" rationale; updated at CHORE(close) |

---

## Sections (implementation slices)

### §1 — Canonical schema definition (`docs/SKILL_FRONTMATTER_SCHEMA.md`)

#### SKILL.md (the SOUL)

```yaml
---
# Authoring metadata. Permissive: unknown keys pass through silently.
name: platform-ops-zombie              # required, /^[a-z0-9-]+$/, 1-64 chars; must match TRIGGER.md
description: <one-line>                # required, <= 200 chars
version: 0.1.0                         # required, semver
when_to_use: <one-line>                # optional, pass-through
tags: [platform-ops, diagnostics]      # optional, pass-through
author: usezombie                      # optional, pass-through
model: claude-sonnet-4-6               # optional, pass-through hint
---

# Body — plain markdown prose. The agent's reasoning prompt. Runtime
# never structurally reads this; it is dropped into the LLM's system
# prompt at execution time.
```

#### TRIGGER.md (the CONTRACT)

```yaml
---
# Top-level: identity only. Must match SKILL.md `name:`.
name: platform-ops-zombie

# x-usezombie: runtime config. Rigid: unknown subkey is an error.
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
    - fly         # name; must exist in vault with M45 structured type
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

# Body of TRIGGER.md is operator-facing commentary — kept for human
# documentation of credential shapes, budget reasoning, and firewall
# behavior. Not consumed by runtime or LLM.
```

**Validation policy:**
- SKILL.md top-level: required = `name`, `description`, `version`. Optional pass-through (parsed but not validated): `when_to_use`, `tags`, `author`, `model`. Unknown keys: silent pass-through.
- TRIGGER.md top-level: required = `name`. Required nested = `x-usezombie:`. Unknown top-level keys other than `name`/`x-*`: silent pass-through (forward compat). **Runtime keys at top level** (`tools:`, `credentials:`, `network:`, `budget:`, `trigger:` outside the `x-usezombie:` block): hard error, `runtime_keys_outside_block`.
- TRIGGER.md `x-usezombie:` block: required subkeys = `trigger`, `tools`, `credentials`, `network`, `budget`. Unknown subkey: hard error, `unknown_runtime_key`.
- Cross-file: SKILL.md `name:` == TRIGGER.md `name:`, else `name_mismatch` at install handler.
- Other `x-*:` blocks (e.g. `x-amp:`): parsed structurally but ignored — belong to other vendors.

### §2 — Parser update

`yaml_frontmatter.zig`: extend the YAML→JSON converter to handle one additional nesting level under `x-usezombie:`. Today it does top-level + one-level-nested (e.g. `network: { allow: [...] }`). Post-M46 the runtime block is itself nested, so `network.allow` becomes 3-deep. Reject duplicate keys at any level.

`config_parser.zig`: change `parseZombieConfig` to look for runtime keys *inside* the `x-usezombie` object on the parsed root, not at root. Detect runtime keys at the top level and surface `runtime_keys_outside_block` with the offending key name. Detect unknown subkeys under `x-usezombie:` and surface `unknown_runtime_key`.

`config_markdown.zig`: add `parseSkillMetadata(alloc, source_markdown) → SkillMetadata`. Reads SKILL.md frontmatter only — does not interpret the body. Returns owned struct. Existing `parseZombieFromTriggerMarkdown` stays as-is for the runtime side; only the inner JSON shape it produces changes.

`create.zig` (HTTP handler): after parsing both halves, assert `skill_metadata.name == zombie_config.name`; reject `name_mismatch` otherwise.

### §3 — Sample frontmatter cleanup

`samples/platform-ops/TRIGGER.md`:
- Top-level keeps `name: platform-ops-zombie`.
- Move `trigger`, `tools`, `credentials`, `network`, `budget` under `x-usezombie:`.
- Keep all the operator-facing inline comments (credential shapes, budget rationale, firewall behavior). They migrate with their keys.

`samples/platform-ops/SKILL.md`:
- Frontmatter already has `name`, `description`, `version`, `tags`, `author`, `model` — verify it parses cleanly under the new validator.
- Body unchanged.

`samples/platform-ops/README.md`:
- Light edits if it documents key locations explicitly. No structural rewrite.

### §4 — Fixtures + tests

`samples/fixtures/frontmatter/skill/`:
- `minimal.md` — required keys only
- `missing_name.md` — no `name:` → rejects with `missing_required_field`
- `extra_top_level.md` — adds `x-amp: { foo: bar }` and unknown `whatever:` → passes (permissive)

`samples/fixtures/frontmatter/trigger/`:
- `minimal.md` — `name:` + `x-usezombie:` with all required runtime subkeys
- `full.md` — every runtime field set
- `runtime_at_top_level.md` — `tools:` at top level → `runtime_keys_outside_block`
- `unknown_runtime_key.md` — `x-usezombie.contxt: ...` (typo) → `unknown_runtime_key`

`samples/fixtures/frontmatter/bundles/`:
- `name_mismatch/SKILL.md` + `TRIGGER.md` with different `name:` values → install handler rejects `name_mismatch`

`tests/integration/frontmatter_schema_test.zig`:
- `test_canonical_sample_parses` — `samples/platform-ops/` post-update
- `test_runtime_keys_at_top_level_rejected`
- `test_unknown_runtime_key_rejected`
- `test_skill_missing_name_rejected`
- `test_unknown_top_level_key_passes`
- `test_name_mismatch_rejected_at_install`

### §5 — Mintlify doc

`~/Projects/docs/snippets/skill-frontmatter.mdx` (locate exact path during EXECUTE — could be `pages/`, `concepts/`, etc.):
- Schema reference for SKILL.md and TRIGGER.md as authored by end users.
- "Why YAML and not pure prose" — preserve from chat verbatim:
  1. Sandbox boundaries (`network.allow:` is a kernel-level egress rule)
  2. Credential scope (static declaration = static blast radius)
  3. Pre-LLM decisions (cron / budget / trigger registration happens before any model call)
  4. Auditability (5-second scan: spend cap, domains, credentials)
  5. The natural-language alternative reinvents YAML internally (extracted struct + drift)
- "SKILL.md = SOUL, TRIGGER.md = CONTRACT" mental model — explains the two-file design.

---

## Interfaces

```
Parser API (Zig):
  parseSkillMetadata(alloc, skill_md: []const u8) → SkillMetadata
  parseZombieFromTriggerMarkdown(alloc, trigger_md: []const u8) → ZombieConfig    // unchanged signature
                                                                                   // inner: x-usezombie: subtree

ParseError variants:
  - missing_required_field: { field: string, file: skill | trigger }
  - usezombie_block_required                            // x-usezombie: missing in TRIGGER.md
  - unknown_runtime_key: { key: string }                // unknown subkey under x-usezombie:
  - runtime_keys_outside_block: { key: string }           // tools/credentials/etc at top of TRIGGER.md
  - duplicate_key: { path: string }
  - name_mismatch: { skill_name, trigger_name }         // surfaced at install handler

CLI behavior (zombiectl install --from <dir>) — UNCHANGED SHAPE:
  - reads SKILL.md + TRIGGER.md (both required)
  - POSTs { trigger_markdown, source_markdown }
  - server-side validation surfaces the new errors
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Missing `name:` in SKILL.md | Author error | `missing_required_field { field: name, file: skill }` |
| `x-usezombie:` block missing in TRIGGER.md | Pre-M46 file | `usezombie_block_required` |
| `tools:` at top level of TRIGGER.md | Pre-M46 file or copy-paste | `runtime_keys_outside_block { key: "tools" }` |
| Unknown subkey under `x-usezombie:` | Typo or future-version key | `unknown_runtime_key { key }` |
| SKILL.md `name:` ≠ TRIGGER.md `name:` | Authoring drift | `name_mismatch` at install |
| Credential name in vault missing | Operator forgot to add | First event fails `secret_not_found` (M41 surfaces) |

---

## Invariants

1. **Two files per zombie bundle.** SKILL.md (soul) + TRIGGER.md (contract). Install requires both.
2. **Runtime config is namespaced.** All runtime keys under `x-usezombie:` in TRIGGER.md.
3. **SKILL.md frontmatter is authoring metadata only.** Never carries runtime config.
4. **Cross-file identity.** `name:` matches across both files.
5. **Schema docs match parser.** Every key documented in the schema doc is accepted by the parser; every parser rejection has a matching documented failure mode.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_parse_canonical_sample` | `samples/platform-ops/` post-update parses cleanly via both parser entries |
| `test_skill_minimal` | SKILL.md with just `name`/`description`/`version` parses |
| `test_skill_missing_name_rejected` | SKILL.md without `name:` → `missing_required_field` |
| `test_skill_unknown_top_level_passes` | SKILL.md with extra unknown top-level key → silent pass-through |
| `test_trigger_minimal` | TRIGGER.md with `name:` + minimal `x-usezombie:` parses |
| `test_trigger_full` | Every optional runtime field populated |
| `test_runtime_keys_outside_block_rejected` | `tools:` at top of TRIGGER.md → `runtime_keys_outside_block` |
| `test_unknown_runtime_key_rejected` | `x-usezombie.contxt:` → `unknown_runtime_key` |
| `test_name_mismatch_rejected` | Install with mismatched `name:` across files → `name_mismatch` |
| `test_yaml_two_levels_nesting` | `x-usezombie.network.allow:` (3 levels deep counting root) parses correctly |

---

## Acceptance Criteria

- [ ] `make test` passes new parser unit tests
- [ ] `make test-integration` passes install + schema integration tests
- [ ] `samples/platform-ops/TRIGGER.md` runtime keys are under `x-usezombie:`; operator comments preserved
- [ ] `samples/platform-ops/SKILL.md` frontmatter validates under new SKILL metadata parser
- [ ] `docs/SKILL_FRONTMATTER_SCHEMA.md` written and reviewed
- [ ] Cross-file `name:` invariant enforced at install handler with negative test
- [ ] Cross-compile clean: `x86_64-linux` + `aarch64-linux`
- [ ] No regressions in shipped sample integration tests
- [ ] `~/Projects/docs/snippets/skill-frontmatter.mdx` updated with schema reference + "why YAML not prose" rationale + SOUL/CONTRACT mental model
- [ ] Trim invariant: no `context:` or `cron:` schedule keys appear in schema doc, sample, fixtures, or parser surface
- [ ] No-merger invariant: HTTP API still takes `{trigger_markdown, source_markdown}` separately; DB columns unchanged; `loadSkillFromPath` still returns both halves
