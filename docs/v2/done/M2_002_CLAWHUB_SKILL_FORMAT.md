# M2_002: ClaHub-Compatible Skill Format — SKILL.md + TRIGGER.md

**Prototype:** v0.6.0
**Milestone:** M2
**Workstream:** 002
**Date:** Apr 09, 2026
**Status:** DONE
**Priority:** P0 — Fixes P1 greptile finding (simpleYamlParse drops arrays), aligns with ClaHub ecosystem
**Batch:** B1
**Branch:** feat/m2-clawhub-skill-format
**Depends on:** M1_001 (zombie config, CLI, templates)

---

## Overview

**Goal (testable):** A zombie is a directory containing `SKILL.md` (ClaHub-compatible agent instructions) and `TRIGGER.md` (UseZombie deployment manifest). `zombiectl install lead-collector` creates this directory. `zombiectl up` reads both files and deploys. ClaHub skills work via `skill:` reference in TRIGGER.md. The custom `simpleYamlParse` is deleted — SKILL.md is sent raw, TRIGGER.md frontmatter uses the existing Zig YAML-frontmatter extractor + JSON for structured fields. Chaining is declared in TRIGGER.md.

**Problem:** M1_001 shipped a custom YAML parser (`simpleYamlParse`) that silently drops all array items (P1 greptile finding). The single-file `.md` format mixes agent instructions with platform config (trigger, budget, network). This prevents publishing skills to ClaHub and prevents using ClaHub skills in zombies. The parser is unfixable without a proper library — but we don't need a parser at all if we separate concerns correctly.

**Solution summary:** Split the single `lead-collector.md` into a directory with two files. `SKILL.md` follows the ClaHub standard (YAML frontmatter with name/description/tags + markdown body as agent instructions). `TRIGGER.md` carries platform config (trigger routing, chain, budget, network, credential references). The CLI sends `SKILL.md` raw to the server. `TRIGGER.md` frontmatter is parsed server-side by the existing Zig frontmatter extractor. Delete `simpleYamlParse`, `parseZombieMarkdown`, and `parseYamlValue` from the CLI. Add `skill:` field in TRIGGER.md for referencing ClaHub registry skills. Add `chain:` field for declaring downstream zombies.

---

## 1.0 Directory-Based Zombie Format

**Status:** PENDING

A zombie is a directory, not a single file. Two files:

```
lead-collector/
├── SKILL.md      ← ClaHub-compatible: agent instructions
└── TRIGGER.md    ← UseZombie-specific: deployment manifest
```

### SKILL.md (ClaHub standard)

```yaml
---
name: lead-collector
description: Qualifies inbound email leads with a warm, personalized reply
tags: [leads, email, agentmail, qualification]
author: usezombie
version: 0.1.0
---

You are Lead Collector, a friendly and professional lead qualification agent.

When you receive an inbound email via AgentMail:
1. Read the sender's message carefully.
2. Extract key information: sender name, company, what they need.
3. Reply with a warm, personalized greeting.
4. Ask one clarifying question to qualify the lead.
5. Sign off as "Lead Collector at [workspace name]".

Rules:
- Always be professional but warm. Never robotic.
- Keep replies under 200 words.
- Never make up information you don't have.
- If the email is spam or clearly not a lead, reply politely declining.
- Log every interaction to the activity stream.
```

### TRIGGER.md (UseZombie deployment manifest)

```yaml
---
name: lead-collector
trigger:
  type: webhook
  source: agentmail
  event: message.received
chain:
  - lead-enricher
credentials:
  - agentmail_api_key
budget:
  daily_dollars: 5.0
  monthly_dollars: 29.0
network:
  allow:
    - api.agentmail.to
---

## Trigger Logic

This zombie activates on inbound AgentMail webhooks (message.received events).

## Chain

When the agent scores a lead >= 7, the event is forwarded to `lead-enricher`.

## Security

- Bearer token required (auto-generated on `zombiectl up`)
- Network restricted to agentmail API only
- Budget hard-capped at $29/month
```

### Using a ClaHub skill (no local SKILL.md needed)

```yaml
# TRIGGER.md — references ClaHub skill instead of local SKILL.md
---
name: lead-enricher
skill: clawhub://queen/lead-hunter@1.0.1
trigger:
  type: chain
  source: lead-collector
budget:
  daily_dollars: 2.0
---
```

