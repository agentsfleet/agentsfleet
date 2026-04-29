# SKILL.md / TRIGGER.md Frontmatter Schema

Canonical reference for the YAML frontmatter on the two files that make up a zombie bundle. The parser, integration tests, and `M49 install-skill` generator all derive from this document.

> Audience: implementing agents, parser authors, test writers. End-user docs live at `docs.usezombie.com/concepts/skill-frontmatter`.

---

## The two-file model

A zombie bundle is a directory containing exactly two files:

- **`SKILL.md`** — the **SOUL**. Authoring metadata in the frontmatter; prose body that becomes the agent's system prompt at execution time. The runtime never structurally interprets the body.
- **`TRIGGER.md`** — the **CONTRACT**. Identity at the top level; runtime configuration (security/cost boundaries, tool grants, trigger sources) under an `x-usezombie:` block. The runtime parses this file before any LLM call.

The split exists because the two files have different audiences and different review bars. SKILL.md is iterated like a prompt — frequent edits, prose review. TRIGGER.md is a security/cost contract — every edit is privileged (network egress, credential scope, budget caps) and benefits from file-level CODEOWNERS, `git log` per concern, and standalone audit.

---

## SKILL.md schema

```yaml
---
name: platform-ops-zombie              # required
description: Diagnoses platform health from fly.io and upstash...   # required
version: 0.1.0                         # required, semver
when_to_use: When operators want a read-only platform sweep    # optional
tags: [platform-ops, diagnostics]      # optional
author: usezombie                      # optional
model: claude-sonnet-4-6               # optional
---

You are Platform Ops Zombie. You diagnose problems in a small production
platform...
```

### Top-level keys

| Key | Required | Type | Constraint |
|---|---|---|---|
| `name` | yes | string | `/^[a-z0-9-]+$/`, 1-64 chars. Must equal `TRIGGER.md` `name:` |
| `description` | yes | string | ≤ 200 chars, single line |
| `version` | yes | string | semver (e.g. `0.1.0`) |
| `when_to_use` | no | string | pass-through; not validated, not used by runtime |
| `tags` | no | string[] | pass-through |
| `author` | no | string | pass-through |
| `model` | no | string | pass-through hint |

**Unknown top-level keys: silent pass-through.** Future skill-host vendors may add their own keys; we do not own the top-level namespace.

### Body

Plain markdown prose. Goes verbatim into the agent's system prompt. No structural parsing — the prose is the contract with the LLM.

---

## TRIGGER.md schema

```yaml
---
name: platform-ops-zombie

x-usezombie:
  trigger:
    types: [chat]
  tools:
    - http_request
    - memory_store
    - memory_recall
    - cron_add
    - cron_list
    - cron_remove
  credentials:
    - fly
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

# Optional operator-facing prose explaining credential shapes,
# budget reasoning, firewall behavior. Not consumed by runtime.
```

### Top-level keys

| Key | Required | Type | Constraint |
|---|---|---|---|
| `name` | yes | string | Same constraints as SKILL.md `name:`; must match |
| `x-usezombie` | yes | object | Runtime config block |

**Forbidden at top level** (hard error, `legacy_top_level_runtime`): `tools`, `credentials`, `network`, `budget`, `trigger`. These belong under `x-usezombie:`.

**Other top-level keys** (e.g. `x-amp:`, unknown vendor extensions): silent pass-through.

### `x-usezombie:` block — required subkeys

**Validation: rigid.** Unknown subkeys are a hard error (`unknown_runtime_key`). Typos must fail loud.

#### `trigger`

| Subkey | Required | Type | Constraint |
|---|---|---|---|
| `types` | yes | string[] | Non-empty. Each: `chat` \| `webhook` \| `cron` \| `api` \| `chain` |

Behavioral schedules (e.g. "every 30 min during incident windows") live in SKILL.md prose. The agent owns scheduling via the `cron_add` tool.

#### `tools`

`string[]`, non-empty. Each entry is a tool name from the platform tool registry. Order is not significant. The runtime cross-checks every name against the registry at install; unknown tool name → install rejected.

#### `credentials`

`string[]`. Each entry is a credential name that must exist in the operator's vault under the same name with the M45 structured `type` discriminator. Empty list is allowed (zombie does no authenticated egress).

#### `network`

| Subkey | Required | Type | Constraint |
|---|---|---|---|
| `allow` | yes | string[] | Hostname allow-list; empty = no outbound HTTPS allowed |

