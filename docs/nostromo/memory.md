# Zombie Memory — How Zombies Remember Across Runs

**Published at:** docs.usezombie.com/memory
**Applies to:** All zombie types (Lead Collector, Hiring Agent, Ops, Customer Support, and any custom zombie)
**Default state:** Memory is *enabled by default* for zombies created on UseZombie ≥ the M14 release. Pre-M14 zombies opt in with `zombiectl zombie memory enable`.

---

## What memory is, in one sentence

Memory is what a zombie *learned* from prior runs that lets it behave like a teammate who's been here before, instead of a goldfish that showed up this morning.

---

## Log vs memory (the distinction that makes or breaks the feature)

**Log** = what happened. Tool calls, events, timestamps. Raw history. UseZombie already has this via SSE streams, event tables, and `zombiectl runs replay`.

**Memory** = what the zombie *learned* from what happened. Curated, distilled, judged. The agent decides "this fact is worth keeping, that one isn't."

| | Log | Memory |
|---|---|---|
| Answers | "What did the zombie do yesterday?" | "What does the zombie now know that it didn't know before?" |
| Granularity | Every tool call, every event | One entry per durable fact |
| Who writes it | The system | The agent (via `memory_store`) |
| Example | `POST /v1/execute {slack.com/...} → 200` | `"Bob at Acme prefers technical responses, on Pro plan since Jan"` |

If a zombie's memory looks like a log, the skill template is wrong. Fix the template, not the storage.

---

## The 4 memory categories

```
┌──────────────────┬───────────────┬────────────────────────────┬──────────────────────────────┐
│     Category     │     Scope     │         Durability         │             Use              │
├──────────────────┼───────────────┼────────────────────────────┼──────────────────────────────┤
│ core             │ zombie        │ permanent until forgotten  │ learned entities, preferences│
├──────────────────┼───────────────┼────────────────────────────┼──────────────────────────────┤
│ daily            │ zombie        │ 72h auto-prune             │ follow-ups, open tickets     │
├──────────────────┼───────────────┼────────────────────────────┼──────────────────────────────┤
│ conversation     │ run           │ dies with workspace        │ within-run working notes     │
├──────────────────┼───────────────┼────────────────────────────┼──────────────────────────────┤
│ workspace        │ workspace     │ durable (separate system)  │ shared facts across zombies  │
└──────────────────┴───────────────┴────────────────────────────┴──────────────────────────────┘
```

- `core` and `daily` land in the dedicated memory database. **These are what survive workspace destruction.**
- `conversation` stays in workspace SQLite — fast, local, discarded.
- `workspace` is a separate system (Warden/Echo) — out of scope for this doc.

---

## Primitives (what the zombie actually calls)

The zombie uses four tool calls. The harness wires them to the right backend automatically.

| Tool | Shape | Used for |
|---|---|---|
| `memory_store(key, category, content, tags?)` | upsert | write a distilled fact |
| `memory_recall(key)` | returns one entry or empty | exact lookup by key |
| `memory_list(filter)` | returns matching entries | filter by category/tags/recency |
| `memory_forget(key)` | delete | correct or remove a fact |

No vector search, no embeddings. Pre-filter with tags/categories/recency, then let the LLM reason over the candidate set. Modern context windows + LLM judgment beat cosine similarity for typical zombie memory sizes.

---

## Memory entry format (the human-readable view)

Memory is stored in a dedicated Postgres database but exported as markdown for human review and editing. Each entry round-trips through this format:

```markdown
---
key: lead_acme_corp
category: core
zombie_id: zom_leadgen_01
tags: [lead, enterprise, warm]
created: 2026-04-10T14:22:11Z
updated: 2026-04-12T09:01:03Z
---

Acme Corp (acme.com) — CTO Jane Park (jane@acme.com).
Intent: enterprise tier pricing inquiry, team of 50.
Preferences: email over LinkedIn DMs.
Last interaction: Apr 8, replied "circle back after Q2 planning."
Budget: confirmed >$50k.
Stage: qualified-warm. Next action: follow up Jun 15.
```

Frontmatter is structured metadata. The body is free-form distilled prose the agent wrote.

---

## Runtime flow (generic — same for every archetype)

```
[Run starts]
    → memory_recall(entity_key)       — pull context for this specific thing
    → memory_list(tag="workspace")     — pull workspace preferences
    → memory_list(category="daily")    — check scheduled actions
    → [agent reasons, takes action]
    → memory_store(entity_key, ...)    — update what was learned
    → memory_store(followup, daily)    — schedule any needed follow-up
[Run ends, workspace destroyed]
[core and daily memory persist in the memory DB]
[conversation memory dies with the workspace]
```

The two memory touchpoints are at the **start** (recall, before deciding) and the **end** (store, after deciding). Anything in the middle writing to memory is probably a bug — that's scratch that belongs in `conversation`.

---

## Setup — memory-specific steps for any zombie

Memory is default-on for new zombies. Explicit configuration looks like:

```bash
# Enable memory (no-op if already enabled)
zombiectl zombie memory enable --zombie {zombie_id} \
  --categories core,daily \
  --daily-retention-hours 72

# Verify
zombiectl zombie memory status --zombie {zombie_id}
# Backend: postgres (memory.memory_entries)
# Categories: core, daily
# Entries: N
# Retention (daily): 72h
```

Seeding memory before first run is optional but dramatically improves day-1 behavior for archetypes with known priors (roles, SLOs, customer base). Use `zombiectl memory import` with a local folder of markdown entries.

---

## Edit-then-replay — correcting what the zombie learned

Memory lives in Postgres, but humans interact with it as markdown on their laptop.

```bash
# Export to the operator's machine (never touches worker filesystems)
zombiectl memory export --zombie {zombie_id} --out ./zombie-memory/

# Review / fix / delete entries
vim ./zombie-memory/core/some_entity.md
rm ./zombie-memory/core/stale_entry.md

# Import — next run sees the corrections
zombiectl memory import --zombie {zombie_id} --from ./zombie-memory/

# Optional: treat the exported folder as a git repo for audit history
cd ./zombie-memory && git init && git add . && git commit -m "Correct Acme budget"
```

This composes with two primitives UseZombie already shipped:

| Primitive | When | What it does |
|---|---|---|
| Session replay (`zombiectl runs replay`) | after a run | forensic event playback |
| Interrupt and steer (M21_001) | during a run | live correction |
| Memory export/import (M14_001) | between runs | surgical brain-edit |

---

## Metrics — is memory actually helping?

| Metric | Meaning | Healthy zone |
|---|---|---|
| Recall-hit rate | fraction of runs where `memory_recall` returned context | rises from 0% → 30-60% at steady state |
| Duplicate-action rate | zombie took an action it already took in a prior run | < 2-5% depending on archetype |
| Wrong-context-asserted | zombie confidently stated something false from memory | MUST stay near zero — P0 trust metric |
| Memory size per zombie | total entries × avg size | monitor for runaway growth |
| Daily-followup compliance | daily reminders acted on in-window | > 80% at steady state |

If recall-hit rate stays near zero after a week of operation, the skill template isn't storing the right things or isn't recalling at the right moment. That's a policy problem (M14_002), not a storage problem (M14_001).

---

## Archetype guides — what each zombie remembers

The primitives are the same. The **key conventions**, **what-to-store discipline**, and **seed data** differ per archetype. Below is the condensed guide per type — an LLM-driven skill template fills in the rest.

### Lead Collector

| | |
|---|---|
| Key convention | `lead_{company_slug}` — one entry per company, not per email |
| Core stores | learned entity: contact, preferences, stage, budget, next action |
| Core seeds | workspace CRM preference, scoring thresholds, approval-gate criteria |
| Daily stores | `followup_{company}_{date}` scheduled follow-ups |
| Forgets | raw email bodies (CRM has them), tool-call noise |
| Recall-critical moment | start of each email ingestion — before deciding outreach |
| Win metric | duplicate-contact rate drops, follow-ups land on correct dates |
| Edit-then-replay hotspot | correcting budget / stage / preferences mismatches |

Example entry:
```
---
key: lead_acme_corp
category: core
tags: [lead, enterprise, warm]
---
Acme Corp — CTO Jane. Prefers email. Budget $75k. Follow up Jun 15.
```

### Hiring Agent

| | |
|---|---|
| Key convention | `candidate_{name_slug}_{role_slug}`, `role_{slug}`, `interviewer_{name}_preferences` |
| Core stores | candidate profile + loop history, role rubrics, interviewer styles |
| Core seeds | role definitions, interviewer preferences — seed before first run |
| Daily stores | `followup_{candidate}_{date}` feedback collection reminders |
| Forgets | raw resume content (ATS has it), detailed rejection reasons (compliance) |
| Recall-critical moment | every `#hiring` message — carry stage forward, don't re-ask HR |
| Win metric | re-question rate < 5%, stage recall-hit > 90% |
| Edit-then-replay hotspot | correcting candidate mischaracterizations, updating role rubrics |

Example entry:
```
---
key: candidate_jane_smith_sr_eng
category: core
tags: [candidate, role_senior_engineer, stage_technical_screen]
---
Jane Smith — Senior Eng candidate. Mar 28 inbound.
Stage: tech screen Apr 15 with Priya (systems design focus).
Phone screen Apr 2 with Raj: pass, notes in Lever #445.
```

### Ops Zombie

| | |
|---|---|
| Key convention | `incident_sig_{service}_{symptom}_{root_cause_hash}`, `noise_pattern_{description}`, `slo_{service}` |
| Core stores | incident signatures with root causes + fixes, noise patterns, SLO definitions |
| Core seeds | SLOs, known noise patterns — seed before live alerts |
| Daily stores | `incident_active_{timestamp}` during ongoing incidents (dedup windows) |
| Forgets | raw log lines (Loki has them), individual alert events (only patterns) |
| Recall-critical moment | every alert — before classifying, check if seen before |
| Win metric | incident-recognition rate, MTTR delta, false-positive suppression near zero |
| Edit-then-replay hotspot | postmortem-driven signature updates, retiring noise patterns |