**Dimensions:**
- 1.1 PENDING
  - target: `zombiectl/templates/lead-collector/SKILL.md`
  - input: Template file exists with valid ClaHub frontmatter
  - expected: `name`, `description`, `tags` in frontmatter; markdown body contains agent instructions
  - test_type: unit (file exists, frontmatter parseable)
- 1.2 PENDING
  - target: `zombiectl/templates/lead-collector/TRIGGER.md`
  - input: Template file exists with valid trigger config
  - expected: `trigger`, `credentials`, `budget`, `network` fields present in frontmatter
  - test_type: unit (file exists, frontmatter parseable)
- 1.3 PENDING
  - target: `src/zombie/config.zig:extractFrontmatter`
  - input: TRIGGER.md content with YAML frontmatter
  - expected: Frontmatter extracted, parsed into JSON, feeds existing `parseZombieConfig`
  - test_type: unit

---

## 2.0 CLI Changes

**Status:** PENDING

### `zombiectl install <template>`

Creates a directory instead of a single file:

```
$ zombiectl install lead-collector
  Created lead-collector/
    SKILL.md   — agent instructions (edit this)
    TRIGGER.md — deployment config
```

### `zombiectl up [directory]`

Reads both files from directory, sends to API:

```js
// Before (buggy):
const config = parseZombieMarkdown(configContent);  // simpleYamlParse — drops arrays
POST { source_markdown, config_json: config }

// After (no parser):
const skillMd = readFileSync("lead-collector/SKILL.md", "utf-8");
const triggerMd = readFileSync("lead-collector/TRIGGER.md", "utf-8");
POST { source_markdown: skillMd, trigger_markdown: triggerMd }
```

### Deleted code

- `simpleYamlParse()` — deleted (root cause of P1 greptile finding)
- `parseZombieMarkdown()` — deleted
- `parseYamlValue()` — deleted
- `currentObj`, `inArray`, `arrayKey` variables — deleted

**Dimensions:**
- 2.1 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandInstall`
  - input: `zombiectl install lead-collector`
  - expected: Directory `lead-collector/` created with `SKILL.md` and `TRIGGER.md`
  - test_type: unit
- 2.2 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandUp`
  - input: `zombiectl up` with valid `lead-collector/` directory in cwd
  - expected: Both files read, sent to API as raw strings, no client-side parsing
  - test_type: unit (mocked API)
- 2.3 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandUp`
  - input: `zombiectl up` with no zombie directory in cwd
  - expected: Error: "No zombie directory found. Run: zombiectl install <template>"
  - test_type: unit
- 2.4 PENDING
  - target: `zombiectl/src/commands/zombie.js`
  - input: Code review
  - expected: `simpleYamlParse`, `parseZombieMarkdown`, `parseYamlValue` deleted. No custom YAML parsing.
  - test_type: code review

---

## 3.0 Server-Side Changes

**Status:** PENDING

The server receives `source_markdown` (SKILL.md raw) and `trigger_markdown` (TRIGGER.md raw). Server extracts TRIGGER.md frontmatter using existing `extractZombieInstructions`-style extraction, converts frontmatter fields to JSON, feeds to existing `parseZombieConfig`. SKILL.md body (agent instructions) extracted via existing `extractZombieInstructions`. Both stored in `core.zombies` columns.

The `config_json` column is now **server-computed** from `trigger_markdown` frontmatter — not client-supplied. This is more secure (untrusted client can't inject arbitrary config).

**Dimensions:**
- 3.1 PENDING
  - target: `src/zombie/config.zig:parseZombieFromTriggerMarkdown`
  - input: Valid TRIGGER.md content
  - expected: Frontmatter extracted, parsed into ZombieConfig struct
  - test_type: unit
- 3.2 PENDING
  - target: `src/zombie/config.zig:parseZombieFromTriggerMarkdown`
  - input: TRIGGER.md with `skill: clawhub://queen/lead-hunter@1.0.1`
  - expected: `skill` field captured for registry resolution (resolution itself is M3)
  - test_type: unit
- 3.3 PENDING
  - target: `src/zombie/config.zig:parseZombieFromTriggerMarkdown`
  - input: TRIGGER.md with `chain: [lead-enricher]`
  - expected: `chain` field parsed as string array
  - test_type: unit
- 3.4 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleCreateZombie` (M2_001 §5.1)
  - input: `POST /v1/zombies/ with source_markdown + trigger_markdown`
  - expected: Server computes config_json from trigger_markdown, stores both
  - test_type: integration (DB)

---

## 4.0 Chaining (Declarative)

**Status:** PENDING

A zombie's TRIGGER.md declares downstream zombies in `chain:`. When the agent emits a chain event in its response, the platform routes to the next zombie's Redis stream. Chain routing is platform-level — the agent just says what to forward, not where.

```
lead-collector → lead-enricher → crm-updater
  (webhook)        (chain)          (chain)
