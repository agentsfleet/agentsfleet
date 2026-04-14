# Zombie Memory — How Zombies Remember Across Runs

**Published at:** docs.usezombie.com/memory
**Applies to:** All zombie types (Lead Collector, Hiring Agent, Ops, Customer Support, and any custom zombie)
**Default state:** Memory is *enabled by default* for zombies created on UseZombie ≥ the M14 release. Pre-M14 zombies opt in with `zombiectl zombie memory enable`.

---

## What memory is, in one sentence

Memory is what a zombie *learned* from prior runs that lets it behave like a teammate who's been here before, instead of a goldfish that showed up this morning.

---

## The problem memory solves (the flat tyre nobody calls a flat tyre)

A flat tyre is usually acute and visible. Memoryless zombies don't feel flat-tyred because the pain shows up as three slow leaks:

1. **Death by re-steering.** You tell the zombie "don't pitch enterprise to sub-50" on Monday. You tell it again Wednesday. And Friday. Each correction feels minor — cumulatively it's a huge tax. No single moment looks like failure.
2. **Confident wrongness with delayed blowback.** "Following up on our last call…" when no call happened. Damage shows up days later as a churned lead, a ghosted candidate, a page ignored. The cause looks like "AI wasn't good enough" — it's actually "AI had no memory of what actually happened."
3. **Silent redundancy.** Re-researching Acme. Re-classifying the same flap at 3am. Re-asking Sarah questions she already answered. Small per-instance, large in aggregate, invisible to an operator not watching closely.

**Reframed:** the zombie feels *almost* useful — enough to keep, not enough to trust. The operator becomes its permanent supervisor. Memory is what converts "needs re-briefing every run" into "remembers the briefing." That's the flat tyre. It hides because it's the difference between "no AI" and "underwhelming AI," not "no AI" and "no car."

By archetype, the hidden pain concentrates differently:

| Archetype | What memory actually solves | Highest-pain failure mode without it |
|---|---|---|
| Lead Collector | Keeps a journal on people + companies + your steering | Pitching the same lead twice; saying "per our last call" to a first-time contact |
| Ops Zombie | Keeps an incident library + noise fingerprints + runbooks | Paging humans at 3am for a flap it has seen 47 times |
| Hiring Agent | Keeps a candidate file + role rubric + phrasing rules | "Continue from your onsite" to someone who never onsited — legal/brand risk |
| Customer Support | Keeps customer profile + account context + prior issues | Asking for info already given; wrong-plan assertions |

The **observability layer on top of memory** (see Metrics section) exists for a second-order flat tyre: once memory ships, you can't tell if it's working. Recall could be silently missing. Steering could be stored but not applied. Without metrics, memory is a black box.

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
| Interrupt and steer (M23_001) | during a run | live correction |
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

## End-to-end flow (what every archetype shares)

Every archetype — Lead Collector, Ops, Hiring, Support, custom — follows the same generic sequence. Only the keys and skill-template choices differ.

```
[external trigger fires — email / webhook / Slack event / cron]
    → run starts
        → memory_recall × N (template-prescribed lookups from the trigger payload)
            → emit memory.recall.hit | memory.recall.miss per lookup
        → agent reasons using recalled context + incoming trigger
        → agent takes action (tool calls logged to core.activity_events)
            → approval gate branch if action is high-impact (Slack/Discord DM or dashboard)
        → memory_store × M (distilled facts learned this run)
            → emit memory.store.ok | memory.store.full per write
    → run ends

[separately, async — operator at their leisure]
    → opens replay UI
        → reads core.activity_events (timeline of what happened)
        → reads memory.memory_entries (what the zombie now knows)
    → optional: chat to steer → memory_store("steering:...", ...)
    → optional: thumbs-down on a wrong draft → INSERT INTO memory_feedback

[separately, async — every 5 min]
    → memory_metrics aggregator reads events + memory_feedback + activity_events
    → writes rolled-up rates keyed (zombie_id, archetype)
    → dashboard panel + Grafana read the pre-computed rates
```

Two surfaces the operator looks at, and they're different tables:

| Surface | Table | Purpose |
|---|---|---|
| "What did the zombie do?" | `core.activity_events` | Append-only timeline of every tool call and event |
| "What does the zombie know?" | `memory.memory_entries` | Curated durable facts — editable, exportable |

`core.zombie_sessions` is **not** user-facing — it's a resume bookmark for crash recovery, explicitly not memory.

Steering happens two ways and they're complementary, not alternatives:

- **In-app replay chat** (M23_001) → coach the zombie live. Chat messages get stored as `steering:*` entries and persist for future runs.
- **Slack / Discord / dashboard approval** → gate specific high-impact actions (page on-call, send offer, auto-scale). Not a steering surface — a go/no-go surface.

---

## Archetype guides — what each zombie remembers

