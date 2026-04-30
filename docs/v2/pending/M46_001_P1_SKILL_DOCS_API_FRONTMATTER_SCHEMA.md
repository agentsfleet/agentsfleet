# M46_001: Frontmatter Schema — gstack-Conformant + `x-usezombie:` Extension, TRIGGER.md → SKILL.md Merge

**Prototype:** v2.0.0
**Milestone:** M46
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — packaging-blocking. The install-skill (M49) consumes a single SKILL.md; users authoring zombies in Claude Code want one file, not two. The current bespoke top-level frontmatter collides with gstack tooling. Conforming to gstack + namespacing the runtime config under `x-usezombie:` is a one-shot schema decision the ecosystem inherits forever.
**Categories:** SKILL, DOCS, API
**Batch:** B2 — after M40-M45 substrate lands, before M49 install-skill.
**Branch:** feat/m46-frontmatter-schema (to be created)
**Depends on:** M44_001 (parser conforms to canonical key shape — `tools:` etc.). M45_001 (credential `type` discriminator referenced from `x-usezombie.credentials`).

**Canonical architecture:** `docs/architecture/` §8.1 (Authoring), §10 (capabilities — TRIGGER.md row mentions merged frontmatter), §11 (context knobs live under `x-usezombie.context`).

---

## Implementing agent — read these first

1. `~/.claude/skills/gstack/` directory — read 3-5 existing gstack skill SKILL.md files for the canonical top-level frontmatter shape (`name`, `description`, `when_to_use`, `tags`, `author`, `version`, `model`).
2. https://github.com/resend/resend-cli/tree/main#agent-skills — Resend's pattern for vendor extensions.
3. `samples/platform-ops/SKILL.md` + `samples/platform-ops/TRIGGER.md` — current shipped state (two files, bespoke top-level keys).
4. OpenAPI spec — `x-*` extension key convention pattern that this borrows from.
5. `src/zombie/config_parser.zig` — current parser (post-M44 with `tools:` not `skills:`).

---

## Overview

**Goal (testable):** A zombie is defined by ONE file (`SKILL.md`) with frontmatter that splits cleanly:
- **Top-level keys** match gstack's schema and are readable by any gstack-aware agent (Claude Code, Amp, OpenCode).
- **`x-usezombie:` block** namespaces all usezombie-specific runtime config (trigger, tools, credentials, network, budget, context). gstack tooling ignores anything under `x-*`.

After M46, `samples/platform-ops/` ships as a single `SKILL.md` (with `README.md` for human docs) — `TRIGGER.md` is gone. Existing `tools:` / `credentials:` / `network:` / `budget:` keys move from top-level into `x-usezombie:`. Parser conforms.

**Problem:** Today the shipped `samples/platform-ops/TRIGGER.md` declares `tools:`, `credentials:`, `network:`, `budget:` at top level. gstack's convention reserves the top level for cross-agent metadata. Users who load the SKILL.md into Claude Code see noisy "unknown key" warnings; tools that strictly enforce gstack reject the file. Two files (SKILL + TRIGGER) doubles the authoring surface.

**Solution summary:** Define the canonical schema. Provide a one-shot migration that merges TRIGGER.md into SKILL.md frontmatter under `x-usezombie:`. Update the parser to read the new shape. Update the shipped sample. Document it in `docs.usezombie.com` so the install-skill (M49) generates conformant files.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docs/SKILL_FRONTMATTER_SCHEMA.md` | NEW | Canonical schema reference: every key, every type, every default |
| `src/zombie/config_parser.zig` | EDIT | Read top-level + `x-usezombie:` block; surface gstack metadata + runtime config separately |
| `samples/platform-ops/SKILL.md` | EDIT | Merge TRIGGER.md frontmatter under `x-usezombie:` |
| `samples/platform-ops/TRIGGER.md` | DELETE | Folded into SKILL.md |
| `samples/platform-ops/README.md` | EDIT | Update credential add commands to reference single SKILL.md |
| `zombiectl/src/commands/zombie.js` | EDIT | Install reads single SKILL.md; no longer expects sibling TRIGGER.md (backward shim: if TRIGGER.md present, merge into SKILL.md frontmatter at install time + warn deprecation) |
| `tests/integration/frontmatter_schema_test.zig` | NEW | Parse canonical sample, parse legacy, parse missing required fields |
| `samples/fixtures/m46-frontmatter-fixtures/` | NEW | Several test SKILL.md files: minimal, full, legacy, broken |

---

## Sections (implementation slices)

### §1 — Canonical schema definition

`docs/SKILL_FRONTMATTER_SCHEMA.md`:

```yaml
---
# Top-level — gstack-conformant. Every gstack-aware host reads these.
name: platform-ops-zombie              # required, /^[a-z0-9-]+$/, 1-64 chars
description: <one-line>                # required, <= 200 chars
when_to_use: <one-line>                # optional but recommended
tags: [platform-ops, diagnostics]      # optional
author: usezombie                      # optional
version: 0.1.0                         # required, semver
model: claude-sonnet-4-6               # optional, hint to runtime; defaults to platform-selected