```

Each zombie is independent. Chain is one-way (no cycles). Chain resolution happens at deploy time (`zombiectl up` validates all referenced zombies exist in the workspace).

**Dimensions:**
- 4.1 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandUp`
  - input: TRIGGER.md with `chain: [lead-enricher]`, lead-enricher exists in workspace
  - expected: Deploy succeeds, chain metadata stored
  - test_type: unit (mocked API)
- 4.2 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandUp`
  - input: TRIGGER.md with `chain: [nonexistent-zombie]`
  - expected: Warning: "Chain target 'nonexistent-zombie' not found in workspace. Deploy anyway? (it can be created later)"
  - test_type: unit
- 4.3 PENDING
  - target: Chain routing (server-side, M2_001 §1.0 dependency)
  - input: Agent response contains `{ "chain": "lead-enricher", "data": {...} }`
  - expected: Event enqueued to `zombie:{lead-enricher-id}:events` stream
  - test_type: integration (Redis) — deferred to M2_001 worker integration

---

## 5.0 Credential References

**Status:** PENDING

Credentials in TRIGGER.md are **names**, not vault paths. The user creates them with `zombiectl credential add <name>`. The platform resolves name → vault path at deploy/runtime.

```yaml
# TRIGGER.md
credentials:
  - agentmail_api_key     # ← just a name
```

```bash
# User creates the credential
$ zombiectl credential add agentmail_api_key --value=sk-xxx
  Stored in vault.
```

No `op://` paths in TRIGGER.md. No `requires.env` (that's ClaHub's pattern for env vars — our credentials are vault-backed, not env-var-backed).

