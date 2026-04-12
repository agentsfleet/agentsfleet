Memory Layer Design (UseZombie edition)

  The 4 Memory Use Cases

  ┌──────────────────────────┬───────────────┬──────────────────────────────┬──────────────────────────────────────┐
  │         Use Case         │     Scope     │          Durability          │                Storage               │
  ├──────────────────────────┼───────────────┼──────────────────────────────┼──────────────────────────────────────┤
  │ 1. Conversation resume   │ zombie        │ crash-safe                   │ core.zombie_sessions (already built) │
  │    (last_event cursor)   │               │                              │                                      │
  ├──────────────────────────┼───────────────┼──────────────────────────────┼──────────────────────────────────────┤
  │ 2. Core memory           │ zombie        │ survives workspace           │ memory DB (new), category = 'core'   │
  │    (learned entity facts)│               │ destruction                  │                                      │
  ├──────────────────────────┼───────────────┼──────────────────────────────┼──────────────────────────────────────┤
  │ 3. Daily / ephemeral     │ zombie        │ 72h auto-prune               │ memory DB (new), category = 'daily'  │
  │    (reminders, todos)    │               │                              │                                      │
  ├──────────────────────────┼───────────────┼──────────────────────────────┼──────────────────────────────────────┤
  │ 4. Conversation scratch  │ run           │ dies with workspace          │ workspace SQLite (unchanged)         │
  │    (within-run working)  │               │                              │                                      │
  ├──────────────────────────┼───────────────┼──────────────────────────────┼──────────────────────────────────────┤
  │ 5. Workspace shared      │ workspace     │ durable                      │ workspace_memories (already built)   │
  │    (Warden/Echo obs)     │ (N zombies)   │                              │                                      │
  └──────────────────────────┴───────────────┴──────────────────────────────┴──────────────────────────────────────┘

  ---
  Log ≠ Memory

  The most important distinction in this design:

  Log    = what happened (tool calls, events, timestamps)  — UseZombie already has this (SSE, runs replay).
  Memory = what the zombie learned from what happened (distilled facts, judgments, preferences).

  A log answers: "what did the zombie do yesterday?"
  Memory answers: "what does the zombie now know that it didn't know before?"

  If memory is just "the daily log with extra steps" it shouldn't ship. The value
  comes from curation — the agent being taught to write distilled facts, not
  transcripts. That teaching lives in per-archetype skill templates (sibling
  workstream M14_002), not in the storage layer itself.

  ---
  What's Already Built — Don't Duplicate

  ┌──────────────────────────────┬───────────────────────────────────────────────────────────────────────────────────┐
  │            Table             │                                   What it is                                      │
  ├──────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────┤
  │ core.zombie_sessions         │ Conversation RESUME BOOKMARK. SQL comment says "history + memory" — the           │
  │                              │ implementation stores only {last_event_id, last_response}. It is a cursor, not    │
  │                              │ memory. Leave it alone. Tighten the comment.                                      │
  │                              │ Evidence: src/zombie/event_loop_helpers.zig:67-75                                 │
  ├──────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────┤
  │ workspace_memories           │ Cross-run observations written by Warden, read by Echo at boot. Workspace-level,  │
  │                              │ not zombie-level. Separate concern. M14 does not extend this.                     │
  │                              │ Evidence: src/memory/workspace.zig                                                │
  └──────────────────────────────┴───────────────────────────────────────────────────────────────────────────────────┘

  ---
  Where Memory Currently Goes (The Gap)

  Today, memory_store('user_pref', 'dark mode') lands in {workspace_dir}/memory.db
  (SQLite via NullClaw's default hybrid_keyword profile).

    Survives process crashes      ✓
    Survives zombie restart       ✓ (if same workspace)
    Survives workspace destruction ✗  ← THE GAP

  Workspaces are temporary worktrees destroyed after each run. So every zombie run
  today is a cold start. The tool exists; the durability doesn't. That is what M14_001
  closes.

  Evidence: src/executor/runner.zig:155, 200 (executor takes NullClaw defaults,
  does not configure a durable backend).

  ---
  Storage Tier — The Rejected Options

  ┌──────────────────────────────────┬───────────────────────────────────────────────────────────────────────────────┐
  │              Option              │                                  Why rejected                                 │
  ├──────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────┤
  │ SQLite on persistent host volume │ Zombies float across workers — local storage inaccessible next run.           │
  │                                  │ Also: shell tools can read sibling zombies' dirs (confused deputy).           │
  ├──────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────┤
  │ Markdown on networked FS         │ SQLite WAL fails on network FS; write-contention races; adds distributed      │
  │                                  │ storage operational surface.                                                  │
  ├──────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────┤
  │ pgvector / RAG semantic recall   │ Cosine over embeddings is a cheap approximation of reasoning — modern         │
  │                                  │ context windows + LLM judgment beats it for typical zombie memory sizes.      │
  │                                  │ Additive later if evidence demands it.                                        │
  ├──────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────┤
  │ Cloudflare R2 / Fly Volumes      │ OVH bare-metal deployment. Cross-cloud latency kills per-call memory ops.     │
  │                                  │ Fly Volumes don't apply (no Fly).                                             │
  └──────────────────────────────────┴───────────────────────────────────────────────────────────────────────────────┘

  ---
  Storage Tier — Chosen

  Dedicated Postgres DATABASE on the existing core cluster (not a separate instance).

  ┌────────────────────────┬──────────────────────────────────────────────────────────────────────────────────────┐
  │      Property          │                                     How                                              │
  ├────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────┤
  │ Isolation from core.*  │ Separate database name (memory), separate role (memory_runtime), separate migrations │
  ├────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────┤
  │ Floating-zombie safe   │ Any worker reaches the DB via the cluster's internal network                         │
  ├────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────┤
  │ Process boundary       │ Agent shell tools cannot reach memory — different protocol, different credentials    │
  ├────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────┤
  │ Operational familiarity│ Already run Postgres — zero new runtime skill                                        │
  ├────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────┤
  │ Escape hatch           │ If workload forces it, move to separate instance later (backup + restore, not rework)│
  └────────────────────────┴──────────────────────────────────────────────────────────────────────────────────────┘

  Boring by default. Zero innovation tokens spent.

  ---
  Human-Readable Memory — Export, Not Storage

  Markdown stays the human view. Postgres is the authoritative store.

  zombiectl memory export {zombie_id}   → writes markdown to OPERATOR's laptop
  [operator edits in vim, commits to git]
  zombiectl memory import {zombie_id}   → validates, scopes, upserts to Postgres

  File format per entry:

    ---
    key: lead_acme_corp
    category: core
    created: 2026-04-10T14:22:11Z
    updated: 2026-04-12T09:01:03Z
    zombie_id: zom_leadgen_01
    tags: [lead, enterprise, warm]
    ---

    Acme Corp (acme.com) — CTO Jane Park.
    Prefers email over LinkedIn DMs.
    Replied Apr 8 saying "circle back after Q2 planning."
    Budget confirmed >$50k. Not ready now — follow up Jun 15.

  ---
  The Three Replay Primitives

  Memory export/import composes with v1 primitives. Three distinct capabilities:

  ┌───────────────────────┬────────────────────┬────────────────────────┬──────────────────────────────────────────┐
  │     Primitive         │        When        │      What it does      │                 Source                   │
  ├───────────────────────┼────────────────────┼────────────────────────┼──────────────────────────────────────────┤
  │ Session replay        │ after a run        │ forensic playback      │ zombiectl runs replay (v1)               │
  ├───────────────────────┼────────────────────┼────────────────────────┼──────────────────────────────────────────┤
  │ Interrupt and steer   │ during a run       │ live correction        │ M21_001 (v1 done)                        │
  ├───────────────────────┼────────────────────┼────────────────────────┼──────────────────────────────────────────┤
  │ Memory export/import  │ between runs       │ surgical brain-edit    │ M14_001 (this milestone)                 │
  └───────────────────────┴────────────────────┴────────────────────────┴──────────────────────────────────────────┘

  ---
  Confused Deputy — The Security Rule This Surfaced

  If a zombie has any shell or file tool AND memory is backed by a shared filesystem,
  the zombie can bypass memory-API scoping by reading sibling directories directly.
  Rule added to docs/greptile-learnings/RULES.md as part of M14:

    Cross-tenant data that a sandboxed agent must not read cannot share a
    filesystem with the agent. Use a process boundary.

  Filesystem = same boundary as the agent's shell = unsafe.
  Postgres / Redis / any network protocol = different process, different credentials,
  different protocol = enforced boundary.

  ---
  What the Revised M14 Spec Scope Looks Like

  ┌──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────┐
  │      Original Section        │                              Revised Framing                                       │
  ├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────┤
  │ 1.0 Storage Backend Eval     │ Narrow — drop Fly/R2/SQLite-on-volume/pgvector. Decision: dedicated Postgres DB.  │
  ├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────┤
  │ 2.0 Memory Backend Wiring    │ Executor passes MemoryConfig per zombie; core/daily → Postgres, conversation →    │
  │                              │ workspace SQLite (unchanged).                                                      │
  ├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────┤
  │ 3.0 Cross-Run Persistence    │ Same — store in run N, workspace destroyed, recall in run N+1.                    │
  ├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────┤
  │ 4.0 Interfaces (NEW)         │ Add zombiectl memory export|import tool. Markdown frontmatter + body format.      │
  ├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────┤
  │ 5.0 Failure Modes            │ Add confused-deputy row; drop SQLite-on-NFS row (no longer applicable).            │
  └──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────┘

  Sibling workstreams named so they don't get dropped:
    M14_002 — per-archetype memory policies (skill templates that teach WHAT to remember)
    M14_003 — memory dashboard view (operator UI)
    M14_004 — memory-effectiveness metrics (duplicate-action rate, recall-hit rate)

  ---
  File References

  Spec:                docs/v2/active/M14_001_PERSISTENT_ZOMBIE_MEMORY.md
  Conversation cursor: schema/023_core_zombie_sessions.sql
                       src/zombie/event_loop_helpers.zig:67-75
  Workspace memory:    src/memory/workspace.zig
  Executor wiring:     src/executor/runner.zig:155, 200
  NullClaw profiles:   nullclaw/src/config_types.zig:850-942
  Session replay (v1): zombiectl/src/commands/runs.js
  Steer (v1):          docs/v1/done/M21_001_AGENT_INTERRUPT_AND_STEER.md

  ---
  Before implementation begins — one remaining question

  Memory size bounds per zombie. Spec §4.1 includes max_entries but the enforcement
  policy (reject new writes vs prune oldest `core` entries vs error back to agent)
  needs product input before it's set in stone. Defaulting to "prune oldest daily,
  error on core overflow" until told otherwise.