The sandbox firewall is a kernel-level egress rule. Hostnames not on this list are rejected at packet time.

#### `budget`

| Subkey | Required | Type | Constraint |
|---|---|---|---|
| `daily_dollars` | yes | number | `> 0`. UTC-day blast-radius cap |
| `monthly_dollars` | yes | number | `> 0`. Calendar-month spend envelope |

Caps do not compose (`daily * 30 ≠ monthly`). First trip blocks further runs. Independent guards by design.

---

## Cross-file invariant

`SKILL.md.name == TRIGGER.md.name`. Enforced at the install HTTP handler (`POST /v1/.../zombies`). Mismatch → `name_mismatch` error pointing at both values.

The directory basename is **not** the canonical name — it's a fallback hint for human-readable CLI output if the server response omits the name.

---

## Validation policy summary

| Region | Policy | Behavior on unknown key |
|---|---|---|
| SKILL.md top-level | permissive | silent pass-through |
| SKILL.md body | unparsed | n/a |
| TRIGGER.md top-level | mostly permissive | unknown: pass-through; legacy runtime keys (`tools`, etc.): hard error |
| TRIGGER.md `x-usezombie:` block | rigid | hard error (`unknown_runtime_key`) |
| TRIGGER.md other `x-*:` blocks | parsed but ignored | pass-through (other vendors) |
| TRIGGER.md body | unparsed | n/a (operator commentary) |

---

## Error contract

| Code | Source | Trigger |
|---|---|---|
| `missing_required_field` | parser | required field absent in SKILL.md or TRIGGER.md frontmatter |
| `usezombie_block_required` | parser | `x-usezombie:` missing in TRIGGER.md |
| `legacy_top_level_runtime` | parser | runtime key (`tools`/`credentials`/`network`/`budget`/`trigger`) at top level of TRIGGER.md |
| `unknown_runtime_key` | parser | unknown subkey under `x-usezombie:` |
| `duplicate_key` | YAML→JSON converter | same key declared twice at any level |
| `name_mismatch` | install handler | SKILL.md `name:` ≠ TRIGGER.md `name:` |
| `secret_not_found` | runtime, first event | credential listed but missing in vault |

---

## Why YAML and not pure prose

The frontmatter exists for the parts of the system that are **not** the LLM. The body of SKILL.md is the prose the LLM reads — that's where the natural language lives. The YAML carries the small bag of machine-actionable facts the runtime needs **before** it ever invokes the LLM:

1. **Sandbox boundaries.** `network.allow:` is a kernel-level egress rule. The firewall cannot ask the LLM "is this domain ok?" at packet time. If an LLM extracted the allow-list from prose, you'd get non-deterministic security boundaries — a real incident waiting to happen.

2. **Credential scope.** `credentials:` gates which secrets get injected. If the model decided per-run, a prompt-injection attack could expand scope by talking the model into "I need stripe too." Static declaration = static blast radius.

3. **Pre-LLM decisions.** Cron scheduling, budget caps, trigger registration all happen *before* any model call. The HTTP handler that registers a zombie cannot afford a 200ms + $0.001 LLM call to figure out the schedule.

4. **Auditability.** A reviewer scanning a TRIGGER.md sees in 5 seconds: this thing can spend $1/day, hit two domains, and use these two credentials. If those facts were in prose, you would need to re-run the LLM to audit a skill — and trust the audit run matched the runtime run.

5. **The natural-language alternative reinvents YAML internally.** If you accepted pure prose, you would build an extraction pass that produces a struct. Now you have two surfaces (prose + extracted struct), they drift, and authors cannot see what got extracted. Better to make the contract explicit.

The honest critique that informed the M46 trim: most YAML config in skill-style systems is bigger than it needs to be. `context.tool_window: auto` and declarative `cron:` schedule blocks belong in prose where the LLM can act on them. The non-negotiables are `credentials`, `network.allow`, `budget` (security/cost) and `name`/`version` (identity). Everything else is debatable.

---

## See also

- `docs/ARCHITECHTURE.md` §8.1 (Authoring), §10 (capabilities)
- `samples/platform-ops/` — canonical shipped example
- `samples/fixtures/frontmatter/` — minimal/full/broken parser fixtures
- `src/zombie/config_parser.zig` — parser implementation
- `src/zombie/yaml_frontmatter.zig` — YAML→JSON converter
- `~/Projects/docs/concepts/skill-frontmatter` — end-user reference (mintlify)