**Dimensions:**
- 5.1 PENDING
  - target: `src/zombie/config.zig:parseZombieConfig`
  - input: `credentials: ["agentmail_api_key"]` (name, not op:// path)
  - expected: Parsed as string array, no op:// validation
  - test_type: unit
- 5.2 PENDING
  - target: Credential resolution (M2_001 §2.2 dependency)
  - input: Zombie starts with credential name "agentmail_api_key"
  - expected: Platform resolves name to vault path, injects into sandbox
  - test_type: integration (deferred to M2_001)

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 File Format

| File | Required | Format | Purpose |
|------|----------|--------|---------|
| `SKILL.md` | Yes (unless `skill:` in TRIGGER.md) | ClaHub standard: YAML frontmatter + markdown | Agent instructions |
| `TRIGGER.md` | Yes | YAML frontmatter + markdown | Deployment manifest |

### 6.2 SKILL.md Frontmatter Fields

| Field | Type | Required | ClaHub standard | Example |
|-------|------|----------|----------------|---------|
| `name` | string | yes | yes | `lead-collector` |
| `description` | string | yes | yes | `Qualifies inbound email leads` |
| `tags` | string[] | no | yes | `[leads, email]` |
| `author` | string | no | yes | `usezombie` |
| `version` | string | no | yes | `0.1.0` |

### 6.3 TRIGGER.md Frontmatter Fields

| Field | Type | Required | Example |
|-------|------|----------|---------|
| `name` | string | yes | `lead-collector` |
| `skill` | string | no | `clawhub://queen/lead-hunter@1.0.1` |
| `trigger.type` | enum | yes | `webhook` \| `chain` \| `cron` \| `api` |
| `trigger.source` | string | conditional | `agentmail` (webhook/chain: required) |
| `trigger.event` | string | no | `message.received` |
| `trigger.schedule` | string | conditional | `0 9 * * *` (cron: required) |
| `chain` | string[] | no | `[lead-enricher]` |
| `credentials` | string[] | no | `[agentmail_api_key]` |
| `budget.daily_dollars` | float | yes | `5.0` |
| `budget.monthly_dollars` | float | no | `29.0` |
| `network.allow` | string[] | no | `[api.agentmail.to]` |

### 6.4 API Contract Change

```
# Before (M1_001):
POST /v1/zombies/
{ workspace_id, name, source_markdown, config_json }
                                       ^^^^^^^^^^^^^ client-parsed (untrusted)

# After (M2_002):
POST /v1/zombies/
{ workspace_id, source_markdown, trigger_markdown }
                                 ^^^^^^^^^^^^^^^^^ server parses (trusted)
```

---

## 7.0 Implementation Constraints

| Constraint | How to verify |
|-----------|---------------|
| No custom YAML/TOML parser in JS | `grep -r 'simpleYaml\|parseZombieMarkdown\|parseYamlValue' zombiectl/` returns empty |
| SKILL.md publishable to ClaHub as-is | Frontmatter matches ClaHub standard (name, description, tags, author, version) |
| Server computes config_json, not client | API handler extracts config from trigger_markdown, ignores any client config_json |
| Credential names only, no vault paths | `grep 'op://' zombiectl/templates/` returns empty |
| Every new file < 500 lines | `wc -l` check |
| Cross-compiles | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## 8.0 Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Create `zombiectl/templates/lead-collector/SKILL.md` + `TRIGGER.md` | Files exist, frontmatter valid |
| 2 | Delete old `zombiectl/templates/lead-collector.md` | File gone |
| 3 | Update `commandInstall` to create directory with both files | Unit test: directory created |
| 4 | Update `commandUp` to read directory, send both files raw | Unit test: API called with source_markdown + trigger_markdown |
| 5 | Delete `simpleYamlParse`, `parseZombieMarkdown`, `parseYamlValue` | grep returns empty |
| 6 | Update `findZombieConfig` to find zombie directories | Unit test |
| 7 | Add `parseZombieFromTriggerMarkdown` to config.zig (server-side) | Unit test |
| 8 | Update zombie.unit.test.js for new format | `bun test` passes |
| 9 | Cross-compile check | Both targets pass |
| 10 | Full test suite | `make test && make lint` |

---

## 9.0 Acceptance Criteria

- [ ] `zombiectl install lead-collector` creates `lead-collector/SKILL.md` + `lead-collector/TRIGGER.md`
- [ ] `zombiectl up` reads both files, sends raw to API (no client-side YAML parsing)
- [ ] `simpleYamlParse` deleted from codebase
- [ ] SKILL.md follows ClaHub standard (publishable as-is)
- [ ] TRIGGER.md `skill:` field accepted (registry resolution deferred to M3)
- [ ] TRIGGER.md `chain:` field accepted (routing deferred to M2_001)
- [ ] Credentials are names, not vault paths
- [ ] `bun test zombiectl/test/zombie.unit.test.js` passes
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compile succeeds

---

## 10.0 Webhook Auth — Slack-Style URL Secret (from M1_001 security review)

**Status:** PENDING

M1_001 shipped Bearer token auth for webhooks. Greptile flagged that templates without
a `token` field leave the endpoint unauthenticated. The M1_001 fix rejects requests when
no token is configured (fail-closed). But Bearer headers are awkward — most webhook sources
(agentmail, GitHub, Slack) only let you configure a URL, not custom headers.

**M2_002 change:** Switch to Slack-style URL-embedded secret:

```
# Before (M1_001):
POST /v1/webhooks/{zombie_id}  + Authorization: Bearer {token}

# After (M2_002):
POST /v1/webhooks/{zombie_id}/{webhook_secret}
```

The `webhook_secret` is generated at `zombiectl up` time (crypto-random, 32 bytes, base64url).
Stored in `core.zombies.webhook_secret` (new column). Returned to the user at deploy time:

```
$ zombiectl up
  lead-collector is live.
  Webhook URL: https://api.usezombie.com/v1/webhooks/019abc.../kR7x2mN...
  Configure this URL in agentmail's webhook settings.
```

No Authorization header needed. The URL itself is the auth (same as Slack incoming webhooks).
Bearer auth remains as a fallback for sources that support custom headers.

**Dimensions:**
- 10.1 PENDING — Router accepts `POST /v1/webhooks/{zombie_id}/{secret}`
- 10.2 PENDING — `zombiectl up` generates and stores webhook_secret
- 10.3 PENDING — Existing Bearer auth still works as fallback
- 10.4 PENDING — Schema migration adds `webhook_secret` column to `core.zombies`

---

## 11.0 Out of Scope

- ClaHub registry resolution (M3 — `skill:` field is stored but not fetched)
- Chain event routing at runtime (M2_001 — worker integration)
- Credential vault resolution at runtime (M2_001 — executor integration)
- Multi-zombie `zombiectl up` (deploys all directories in cwd) — future
- Pipeline visualization (`zombiectl status` showing chain graph) — future
- TRIGGER.md validation against ClaHub skill's declared requirements — future