The primitives are the same. The **key conventions**, **what-to-store discipline**, and **seed data** differ per archetype. Below is the condensed guide per type, followed by an elaborated step-by-step walkthrough. An LLM-driven skill template fills in the rest.

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

#### Lead Collector — step-by-step walkthrough

**Real flow:** contact form on `usezombie.com/contact/sales` → email to `usezombie@agentmail.to` → zombie traps the email, researches, drafts a reply, notifies the operator. Operator replays in the UI and chats to steer.

1. **Trigger.** Prospect `jane@acme.com` fills the form. AgentMail webhook delivers to the zombie. One email = one run. No cron.
2. **Recall — before research.** Zombie extracts identifiers and does exact-key lookups:
   - `memory_recall("lead_acme_corp")` → hit: CTO Jane, team ~200, asked pricing 3 weeks ago, "sub-50 push declined."
   - `memory_recall("thread:<message_id>")` → miss. New thread.
   - `memory_list(category="core", tags=["steering"])` → `steering:sub_50_employees`, `steering:tone`.
   - Each recall emits `memory.recall.hit` / `memory.recall.miss`.
3. **Act — research only the gaps.** Because `lead_acme_corp` hit, zombie skips LinkedIn + company-size research. Only researches what's new (Jane's current role, recent news). Actions written to `core.activity_events`.
4. **Act — draft.** Reply conditioned on recalled context (picking up the pricing thread, skipping demo CTA per steering). Draft → `core.activity_events`.
5. **Notify operator.** Slack DM or dashboard notification: *"Lead Collector drafted a reply to jane@acme.com."*
6. **Store — what was learned.**
   - `lead_acme_corp` updated: new interaction date, stage = "re-engaged," Jane's updated role.
   - `thread:<message_id>` created: `{last_action: "drafted_pricing_reply", approved_by_human: false}`.
   - Possibly a daily `followup_acme_corp_2026-04-20` if a nudge is warranted.
   - Each store emits `memory.store.ok` / `memory.store.full`.
7. **Operator loop — chat steering.** Operator opens replay view. If tone is off, types: *"don't mention competitor X, she's allergic."* That chat triggers `memory_store("steering:acme_corp:no_competitor_mentions", ...)`. Persists for future Acme runs.
8. **Operator loop — thumbs-down.** Zombie asserted "per our last call" when no call happened. Operator clicks thumbs-down → row in `memory_feedback`. This is the only input to wrong-context-asserted rate — no heuristics.
9. **Aggregator (async).** Every 5 min rolls the last 7 days into the four rates. Dashboard reads pre-computed rates; never live-computes.

**What good looks like after a month:** recall-hit 40-60%, duplicate-action <2%, wrong-context near zero, memory size plateaus after pruning.

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

#### Hiring Agent — step-by-step walkthrough

**Real flow:** HR posts in `#hiring`. Zombie drafts response (interview questions, scheduling, feedback summary) to the thread. Offers / rejections require per-action approval.

1. **Trigger.** HR posts: *"Sarah from the April 2 phone screen wants to schedule the technical round."* Slack Events API → webhook. One message = one run.
2. **Recall — before drafting.**
   - `memory_recall("candidate_sarah_smith_sr_backend")` → hit: Apr 2 phone screen by Raj (pass), strengths = Rust + distributed systems, concerns = no on-call exp, stage = `phone_screen_done`.
   - `memory_recall("thread:<slack_ts>")` → hit: previous message context.
   - `memory_recall("role_senior_backend_2026_q2")` → hit: rubric, must-haves, hiring manager = Kishore.
   - `memory_recall("interviewer_priya_preferences")` → hit: systems design focus, 90-min slots, Tue/Thu IST.
   - `memory_list(tags=["steering", "rejections"])` → `steering:rejections` (legal-preferred phrasing).
3. **Act — draft with full context.** *"Scheduling Sarah with Priya — 90 min, Tue Apr 21 or Thu Apr 23 IST, systems design focus based on her phone screen strengths."* Without memory, HR would have had to re-type all of that.
4. **Post to thread.** Via Slack API. `message_posted` event to `core.activity_events`.
5. **Approval gate branches.** Scheduling messages post directly. Offer letters / rejections trigger firewall → DM to hiring manager with `[Approve] [Deny]`. Steering is applied BEFORE the approval DM, so the manager is approving already-corrected phrasing.
6. **Store — candidate file updated.**
   - `candidate_sarah_smith_sr_backend`: stage → `technical_round_scheduled`, next_action → "await Priya's feedback Apr 23."
   - `thread:<slack_ts>` updated.
   - Daily `followup_sarah_smith_2026-04-23` created to nudge Priya.