# x-usezombie: — runtime config. Ignored by all non-usezombie tooling.
x-usezombie:
  trigger:
    types: [chat, webhook:github, cron]   # one or more
    webhook:
      github:
        events: [workflow_run]             # which GH event types
        conditions: [conclusion=failure]   # filter
    cron:                                  # optional; agent can also self-schedule via cron_add
      - schedule: "*/30 * * * *"
        message: "post-recovery health check"
  tools:
    - http_request
    - memory_store
    - memory_recall
    - cron_add
    - cron_list
    - cron_remove
  credentials:
    - fly       # name, must exist in vault under same name with type=fly
    - upstash
    - slack
    - github
  network:
    allow:
      - api.machines.dev
      - api.upstash.com
      - slack.com
      - api.github.com
  budget:
    daily_dollars: 1.00
    monthly_dollars: 8.00
  context:
    tool_window: auto                # auto | int
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75
---

# Body of SKILL.md is plain markdown — the agent's reasoning prompt.
```

**Implementation default**: any unrecognized key under `x-usezombie:` is a warning (not an error). Forward-compat for v3 fields.

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

For user repos with old `.usezombie/platform-ops/SKILL.md + TRIGGER.md`: install-skill (M49) detects two files, offers to merge automatically. Backward shim in `zombiectl install`: if `--from <path>` contains both, log deprecation warning and merge before sending.

### §4 — Shipped sample update

`samples/platform-ops/SKILL.md` — merge TRIGGER.md content under `x-usezombie:`. Keep README.md updated to reference single file. Test: `zombiectl install --from samples/platform-ops/` succeeds against fresh workspace.

### §5 — Schema doc + examples

`docs/SKILL_FRONTMATTER_SCHEMA.md`: human-readable reference. Each key documented with type, default, example, where it's enforced (LLM prompt vs sandbox boundary). This is what M49 install-skill reads to generate user files.

`samples/fixtures/m46-frontmatter-fixtures/`:
- `minimal.md` — just required keys
- `full.md` — every optional key set
- `legacy.md` — bespoke top-level (parser should reject post-M46 with hint)
- `broken_no_name.md` — missing required field
- `broken_unknown_x_usezombie.md` — unknown key under runtime config (warning, not error)

---

## Interfaces

```
Parser API (Zig):
  parseSkillMd(content: []const u8) → struct {
    metadata: GstackMetadata { name, description, when_to_use?, tags?, author?, version, model? },
    runtime: UsezombieRuntime { trigger, tools, credentials, network, budget, context },
  } | ParseError

ParseError variants:
  - missing_required_field: { field: string }
  - usezombie_block_required
  - duplicate_key: { path: string }
  - unknown_credential_type: { name: string }   (cross-checks against vault registry)

CLI behavior:
  zombiectl install --from <path>
    if path contains BOTH SKILL.md and TRIGGER.md (legacy):
      log deprecation: "TRIGGER.md is folded into SKILL.md frontmatter — please migrate"
      auto-merge in memory before POST
    else (only SKILL.md):
      parse normally, send canonical body
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Missing `name:` | User wrote SKILL without it | `missing_required_field` with hint |
| `x-usezombie:` block missing | Plain markdown without runtime config | `usezombie_block_required` with hint pointing at schema doc |
| Unknown top-level key | User used `tools:` at top instead of under `x-usezombie:` | Error: "Move `tools:` under `x-usezombie:` — see schema doc" |
| Both SKILL.md and TRIGGER.md present | Legacy on-disk state | Backward shim merges + warns |
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
| `test_legacy_files_backward_shim` | SKILL.md + TRIGGER.md both present → install merges + warns |
| `test_unknown_x_usezombie_key_warns_only` | Unknown subkey → warning, not error; rest parses |
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