Example entry:
```
---
key: incident_sig_api_server_oom_pg_pool
category: core
tags: [incident, api-server, oom, postgres, resolved]
---
Signature: api-server OOM from PG pool exhaustion.
Seen Mar 14, Apr 2. Root: connection leak in PDF worker (src/workers/pdf.py:142).
Fix: use context manager for pool.acquire. Runbook: [link].
```

### Customer Support

| | |
|---|---|
| Key convention | `customer_{name_slug}_{company_slug}`, `account_{company_slug}`, `customer_flags_{...}`, `ticket_open_{id}` |
| Core stores | customer profile, account context, feature flags, prior issues |
| Core seeds | optional bulk import from CRM on enable |
| Daily stores | open tickets (with auto-prune once resolved) |
| Forgets | full ticket transcripts (ticketing system has them), unneeded PII |
| Recall-critical moment | every customer message — full context before drafting |
| Win metric | re-question rate < 3%, wrong-context-asserted near zero |
| Edit-then-replay hotspot | plan upgrades, contact changes, preference shifts |

Example entry:
```
---
key: customer_bob_chen_acme
category: core
tags: [customer, plan_pro, api_user]
---
Bob Chen, Acme. Pro since Jan 15. API user (not dashboard), webhooks → Slack.
Prefers technical responses. Prior: Feb 3 signature issue (NTP), Mar 22 rate limit (upgraded).
Flag acme_webhook_retry_v2 enabled.
```

### Custom zombies

Any custom zombie follows the same pattern:

1. **Pick a key convention** — stable, deterministic from the input. `{entity_type}_{identifier}`.
2. **Define what's worth a `core` entry** — learned facts about durable entities, not transcripts.
3. **Define what's `daily`** — anything time-bounded, with a natural expiry.
4. **Seed any priors** — workspace preferences, role definitions, known patterns.
5. **Wire `memory_recall` to the start of the flow** — before deciding, not after.
6. **Wire `memory_store` to the end of the flow** — after deciding what was learned.

The skill template for the archetype encodes these choices. M14_002 is the workstream that ships opinionated templates for the common archetypes.

---

## Isolation and security

**Row-level zombie_id scoping.** Every memory operation includes `WHERE zombie_id = $current`. Zombie A cannot read Zombie B's memory via the memory tool.

**Process boundary.** Memory lives in Postgres, not on a shared filesystem. If a zombie has shell or file tools, those cannot reach memory — wrong protocol, wrong credentials. See `docs/greptile-learnings/RULES.md`:

> Cross-tenant data that a sandboxed agent must not read cannot share a filesystem with the agent. Use a process boundary.

**PII discipline.** The Customer Support archetype is the highest-sensitivity case. Configure redaction in the skill template; run `zombiectl memory scrub` to retroactively remove matched patterns.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Zombie "forgets" between runs | Memory not enabled for this zombie | `zombiectl zombie memory enable` |
| Recall returns nothing on known entity | Key convention inconsistent | Audit skill template; stable key derivation |
| Memory grows forever | No pruning on `core` (by design) | Use `zombiectl memory forget` or export→prune→import |
| Responses feel generic despite memory | Template not recalling before drafting | Audit: `memory_recall` MUST happen before the reasoning step |
| `UZ-MEM-001` on store | Memory backend unreachable | Check `zombie memory status`; verify DB connection |
| Sibling zombie's memory visible | Must never happen — P0 | Stop shipping. See M14 integration test `zombie_isolation`. |
| Wrong-context asserted to user | Stale memory, underlying system changed | Export → correct → import; add a sync job from system-of-record |
| Daily entry expired before resolved | 72h TTL hit, issue is long-running | Promote to `core` with `tag: open_extended` |

---

## External agent variant (Path B)

Any external pipeline (LangGraph, CrewAI, custom Python) can use UseZombie memory
by calling the memory API directly:

```bash
curl -X POST https://api.usezombie.com/v1/memory/recall \
  -H "Authorization: Bearer <YOUR_ZMB_KEY>" \
  -d '{"zombie_id": "zom_xyz", "key": "lead_acme_corp"}'

curl -X POST https://api.usezombie.com/v1/memory/store \
  -H "Authorization: Bearer <YOUR_ZMB_KEY>" \
  -d '{
    "zombie_id": "zom_xyz",
    "key": "lead_acme_corp",
    "category": "core",
    "tags": ["lead", "warm"],
    "content": "Acme Corp — CTO Jane. Budget $75k. Follow up Jun 15."
  }'
```

Same isolation model (row-level `zombie_id` scope), same edit-then-replay via
export/import. Useful for mixed workflows: scoring in CrewAI, notifications via
UseZombie, memory shared between them.