7. **Operator loop — chat steering.** Manager types: *"less systems design emphasis, she's more of an implementer."* → `steering:candidate_sarah_smith:rubric_override`.
8. **Operator loop — thumbs-down (P0 here).** Zombie drafted "continuing from your onsite feedback" but Sarah never onsited. Manager thumbs-down → `memory_feedback` row. **In hiring, wrong-context in a rejection or offer is legal/brand risk**, not just an embarrassing draft. This rate must stay near zero.
9. **Aggregator.** Per-archetype rollup flags `hiring_zombie` wrong-context trend. Above 0.5% → investigate (usually colliding candidate keys or stale memory).

**What good looks like:** re-question rate drops 30% → <5%, stage recall-hit >90%, wrong-context ~0, daily-followup compliance >80%.

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

#### Ops Zombie — step-by-step walkthrough

**Real flow:** Grafana alert webhook or 5-min Loki log poll fires. Zombie classifies and routes to Slack/Discord. Critical actions (paging on-call, auto-scale) go through approval gates. Noise suppression is the single biggest on-call-human time saver.

1. **Trigger.** Grafana fires `api-server error rate > 5%` webhook. Payload contains alert name, labels, current value, timestamp. One alert = one run.
2. **Recall — before classification.** Zombie derives a fingerprint from the alert (service + symptom) and recalls:
   - `memory_recall("incident_sig_api_server_pg_timeout")` → hit. Returns: signature seen 47 times over last 3 months, last 3 occurrences were flaps that auto-resolved in <4 min, root cause = pg connection pool exhaustion, runbook = `scale-pool-to-50`.
   - `memory_recall("noise_pattern_api_server_batch_3am")` → hit (current time is 03:12). Returns: "batch job causes 30s error spike at 03:00-03:05 nightly — suppress unless sustained >10 min."
   - `memory_list(category="core", tags=["slo", "api-server"])` → `slo_api_server` ("99.9% target, current month 99.94% — room to absorb minor flaps").
   - `memory_recall("oncall_roster_weekly")` → `{primary: "jane", secondary: "sam"}`.
   - Each recall emits hit / miss events with latency.
3. **Act — classify using the recalled context.** Armed with the fingerprint hit + noise pattern + time-of-day steering + SLO headroom, zombie classifies this alert as `noise` (not a real incident). Without memory it would have classified as `warning` and posted to Slack — a notification for something the operator has already told it to suppress at 3am.
4. **Act — suppress or post.** Because classification is `noise`, zombie does NOT post to Slack. Instead it updates the existing aggregate count in `incident_sig_api_server_pg_timeout` (occurrences + 1, last_seen = now). Writes a `suppressed_noise` event to `core.activity_events` so the operator can still see it happened in the replay view if they skim.
5. **Alternate branch — if classification had been `critical`.** Zombie drafts the Slack message. If the body contains `@on-call`, the firewall rule fires → DM to workspace owner with `[Approve] [Deny]`. On approve, Slack message posts with the page. For destructive actions (scale-up, restart-pod), additional approval gate applies even if classification is `critical` — memory of "already scaled 2 min ago" prevents approval loops.
6. **Store — signature update.** On run completion:
   - `incident_sig_api_server_pg_timeout` updated with new last_seen + occurrence count + auto-resolve duration if applicable.
   - If this had been a NEW signature (recall miss), zombie would create a new `incident_sig_{service}_{symptom}_{hash}` entry with `{first_seen, root_cause: "unknown, investigating", runbook_applied: null}` — to be filled in post-mortem.
   - Each store emits `memory.store.ok` / `memory.store.full`.
7. **Operator loop — post-mortem steering.** After a real incident resolves, SRE opens the signature entry in the memory view and edits it: adds the root cause, runbook link, and a disambiguation note ("this is NOT the same as the April 2 OOM, separate signature"). That edit lands as a `memory_store` call and persists. Next alert with matching fingerprint gets the corrected context. This is where the zombie compounds value over postmortems the way a senior SRE does.
8. **Operator loop — thumbs-down.** Zombie drafted "this looks like the April 2 OOM incident" but it's actually the pg pool flap. SRE thumbs-down → `memory_feedback` row → wrong-context-asserted rate ticks up. High rate here means signatures are overlapping too much; fix is usually more specific key construction in the skill template (include a root_cause_hash component).
9. **Aggregator (async).** Same 5-min cron. Per-archetype rollup panel on Grafana surfaces `ops_zombie` recall-hit trending up over 30 days — proves the zombie is learning your infra's personality and earning its keep by replacing paged humans with signatures + auto-handled noise.

**What good looks like after a quarter:** noise-suppression saves on-call ~20 pages/week; recall-hit rate > 70% at steady state (most alerts are variations of things seen before); duplicate-action = 0 for destructive actions (approval gate + memory of "already scaled 2 min ago" prevents loops); wrong-context-asserted near zero — catastrophic if it drifts, because SRE trust collapses the moment the zombie confidently asserts a false root cause.

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
