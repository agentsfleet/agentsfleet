  ---
  PRE-REVIEW SYSTEM AUDIT

  Current System State

  Stack: Zig backend (zombied), Node CLI (zombiectl), React/Next.js frontend
  (ui/), Postgres + Redis infra, NullClaw agent runtime.

  Pending diff on main: Two changes staged:
  1. docs/ARCHITECTURE.md — +77 lines adding the M9 Agent Gamification &
  Self-Improvement section (the plan under review)
  2. docs/spec/v1/M5_007_VENDOR_NEUTRAL_OIDC_ABSTRACTION.md → docs/done/v1/
  (housekeeping rename)

  Branch: main (no feature branch yet for M9)

  Recent delivery cadence: Active — M5 (entitlements, billing, PostHog), M6
  (UUIDv7, free plan credits, paid scale), M8 (UUID-only clean-state) all landed
   in the last ~10 days. 33 done spec files, 7 open spec files, 15 schema
  migrations.

  In-Flight / Open Work

  Open spec files (not in docs/done/):
  - M4_008 — Sandbox Resource Governance (Firecracker) — P0 production blocker,
  PENDING
  - M6_004 — Marketing CTA Lead Capture Evaluation
  - M6_005 — GitHub CI Release Pipeline
  - M6_006 — V1 Acceptance E2E Gate
  - M7_001 — Deploy from Deployment Guide (dev/prod)
  - M7_002 — Documentation Production & Publish
  - M12_001 — Free Plan Exhaustion & Conversion UX

  No stashed work. No TODOS.md file.

  Pain Points Relevant to M9

  1. No M9 spec files exist — the architecture section is the only written plan.
   Four spec files are referenced (M9_001 through M9_004) but not yet created.
  2. M4_008 (Firecracker) is the biggest open blocker — M9 scoring depends on
  agent runs completing, and agent runs currently have no resource governance.
  Runaway agents = corrupt scoring data.
  3. No existing agent_run_scores, agent_profiles, or scoring tables in the
  schema. This is purely greenfield.
  4. Harness control plane (M5_002, M5_008) is done — profiles are
  workspace-scoped, compile/activate/resolve cycle works. M9's "harness
  auto-improvement" proposals would mutate these profiles.
  5. PostHog + Langfuse + OTel are wired — observability surfaces exist to
  consume scoring events.
  6. Worker stage executor (worker_stage_executor.zig) is the integration point
  — all stage results flow through executeRun() with token counts, wall seconds,
   and exit status already captured.

  FIXME/TODO in Source

  The grep for TODO/FIXME returned only node_modules noise — no source-level
  TODOs in the Zig codebase.

  Retrospective Check

  No prior M9 commits or branches. This is a fresh architecture proposal — no
  prior review cycles.

  ---
  STEP 0: Nuclear Scope Challenge + Mode Selection

  0A. Premise Challenge

  Is this the right problem to solve? M9 proposes a self-improving agent loop:
  score runs → inject score history → propose harness changes → auto-apply with
  trust controls. The core insight is sound — agent quality feedback loops are
  the moat for an agent platform.

  But is it the most direct path to value right now? The open work queue has
  M4_008 (Firecracker), M7_001 (deploy), and M6_006 (E2E gate) — all
  production-critical. M9 is a quality-of-life feature for a platform that
  hasn't shipped to production yet.

  What happens if we do nothing? Agents run without scoring, without feedback
  loops, without self-improvement. Operators manually tune harnesses. For v1
  launch with a small number of early users, this is... fine. The pain compounds
   with scale and multi-tenancy, not at launch.

  0B. Existing Code Leverage

  ┌────────────────────────┬────────────────────────────────────────────────┐
  │     M9 Sub-Problem     │                 Existing Code                  │
  ├────────────────────────┼────────────────────────────────────────────────┤
  │ Run metadata (tokens,  │ AgentResult struct, usage_ledger table,        │
  │ wall_seconds, exit_ok) │ emitNullclawRunEvent()                         │
  ├────────────────────────┼────────────────────────────────────────────────┤
  │ Run terminal state     │ state/machine.zig transition hooks,            │
  │ detection              │ DONE/BLOCKED terminal states                   │
  ├────────────────────────┼────────────────────────────────────────────────┤
  │ Per-workspace harness  │ Harness control plane: PUT source, POST        │
  │ mutation               │ compile, POST activate — full CRUD exists      │
  ├────────────────────────┼────────────────────────────────────────────────┤
  │ Event emission         │ PostHog events + OTel + Langfuse all wired in  │
  │                        │ worker_stage_executor.zig                      │
  ├────────────────────────┼────────────────────────────────────────────────┤
  │ Profile versioning &   │ profile_linkage_audit_artifacts table,         │
  │ audit                  │ immutable append-only rows                     │
  ├────────────────────────┼────────────────────────────────────────────────┤
  │ Workspace scoping      │ RLS tenant isolation (schema 007),             │
  │                        │ entitlements (schema 012)                      │
  └────────────────────────┴────────────────────────────────────────────────┘

  Key reuse: M9_001 (scoring) can read directly from usage_ledger +
  run_transitions + artifacts. No need to build new data capture. M9_004
  (harness auto-improvement) can call the existing compile/activate APIs rather
  than building a parallel mutation path.

  0C. Dream State Mapping

    CURRENT STATE                       THIS PLAN (M9)
  12-MONTH IDEAL
    ─────────────────────────           ─────────────────────────
  ─────────────────────────
    Agents run, produce PRs.            Score every run on 4 axes.
  Self-optimizing agent fleet.
    Operator manually tunes             Inject score history into
  Agents converge on optimal
    harness profiles.                   next run. Propose harness
  harness config per workspace.
    No quality signal beyond            changes when quality declines.
  Cost-per-quality ratio is a
    pass/fail from Warden.              Trust system gates automation.
  first-class product metric.
    No cross-run learning.              Leaderboard for visibility.
  Operator dashboard shows
                                                                           agent
   health like SRE

  monitors service health.

  This plan moves directly toward the 12-month ideal. It's the right trajectory.

  0D. Mode-Specific Analysis

  Context-dependent default: This is a greenfield feature on a pre-launch
  platform. Default is EXPANSION.

  But I need to flag a tension: M4_008 (Firecracker), M7_001 (deploy), and
  M6_006 (E2E gate) are all open and production-critical. M9 is ambitious scope
  on a platform that hasn't shipped yet.

  0E. Temporal Interrogation

    HOUR 1 (foundations):     Schema design — what tables, what indexes, what
  grants?
                              Does scoring happen in-process (worker) or async
  (background job)?

    HOUR 2-3 (core logic):   Scoring engine — deterministic formula. What are
  the axes?
                              How do you normalize across different profile
  topologies?
                              A 3-stage profile and a 7-stage profile aren't
  comparable on raw tokens.

    HOUR 4-5 (integration):  Context injection — the 512-token cap is smart, but
   how do you
                              serialize scores into a prompt-friendly format?
  JSON? Markdown?
                              What does the LLM actually DO with this context?

    HOUR 6+ (polish/tests):  Trust system — the "10 consecutive Gold+" threshold
   is arbitrary.
                              How do you tune it? Is it per-workspace or global?
                              Veto window UX — how does `zombiectl` display
  countdown?

  0F. Mode Selection

  I recommend HOLD SCOPE for this review. Here's why:

  1. SCOPE EXPANSION would be the default for greenfield, but M9 is already very
   ambitious (4 workstreams, scoring engine, trust system, auto-improvement
  proposals, veto windows). Expanding further before shipping v1 is premature.
  2. HOLD SCOPE — The plan's vision is right. The architecture section captures
  the key invariants and flows well. What it needs is rigorous validation of the
   execution plan: schema design, error paths, the scoring formula, the trust
  model edge cases, and whether M9 should ship before or after the production
  blockers (M4_008, M7_001).
  3. SCOPE REDUCTION — Could argue for this given the production backlog, but
  the M9 architecture section is a design doc, not an implementation PR.
  Reviewing the design at full scope is appropriate even if implementation is
  phased.

⏺ Issue 1: Mode Selection

  We recommend B (HOLD SCOPE). M9's vision is already ambitious (4 workstreams,
  trust system, auto-improvement loop). The platform hasn't shipped to
  production yet — M4_008 (Firecracker), M7_001 (deploy), M6_006 (E2E gate) are
  all open blockers. Expanding M9 scope before those land is premature. But the
  design itself deserves rigorous validation at full scope so it's ready when
  the time comes. This maps to your "engineered enough" preference — not
  under-designed, not over-designed.

⏺ User answered Claude's questions:
  ⎿  · We recommend B: HOLD SCOPE. The M9 plan is already ambitious (4
     workstreams, scoring engine, trust system, veto windows). The platform has
     production blockers ahead of it (M4_008, M7_001, M6_006). Expanding M9
     further now is premature, but reviewing the design rigorously is the right
     call. Which review mode? → A) SCOPE EXPANSION

⏺ EXPANSION mode selected. Committing fully — I will not drift toward reduction.
   Let me complete the remaining Step 0 analyses.


⏺ Good. Now I have the full taste calibration.

  Taste Calibration

  Well-designed patterns (style references):
  1. topology.zig — Clean separation of parsing (ProfileDoc → Profile), explicit
   validation function, named error types (TopologyError), deterministic
  defaults with defaultProfile(). This is how M9 types should be structured.
  2. reliable_call.zig — Generic retry wrapper with cooperative cancellation,
  deadline enforcement, error classification, and structured logging. Beautiful
  composability. M9's scoring engine should follow this "small, composable,
  well-tested" pattern.
  3. entitlements.zig — Full audit trail (insertAuditSnapshot), load → evaluate
  → enforce → audit flow. The M9 scoring/trust system should mirror this:
  compute → persist → enforce → audit.

  Anti-patterns to avoid:
  1. worker_stage_executor.zig:executeRun() — 500+ line function with deeply
  nested control flow (while+switch+while+switch). Metrics, Langfuse, PostHog,
  billing, state transitions all inline. M9 scoring MUST NOT be added inline to
  this function — it should be a post-run hook called from one point.
  2. Duplicate billing finalization paths — finalizeRunForBilling is called in
  4+ places with slightly different args in executeRun(). The alloc-failure
  fallback path at line 364 duplicates the happy path at line 396. M9 must not
  repeat this pattern.

  ---
  Step 0D: EXPANSION Mode Analysis

  10x Check

  The current plan: score runs on 4 axes → inject history → propose harness
  changes → trust-gated auto-apply.

  The 10x version: UseZombie becomes the first agent platform where agents
  demonstrably improve over time, and operators can prove it to their
  stakeholders.

  The scoring engine isn't just a quality signal — it's the foundation for:
  - Agent SLAs — "95% of runs score Gold+ within 3 attempts"
  - Cost optimization — "This profile change saved 40% tokens while maintaining
  Gold quality"
  - Competitive moat — No other agent platform has agent self-improvement with
  auditable trust controls
  - Pricing leverage — Quality scoring enables value-based pricing ("pay for
  Gold+ runs only")

  The 10x delta for ~2x effort: add workspace-level quality dashboards and
  comparative profile analytics to the scoring engine. Operators don't just see
  scores — they see trends, regressions, and ROI on harness changes.

  Platonic Ideal

  If the best engineer had unlimited time:

  The operator opens Mission Control and sees an "Agent Health" dashboard — a
  miniature SRE dashboard for their agents. Green/yellow/red status per
  workspace. Click into a workspace and see:
  - Rolling quality trend (sparkline)
  - Cost-per-quality ratio trending down
  - Active harness proposals with diffs and predicted impact
  - Trust level badge with streak count
  - One-click approve/veto with undo

  The agent's next run begins with a small context block: "Your last 5 runs
  scored Gold, Gold, Silver, Gold, Gold. The Silver was a timeout on a large
  repo. You have been trusted with auto-improvement proposals." The agent
  doesn't just receive this information — it uses it to adjust its behavior.

  The feeling: Operators feel like they have a team of junior engineers that are
   getting measurably better every week, with a paper trail they can show their
  CTO.

  Delight Opportunities

  1. Sparkline in zombiectl agent status — ASCII sparkline of recent scores.
  ▂▅▇▇▅▇▇▇ — instantly see quality trends without leaving the terminal.
  2. Score badge in PR description — "This PR was produced by a Gold-tier agent
  (87/100)" in the PR body. Social proof for reviewers.
  3. Improvement velocity metric — "This workspace's agents improved 15% in the
  last 30 days." Operators share this with stakeholders.
  4. "Agent of the week" digest — Weekly summary email: top-performing agent,
  biggest improvement, worst regression. 30 min to build on top of scoring.
  5. Score-gated billing credit — Runs scoring below Bronze are automatically
  marked non-billable. Operators never pay for garbage output.

  ---
  Now for the first real decision point:

  Issue 2: Scoring Engine Placement

  The architecture says "Score is deterministic — identical result from same run
   metadata, no LLM in scoring path." This is correct and critical. But the plan
   doesn't specify WHERE scoring happens:

  - Option A (In-worker, synchronous): After executeRun() reaches a terminal
  state, the worker computes the score before XACK-ing the Redis message. Score
  is guaranteed to exist when the run is visible to the API. Simple. But adds
  latency to the hot path and couples scoring to worker availability.
  - Option B (Async background job): Worker emits a run_terminal event. A
  separate scorer process/goroutine picks it up and computes the score.
  Decouples scoring from run execution. But introduces eventual consistency —
  API might return a run without a score briefly.
  - Option C (In-worker, post-XACK): Worker XACK-s the message, then computes
  the score as a best-effort post-commit hook. Run is acknowledged quickly,
  scoring happens right after. If worker crashes between XACK and scoring, the
  score is missing — needs a reconciler.

  We recommend A (In-worker, synchronous). The scoring is deterministic and
  reads only from data already in memory (AgentResult, usage_ledger rows). The
  computation is trivial — normalize 4 numbers, bucket into a tier. Adding 5ms
  of synchronous scoring doesn't meaningfully affect a pipeline that takes
  minutes. This maps to your "explicit over clever" preference — no async
  reconciliation complexity for a trivial computation. Option B/C introduce
  failure modes that don't justify the decoupling.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: In-worker synchronous scoring. The computation is trivial
      (normalize 4 numbers, ~5ms) on data already in memory. Adding async here
     introduces eventual consistency and reconciliation complexity for zero
     performance benefit on a pipeline that takes minutes. Where should scoring
     happen? → A) In-worker sync (Recommended)

⏺ Good. In-worker synchronous scoring it is.

  Issue 3: M9 Implementation Ordering vs Production Blockers

  The open spec queue has M4_008 (Firecracker), M7_001 (deploy), M6_006 (E2E
  gate) — all production-critical. M9 is a quality-of-life feature for agents.

  However, M9_001 (scoring) and M9_002 (persistence) are low-risk,
  high-information features. They produce data that helps you understand agent
  quality BEFORE production launch. Scoring in dev/staging gives you baseline
  quality metrics to compare against after Firecracker lands.

  M9_003 (context injection) and M9_004 (auto-improvement proposals) are
  higher-risk and depend on having enough run history to be meaningful.

  We recommend A: Ship M9_001 + M9_002 now, defer M9_003 + M9_004 until after
  production blockers. You get quality signal immediately in dev runs, which
  informs harness tuning. The trust system and auto-improvement loop can wait
  until agents are actually running in production with real workloads. This is
  EXPANSION in vision, pragmatic in sequencing.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: Ship scoring + persistence (M9_001/002) now alongside
     production blockers. They're low-risk, produce quality signal in
     dev/staging, and inform harness tuning before launch. Defer context
     injection + auto-improvement (M9_003/004) until post-launch when real run
     history exists. What ordering? → C) Full M9 now, parallel to blockers

⏺ Full M9 in parallel — maximum ambition. Committed.

  Issue 4: Trust Threshold Design

  The plan says "10 consecutive Gold+ runs" for TRUSTED status. This is a
  critical design parameter.

  Concern: 10 consecutive is very strict. One bad run (network timeout, LLM
  provider hiccup, large repo edge case) resets the counter entirely. An agent
  could be genuinely excellent but never earn trust because external failures
  are indistinguishable from agent failures in a consecutive count.

  Three approaches:

  - A) 10 consecutive, as-is: Simple, very conservative. Trust is hard to earn,
  easy to lose. Matches the "fail-closed" philosophy. But conflates agent
  quality with infrastructure reliability.
  - B) 10 of last 12 (sliding window): Allows 2 failures in the window. Robust
  against infrastructure noise. Still strict — 83% Gold+ rate required. More
  complex to compute and explain.
  - C) 10 consecutive with failure classification: Keep the consecutive model
  but exclude infrastructure failures (TIMEOUT, OOM, CONTEXT_OVERFLOW from
  M9_003) from the streak reset. Only agent-attributable failures (BAD_OUTPUT,
  low quality score) reset the streak. This maps neatly to M9_003's failure
  classification.

  We recommend C: 10 consecutive with failure classification exclusions. It
  preserves the simplicity and conservatism of the consecutive model while not
  punishing agents for infrastructure problems. This directly leverages M9_003's
   failure classification work — the same taxonomy serves both context injection
   AND trust evaluation. Without this, trust becomes a measure of infrastructure
   reliability, not agent quality.

⏺ User answered Claude's questions:
  ⎿  · We recommend C: Keep the 10-consecutive model but exclude infrastructure
     failures (TIMEOUT, OOM, CONTEXT_OVERFLOW) from streak resets. Only
     agent-attributable failures (BAD_OUTPUT, low score) break the streak. This
     leverages M9_003 failure classification and measures agent quality, not
     infra reliability. What trust model? → C) 10 consecutive + failure
     classification (Recommended)

❯

  ---
  SECTION 1: Architecture Review

  Critical Finding: The agent_id Entity Does Not Exist

  This is the most fundamental issue. The M9 specs reference agent_id
  extensively — agent_run_scores.agent_id, agent_profiles.agent_id, GET
  /v1/agents/{agent_id}/scores, zombiectl agent profile <agent-id>.

  But the current system has no concept of a named agent entity. Looking at the
  schema and codebase:
  - Runs belong to workspaces, not agents
  - Runs are submitted by users (requested_by field, Clerk user ID)
  - The pipeline is executed by roles (echo, scout, warden) within a single run
  - There is no agents table, no agent_id column anywhere

  The spec treats agent_id as a given, but this is a new entity that needs to be
   defined. Without clarity here, the entire M9 data model is underspecified.

  Possible interpretations:
  - A) Agent = Workspace. Each workspace has one "agent" (the pipeline).
  agent_id is just workspace_id with a different name. Simplest, but misleading
  — a workspace isn't an agent.
  - B) Agent = Profile. Each harness profile is an "agent." A workspace could
  have multiple agents with different profiles. Maps well to the
  auto-improvement loop (each profile evolves independently). But profiles
  already have profile_id.
  - C) Agent = New first-class entity. A named agent registered within a
  workspace, linked to a profile, with its own identity and lifecycle. Most
  expressive, most complex. Adds a new table and FK relationships.

  We recommend B: Agent = Profile. The harness profile already defines the
  agent's behavior (stages, tools, prompts). Scoring a "profile" for quality
  makes semantic sense. Auto-improvement proposals that change a profile
  naturally scope to that profile's performance history. And the entity already
  exists — agent_profiles in the spec is essentially profile_run_metrics. This
  avoids introducing a new entity while giving the concept clear boundaries.

⏺ User answered Claude's questions:
  ⎿  · We recommend B: Agent = Profile. The harness profile already defines
     agent behavior (stages, tools, prompts). Scoring profiles for quality is
     semantically clean. Auto-improvement proposals naturally scope to the
     profile they improve. The entity already exists — no new table needed for
     the identity, just metrics on top. What is an 'agent' in M9? → B) Agent =
     Profile (Recommended)

⏺ Agent = Profile. Good — this grounds the entire M9 data model on existing
  infrastructure.

  Issue 6: Scoring Axes Depend on Non-Existent Data

  M9_001 dim 1.1 defines four scoring axes:
  1. Completion (40%) — "did the run reach terminal state cleanly?" — OK, this
  exists. Terminal states are DONE, BLOCKED, NOTIFIED_BLOCKED in
  state/machine.zig.
  2. Error rate (30%) — "unhandled exceptions or non-zero exits" — Partial.
  AgentResult.exit_ok captures per-stage exit status. But "unhandled exceptions"
   aren't structured — they're in stderr/logs.
  3. Latency percentile (20%) — "p50/p95 vs workspace baseline" — DOES NOT
  EXIST. There's no baseline tracking. This requires historical latency
  aggregation per workspace before any individual run can be scored on this
  axis.
  4. Resource efficiency (10%) — "CPU/memory vs declared sandbox limits" — DOES
  NOT EXIST. M4_008 (Firecracker sandbox) is PENDING. There are no resource
  metrics, no sandbox limits, no CPU/memory measurements.

  Two of four axes reference data that doesn't exist yet. This is a CRITICAL GAP
   for a foundation layer.

  - A) Reduce to 2 axes now, expand later: Score on completion (60%) and error
  rate (40%) for now. Add latency and resource axes when baselines and
  Firecracker land. Score formula is versioned — historical scores preserved,
  new runs use new formula.
  - B) Stub the missing axes with defaults: Latency axis scores 50 (neutral)
  until baseline is established after 10+ runs. Resource axis scores 50 until
  M4_008 lands. Allows the full 4-axis formula to exist from day one.
  - C) Build baseline infrastructure as part of M9_001: Add workspace baseline
  tracking tables (rolling p50/p95 latency, avg resource usage) as part of the
  scoring engine. Firecracker dependency remains for resource axis, but latency
  baseline can be bootstrapped from existing usage_ledger.agent_seconds data.

  We recommend C: Build latency baseline infrastructure, stub resource axis.
  Latency data already exists in usage_ledger.agent_seconds — you just need an
  aggregate. Resource axis gets a neutral 50 until Firecracker lands. This
  delivers 3 meaningful axes on day one and the full 4 when M4_008 ships.
  Partial signal is better than no signal, and stubbing one axis is acceptable;
  stubbing two makes the whole score hollow.

⏺ User answered Claude's questions:
  ⎿  · We recommend C: Build latency baseline from existing usage_ledger data
     (it's already there), stub resource axis at 50 until Firecracker lands.
     This gives 3 meaningful axes on day one. Stubbing one axis is acceptable;
     stubbing two makes the score hollow. How should we handle the missing data?
      → C) Latency baseline + stub resource (Recommended)

⏺ 3 real axes + stubbed resource. Good.

  Issue 7: Architecture Coupling — Scoring Hook Placement

  Per the taste calibration, worker_stage_executor.zig:executeRun() is already a
   500+ line function with inline metrics, Langfuse, PostHog, billing, and state
   transitions. The anti-pattern is adding MORE inline code.

  The scoring hook needs to fire ONCE at the terminal state, not per-stage. The
  plan needs to specify exactly where the scoring call goes and how it avoids
  the coupling trap.

  Looking at executeRun(), there are 5 exit paths:
  1. Line ~346: DONE — successful run, PR opened
  2. Line ~388: DONE (alloc-failure fallback) — same terminal state, error in
  summary generation
  3. Line ~429: BLOCKED — stage graph blocked
  4. Line ~481: BLOCKED (retries exhausted) — ran out of attempts
  5. Implicit: error returns — any try that fails propagates up

  The scoring call must cover all 5 paths. If it's added inline at each exit
  point (like billing is today), that's the anti-pattern repeated. Instead, it
  should be a single deferred call using Zig's defer or an explicit cleanup
  block at function exit.

  This is an architectural recommendation, not a question — I'm noting it for
  the spec. The scoring call should be structured as:

  // At function entry:
  var terminal_state: ?TerminalOutcome = null;
  defer if (terminal_state) |outcome| {
      scoreRun(conn, ctx, outcome, total_tokens, total_wall_seconds);
  };

  // At each exit point, set terminal_state:
  terminal_state = .{ .state = .DONE, .attempt = attempt, ... };

  This ensures exactly one scoring call regardless of exit path and avoids the
  5-point inline pattern.

  OK — moving on. No question needed here; the fix is obvious.

  ---
  System Architecture Diagram (M9 additions)

                                ┌─────────────────────────────────────────────┐
                                │            zombied worker                   │
                                │                                             │
    Redis ──XREADGROUP──►       │  ┌─────────────┐    ┌──────────────────┐    │
                                │  │ executeRun() │───►│ Stage Executor   │   │
                                │  │              │    │ Echo→Scout→Warden│   │
                                │  └──────┬───────┘    └──────────────────┘   │
                                │         │ terminal state                    │
                                │         ▼                                   │
                                │  ┌──────────────┐                           │
                                │  │ M9_001       │  NEW: Scoring Engine      │
                                │  │ scoreRun()   │  - completion axis        │
                                │  │              │  - error rate axis        │
                                │  │ deterministic│  - latency axis (vs p50)  │
                                │  │ no LLM       │  - resource axis (stub)   │
                                │  └──────┬───────┘                           │
                                │         │ score + tier                      │
                                │         ▼                                   │
                                │  ┌──────────────┐                           │
                                │  │ M9_002       │  NEW: Persist             │
                                │  │ persist()    │  → agent_run_scores       │
                                │  │              │  → agent_profiles update  │
                                │  │              │  → PostHog event          │
                                │  └──────┬───────┘                           │
                                │         │                                   │
                                │         ▼                                   │
                                │  ┌──────────────┐                           │
                                │  │ M9_003       │  NEW: Failure Analysis    │
                                │  │ analyze()    │  - classify failure       │
                                │  │              │  → agent_run_analysis     │
                                │  └──────────────┘                           │
                                │                                             │
                                │  ┌──────────────┐                           │
                                │  │ M9_003       │  ON NEXT RUN START:       │
                                │  │ inject()     │  Load last 5 scores       │
                                │  │              │  + failure analyses       │
                                │  │              │  Build ScoringContex      │
                                │  │              │  Prepend to system msg    │
                                │  └──────────────┘                           │
                                └─────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────────┐
    │  M9_004: Auto-Improvement (background / on-trigger)                     │
    │                                                                         │
    │  After each score persist:                                              │
    │    check 5-run rolling avg decline  ──YES──►  LLM proposal generation   │
    │                                                      │                  │
    │                                               ┌──────┴───────┐          │
    │                                               │ TRUSTED?     │          │
    │                                               ├──────┬───────┤          │
    │                                          AUTO │      │ MANUAL│          │
    │                                    VETO_WINDOW│      │PENDING│          │
    │                                    (24h timer)│      │REVIEW │          │
    │                                               ▼      ▼       │          │
    │                                         ┌─────────────┐      │          │
    │                                         │ Apply chang │      │          │
    │                                         │ via harness │      │          │
    │                                         │ compile/    │      │          │
    │                                         │ activate AP │      │          │
    │                                         └──────┬──────┘      │          │
    │                                                │             │          │
    │                                         Tag next 5 runs      │          │
    │                                         post_change_windw    │          │
    │                                         Compute score_delt   │          │
    │                                         3 negative → STALLE  │          │
    └─────────────────────────────────────────────────────────────────────────┘

    ┌──────────────────────────────────────────────────────────┐
    │  API / CLI Layer (read paths)                            │
    │                                                         │
    │  GET /v1/agents/{profile_id}/scores                      │
    │  GET /v1/agents/{profile_id}/profile                     │
    │  GET /v1/workspaces/{ws_id}/leaderboard                  │
    │  zombiectl agent scores / profile / leaderboard          │
    │  zombiectl agent proposals list / approve / veto / reject│
    └──────────────────────────────────────────────────────────┘

  Dependency Graph (before/after M9)

  BEFORE M9:
    runs ──► run_transitions ──► usage_ledger
    workspaces ──► harness_profiles ──► profile_linkage_audit

  AFTER M9:
    runs ──► run_transitions ──► usage_ledger
      │                                │
      ▼                                ▼
    agent_run_scores ◄──── scoring_engine (reads both)
      │
      ▼
    agent_run_analysis (M9_003)
      │
      ▼
    agent_profiles (aggregate)
      │
      ▼
    agent_improvement_proposals (M9_004)
      │
      ▼
    harness_change_log ──► harness_profiles (existing CRUD)

  Coupling Concerns

  New coupling: worker_stage_executor.zig → scoring/engine.zig. This is
  justified — scoring must happen at run termination. But it must be a single
  function call, not scattered logic.

  New coupling: agent_improvement_proposals → harness control plane APIs. This
  is the riskiest coupling — LLM-generated proposals mutating production harness
   config. The existing compile/activate cycle already validates profiles, which
   is a safety net. But the proposals themselves are LLM output, which
  introduces a new trust boundary.

  Scaling Characteristics

  - 10x load: Scoring adds one synchronous DB write per run. At 10x, this is ~10
   more writes/sec. Trivial.
  - 100x load: The leaderboard query (ORDER BY lifetime_score_avg DESC LIMIT 20)
   is fine with an index. The agent_profiles aggregate update is a single-row
  upsert per run. The concern at 100x is the agent_run_analysis table growing
  unbounded — needs a retention policy.

  Rollback Posture

  M9 is additive (new tables, new functions). Rollback = stop calling scoring
  functions + drop tables. Feature flag on the scoring call is sufficient.
  Reversibility: 4/5.

  STOP. Moving to Section 2.

  ---
  SECTION 2: Error & Rescue Map

    METHOD/CODEPATH                    | WHAT CAN GO WRONG                    |
  EXCEPTION/ERROR
    -----------------------------------|--------------------------------------|-
  -----------------
    scoreRun() [M9_001]               | Division by zero (0 total runs       |
  ArithmeticError
                                       |   in baseline)                       |
                                       | usage_ledger query returns no rows   |
  pg.Error / null row
                                       | Score out of 0-100 range after       |
  Logic error
                                       |   normalization                      |
                                       | DB write for score fails             |
  pg.Error
    -----------------------------------|--------------------------------------|-
  -----------------
    persistScore() [M9_002]           | agent_run_scores INSERT conflict     |
  pg.Error (23505)
                                       |   (duplicate run_id score)           |
                                       | agent_profiles UPSERT race          |
  pg.Error
                                       |   (concurrent writes)               |
                                       | PostHog event emission fails         |
  network/timeout
    -----------------------------------|--------------------------------------|-
  -----------------
    analyzeRun() [M9_003]             | Run metadata incomplete (missing     |
  null field
                                       |   stderr, no exit code)              |
                                       | failure_class doesn't match any      |
  Logic error
                                       |   taxonomy entry                     |
    -----------------------------------|--------------------------------------|-
  -----------------
    injectScoringContext() [M9_003]    | No prior scores (first run)          |
  Expected: empty ctx
                                       | Token counting exceeds 512           |
  Truncation logic
                                       | DB query for last 5 scores fails     |
  pg.Error
    -----------------------------------|--------------------------------------|-
  -----------------
    generateProposal() [M9_004]       | LLM call times out                   |
  Timeout
                                       | LLM returns malformed JSON           |
  JSON parse error
                                       | LLM returns empty/refusal            |
  Empty response
                                       | LLM proposes forbidden field         |
  Schema validation
                                       |   (auth, billing, network)           |
    rejection
                                       | LLM proposes value out of range      |
  Schema validation
                                       | DB write for proposal fails          |
  pg.Error
    -----------------------------------|--------------------------------------|-
  -----------------
    autoApplyProposal() [M9_004]      | Harness compile fails with new       |
  CompileError
                                       |   proposed config                    |
                                       | Harness activate fails               |
  ActivateError
                                       | Concurrent proposal and manual       |
  Race condition
                                       |   harness change                     |
                                       | Background job crashes between       |
  Partial state
                                       |   VETO_WINDOW and APPLIED            |
    -----------------------------------|--------------------------------------|-
  -----------------
    evaluateTrust() [M9_004]          | agent_profiles row doesn't exist     |
  FirstRunEdge
                                       | consecutive_gold_plus count          |
  Logic error
                                       |   diverges from actual score history |

  Rescue Status

    ERROR                              | RESCUED?  | RESCUE ACTION
   | USER SEES
    -----------------------------------|-----------|----------------------------
  -|-----------
    Division by zero in baseline       | N ← GAP   | —
   | 500 ← BAD
    usage_ledger no rows               | N ← GAP   | —
   | 500 ← BAD
    Score out of 0-100 range           | N ← GAP   | —
   | Invalid score
    DB write for score fails           | N ← GAP   | —
   | Score missing
    INSERT conflict (dup score)        | ? (spec)  | Should be ON CONFLICT
   | Idempotent
    agent_profiles UPSERT race        | ? (spec)  | Should be ON CONFLICT
  | Stale profile
    PostHog emission fails             | Y (exist) | Fire-and-forget pattern
    | Nothing (OK)
    Run metadata incomplete            | N ← GAP   | —
   | Missing analysis
    Failure class no match             | N ← GAP   | —
   | UNKNOWN class
    Last 5 scores query fails          | N ← GAP   | —
   | No context
    LLM call timeout                   | N ← GAP   | —
   | No proposal
    LLM malformed JSON                 | Y (spec)  | Reject, do not retry
    | No proposal (OK)
    LLM empty/refusal                  | N ← GAP   | —
   | No proposal
    LLM proposes forbidden field       | Y (spec)  | Schema validation reject
    | No proposal (OK)
    Harness compile fails              | N ← GAP   | —
   | Stale proposal
    Concurrent proposal + manual       | N ← GAP   | —
   | Config conflict
    Background job crash               | N ← GAP   | —
   | Stuck VETO_WINDOW

  11 CRITICAL GAPS in the error map. The spec doesn't address most failure
  modes.

  Issue 8: Scoring must be fail-safe — never block the run

  The most critical rescue decision: if scoreRun() or persistScore() fails, what
   happens to the run itself?

  The run has already reached terminal state (DONE/BLOCKED). Scoring is a
  post-termination enrichment. If scoring fails, the run must still be visible,
  the PR must still be created, and the XACK must still happen. Scoring failure
  must NEVER block or fail the run.

  This means every scoring/persistence/analysis call must be wrapped in a catch
  that:
  1. Logs the error with full context (run_id, workspace_id, error class)
  2. Emits a agent.scoring.failed PostHog event
  3. Continues run finalization normally
  4. Leaves the score as null (absent) — the API must handle null scores
  gracefully

  We recommend A: Fail-safe scoring with structured error logging. This is the
  only defensible approach. Scoring is an enrichment, not a gate.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: Fail-safe scoring. If scoreRun() or persistScore() fails,
      catch the error, log with full context, emit agent.scoring.failed event,
     and continue run finalization. Score is null (absent) for that run. API
     handles null gracefully. Scoring must NEVER block a run. Agree? → A)
     Fail-safe with structured logging (Recommended)

⏺ Good. Fail-safe scoring. The 11 gaps all get the same treatment:

  Rescue actions for all gaps (to be specified in the implementation):

  ┌────────────────────┬────────────────────────────────────────────────────┐
  │        Gap         │                        Fix                         │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Division by zero   │ Guard: if baseline_count < 1, score latency axis   │
  │ in baseline        │ at 50 (neutral)                                    │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ usage_ledger no    │ Guard: if no usage rows, score completion=0, log   │
  │ rows               │ warning                                            │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Score out of 0-100 │ Clamp: @min(100, @max(0, raw_score))               │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ DB write for score │ Fail-safe catch, log, continue                     │
  │  fails             │                                                    │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Run metadata       │ Default to failure_class = UNKNOWN, log warning    │
  │ incomplete         │                                                    │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Failure class no   │ Map to UNKNOWN, never panic on unclassified        │
  │ match              │ failure                                            │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Last 5 scores      │ Inject empty context block ("no prior history"),   │
  │ query fails        │ log                                                │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ LLM call timeout   │ Log, no proposal generated this cycle. Next        │
  │ (proposal)         │ trigger retries                                    │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ LLM empty/refusal  │ Treat as malformed, reject, log                    │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Harness compile    │ Transition proposal to REJECTED with reason        │
  │ fails              │ COMPILE_FAILED, log                                │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Concurrent         │ SELECT ... FOR UPDATE on profile row before        │
  │ proposal + manual  │ applying. Last writer wins with audit trail        │
  ├────────────────────┼────────────────────────────────────────────────────┤
  │ Background job     │ Reconciler query: WHERE status = 'VETO_WINDOW' AND │
  │ crash              │  auto_apply_at < now() - interval '1 hour' picks   │
  │                    │ up stuck proposals                                 │
  └────────────────────┴────────────────────────────────────────────────────┘

  OK. Moving to Section 3.

  ---
  SECTION 3: Security & Threat Model

  Issue 9: LLM-Generated Proposals Mutate Production Harness Config

  This is the highest-severity security concern in M9. M9_004 dim 1.4 says:

  "Proposal generation uses the agent's own LLM call with a constrained prompt:
  inject last 10 run analyses + current harness config; output is validated
  against the proposed_changes schema before persisting"

  The threat: An LLM produces a proposed_changes JSON that passes schema
  validation but is semantically malicious or harmful. Examples:
  - system_prompt_appendix: "Ignore all previous instructions..." — prompt
  injection via harness mutation
  - max_tokens: 999999999 — resource exhaustion via absurd limits
  - timeout_seconds: 1 — denial of service via impossibly short timeout
  - tool_allowlist: ["shell"] — privilege escalation by enabling dangerous tools

  The schema validation in the spec only checks structure (target_field,
  current_value, proposed_value). It doesn't validate semantic safety.

  Mitigations needed:
  - Value range constraints: max_tokens capped at workspace entitlement limit.
  timeout_seconds bounded between 30 and RUN_TIMEOUT_MS/1000. tool_allowlist can
   only restrict, never expand beyond what the profile already allows.
  - Prompt appendix sanitization: The system_prompt_appendix field is the most
  dangerous. It's literally injecting LLM-generated text into future system
  prompts. This needs either: (a) removal from proposable fields entirely, or
  (b) a strict character/length limit plus a blocklist scan for injection
  patterns.
  - Diff review in veto window: The CLI must show the EXACT diff, not a summary,
   so operators can catch semantic issues.

  We recommend A: Remove system_prompt_appendix from proposable fields and add
  value range constraints for all numeric fields. Prompt injection via harness
  mutation is an unacceptable attack surface. Let agents tune numeric parameters
   (tokens, timeout, tools) but not inject arbitrary text into future prompts.
  This can always be relaxed later — but shipping with this vector open would be
   irresponsible.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: Remove system_prompt_appendix from proposable fields.
     It's LLM-generated text injected into future system prompts — a direct
     prompt injection vector via harness mutation. Let agents tune numeric
     params (tokens, timeout, tool_allowlist) with range constraints, but not
     inject arbitrary text. This can be relaxed later. How should we scope
     proposable fields? → A) Remove prompt appendix, constrain numerics
     (Recommended)

⏺ Good. No text injection from LLM proposals. Numeric + tool_allowlist
  (restrict-only) with bounds.

  Remaining Security Findings

  Finding: LLM prompt injection via system_prompt_appendix
  Severity: HIGH
  Mitigated?: YES — removed from proposable fields
  ────────────────────────────────────────
  Finding: Numeric value extremes in proposals
  Severity: MEDIUM
  Mitigated?: YES — range constraints from entitlements
  ────────────────────────────────────────
  Finding: tool_allowlist privilege escalation
  Severity: MEDIUM
  Mitigated?: Needs constraint: can only restrict, never expand beyond current
    profile
  ────────────────────────────────────────
  Finding: Cross-workspace data leakage in leaderboard
  Severity: MEDIUM
  Mitigated?: Spec says workspace-scoped auth (M9_002 dim 2.4) — OK if enforced
  ────────────────────────────────────────
  Finding: agent_run_analysis stores stderr tail
  Severity: LOW
  Mitigated?: Already bounded to 200 lines. No credential scrubbing specified —
    should strip env vars
  ────────────────────────────────────────
  Finding: Proposal generation LLM call uses agent's own context
  Severity: LOW
  Mitigated?: Constrained prompt. But if the agent has BYOK keys, the proposal
    LLM call uses the operator's API key to generate a change to the operator's
    own harness. This is expected behavior.
  ────────────────────────────────────────
  Finding: harness_change_log is auditable
  Severity: LOW
  Mitigated?: OK — the spec includes full audit trail

  Issue 10: Stderr in failure analysis may contain secrets

  M9_003 dim 1.1 says "Extract failure signals from run terminal state: exit
  code, stderr tail (last 200 lines)." Stderr from agent execution could contain
   environment variables, API keys from error messages, or database connection
  strings that leaked into error output.

  This is a standard concern — the fix is a scrubbing function that redacts
  patterns matching API_KEY=, Bearer , DATABASE_URL=, etc. before persisting to
  agent_run_analysis.

  OK — noting this for the spec, not a question. The fix is obvious: add a
  scrubSecrets() pass over stderr before persistence.

  Moving to Section 4.

  ---
  SECTION 4: Data Flow & Interaction Edge Cases

  Data Flow: Score Computation

    TERMINAL STATE ──► READ usage_ledger ──► READ baseline ──► COMPUTE ──►
  PERSIST
         │                    │                   │                │           │
         ▼                    ▼                   ▼                ▼           ▼
    [no terminal?]     [no usage rows?]    [no baseline?]   [out of range?] [DB
  error?]
    skip scoring       score=0, warn       axis=50 neutral   clamp 0-100
  fail-safe

  Data Flow: Context Injection

    RUN START ──► QUERY last 5 scores ──► BUILD ScoringContext ──► INJECT into
  system msg
         │                │                      │                        │
         ▼                ▼                      ▼                        ▼
    [first run?]    [DB query fails?]      [> 512 tokens?]         [injection
  disabled?]
    orientation     empty context, log     truncate oldest first    skip
  injection
    block

  Data Flow: Proposal Generation & Application

    SCORE PERSIST ──► CHECK rolling avg ──► TRIGGER? ──► LLM CALL ──► VALIDATE
  ──► PERSIST
         │                  │                  │              │            │
          │
         ▼                  ▼                  ▼              ▼            ▼
          ▼
    [persist fail?]  [< 5 runs?]        [no decline?]  [timeout?]  [invalid?]
  [DB error?]
    fail-safe        skip check          no proposal    no proposal  reject
    fail-safe

    VETO_WINDOW ──► 24h passes ──► COMPILE harness ──► ACTIVATE ──► TAG next 5
  runs
         │               │                │                │               │
         ▼               ▼                ▼                ▼               ▼
    [operator vetoes?] [job crashes?] [compile fail?]  [activate fail?] [score
  delta neg?]
    status=VETOED      reconciler      REJECTED w/       REJECTED w/     3x neg
  → STALLED
                       picks up        COMPILE_FAILED     ACTIVATE_FAILED  →
  reset trust

  Interaction Edge Cases

    INTERACTION                      | EDGE CASE                    | HANDLED? |
   HOW?
    ---------------------------------|------------------------------|----------|
  --------
    zombiectl agent proposals veto   | Veto after auto_apply_at     | ? ← GAP  |
                                     | (race: job applies while     |          |
                                     | operator types veto)         |          |
    --------------------------------|------------------------------|----------|-
  -------
    zombiectl agent proposals approve| Approve already-expired      | ? ← GAP  |
                                     | proposal (>7 days)           |          |
    --------------------------------|------------------------------|----------|-
  -------
    Two proposals generated in       | First approved, second still | ? ← GAP  |
    quick succession                 | pending — second's baseline  |          |
                                     | is now stale                 |          |
    --------------------------------|------------------------------|----------|-
  -------
    Profile manually changed while   | Proposal based on old config | ? ← GAP  |
    proposal is in VETO_WINDOW       | is now invalid               |          |
    --------------------------------|------------------------------|----------|-
  -------
    Agent has exactly 0 runs         | Scoring context query returns| OK       |
   Orientation
                                     | empty                        |          |
   block
    --------------------------------|------------------------------|----------|-
  -------
    Agent has 10,000 scored runs     | agent_run_scores table bloat | ? ← GAP  |
   Needs retention
    --------------------------------|------------------------------|----------|-
  -------
    Score weights change between     | Historical scores use old    | OK       |
   weight_snapshot
    runs                             | weights                      |          |
   field (M9_002)

  Issue 11: Proposal Stale-Config Race

  The most dangerous interaction edge case: An operator manually changes the
  harness profile (via PUT /v1/workspaces/{ws_id}/harness/source → compile →
  activate) while a proposal is in VETO_WINDOW. After 24h, the background job
  applies the proposal's proposed_changes against the OLD config — overwriting
  the operator's manual change.

  This is a silent data loss scenario. The operator's manual change is
  overwritten without warning.

  Fix: Before applying any proposal, compare the harness profile's
  profile_version_id to the version that was current when the proposal was
  generated. If they differ, reject the proposal with reason
  CONFIG_CHANGED_SINCE_PROPOSAL and notify the operator.

  We recommend A: Profile version check before apply. This is a simple CAS
  (compare-and-set) guard — 3 lines of code, prevents silent data loss.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: Add a profile_version_id CAS check before applying any
     proposal. If the harness was changed since the proposal was generated,
     reject with CONFIG_CHANGED_SINCE_PROPOSAL. Prevents silent overwrite of
     operator's manual changes. 3 lines of code, prevents the worst M9 failure
     mode. Agree? → A) CAS check on profile_version_id (Recommended)

❯ You other questions prior to the M9 md files i recon asking you as the ceo to
 add that in the M9 and polish it up so we have a robust Gamification.

⏺ Understood — I'll incorporate all decisions made so far (and remaining ones)
  into the M9 spec files as a final deliverable after the review completes.
  Tracking all changes:

  Decisions to incorporate:
  1. Agent = Profile (agent_id → profile_id)
  2. In-worker synchronous scoring
  3. 3 real axes + stubbed resource (latency baseline from usage_ledger)
  4. 10 consecutive + failure classification exclusions for trust
  5. Fail-safe scoring (never blocks run)
  6. Remove system_prompt_appendix from proposable fields
  7. Numeric range constraints from entitlements
  8. CAS check on profile_version_id before proposal apply
  9. Stderr secret scrubbing in failure analysis
  10. Scoring hook via defer pattern (single call point, not inline at 5 exits)

  Let me continue the remaining review sections, then I'll polish all 4 specs +
  ARCHITECTURE.md.

  ---
  SECTION 5: Code Quality Review

  Module Structure

  The M9 implementation should follow the existing Zig source layout:

  src/
    scoring/
      engine.zig         — M9_001: score computation (deterministic, no DB)
      persistence.zig    — M9_002: agent_run_scores + agent_profiles writes
      analysis.zig       — M9_003: failure classification + analysis
      context.zig        — M9_003: ScoringContext builder + injection
      proposals.zig      — M9_004: proposal generation + validation
      trust.zig          — M9_004: trust evaluation
      types.zig          — shared types: Tier, FailureClass, Score, etc.
    http/handlers/
      agents/
        scores.zig       — GET /v1/agents/{id}/scores
        profile.zig      — GET /v1/agents/{id}/profile
        leaderboard.zig  — GET /v1/workspaces/{id}/leaderboard
        proposals.zig    — GET/POST proposal endpoints

  This is ~10 new files. Per HOLD SCOPE complexity check (from mode reference —
  though we're in EXPANSION, the complexity question for EXPANSION is "is it big
   enough?"): 10 new files for 4 workstreams across scoring, persistence,
  analysis, context injection, proposal generation, trust evaluation, API
  surface, and CLI surface is appropriate. Each file has a single
  responsibility.

  DRY Concerns

  Potential violation: The spec defines tier labels in two places — M9_001 dim
  1.4 (scoring) and M9_002 dim 1.2 (persistence). These must be a single enum in
   scoring/types.zig, not duplicated.

  Potential violation: Score persistence and failure analysis persistence follow
   the same pattern (write row, update aggregate, emit PostHog event). This
  should be a shared persistWithEvent helper, not copy-pasted.

  Naming

  The spec uses agent_id, agent_profiles, agent_run_scores — but we decided
  Agent = Profile. The naming should reflect this clearly:
  - Table: profile_run_scores (not agent_run_scores)
  - Table: profile_metrics (not agent_profiles)
  - Endpoint: /v1/profiles/{profile_id}/scores (not
  /v1/agents/{agent_id}/scores)
  - CLI: zombiectl profile scores (not zombiectl agent scores)

  Or — keep the "agent" naming as user-facing terminology even though the
  implementation maps to profiles. This is a UX decision.

  Issue 12: The user-facing terminology matters. "Agent" is more evocative and
  product-friendly than "profile." Operators think in terms of agents, not
  profiles. But the implementation maps to profiles.

  We recommend A: Keep "agent" as user-facing terminology, map to profile_id
  internally. The API says "agent," the DB can say "profile_run_scores" or
  "agent_run_scores" — either works. The CLI says zombiectl agent. The mapping
  is documented once in the types file and is invisible to users.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: Keep 'agent' as user-facing terminology (API, CLI,
     PostHog events) because operators think in terms of agents, not profiles.
     Internally, agent_id maps to profile_id. DB tables can use either naming.
     The mapping is documented once. What naming convention? → Agent
     user-facing, agent_id all through (v1/agents/{id}. CLI: zombictl agent. DB:
      agent_run_scores. Internally and externally agent_id documented in
     types.zig. Isnt this already there like agent_profile_id if not make a note
      to rename all the profile_id, agent_profile_id to agent_id (UUID type in
     postgres)

⏺ Let me check the existing harness schema to understand the current profile_id
  naming.

⏺ Read 1 file (ctrl+o to expand)

⏺ The agent_profiles table already exists with profile_id as PK. Adding to the
  decisions list:

  Decision 11: Rename agent_profiles.profile_id → agent_id in a new migration.
  All new M9 tables use agent_id as FK to agent_profiles.agent_id. Existing
  references (agent_profile_versions.profile_id, etc.) updated in the same
  migration. This is feasible pre-launch (15 migrations, no production data).

  Continuing the review.

  ---
  SECTION 6: Test Review

  New Things M9 Introduces

    NEW UX FLOWS:
      1. zombiectl agent scores <agent-id>     — score history
      2. zombiectl agent profile <agent-id>     — tier, streak, trust
      3. zombiectl workspace leaderboard        — ranked agents
      4. zombiectl agent proposals <agent-id>   — list proposals
      5. zombiectl agent proposals approve/reject/veto <id>
      6. zombiectl agent improvement-report <agent-id>
      7. zombiectl agent harness revert <agent-id> --to-change <id>

    NEW DATA FLOWS:
      1. Run terminal → scoreRun() → persist score → update profile aggregate
      2. Run terminal → analyzeRun() → persist analysis
      3. Run start → load last 5 scores → build ScoringContext → inject
      4. Score persist → check rolling avg → trigger proposal generation
      5. Proposal → VETO_WINDOW/PENDING_REVIEW → apply/reject → harness change

    NEW CODEPATHS:
      1. Scoring engine: 4-axis computation with weighted normalization
      2. Tier assignment: boundary-value bucketing (0-39, 40-69, 70-89, 90-100)
      3. Latency baseline computation: rolling p50/p95 from usage_ledger
      4. Failure classification: rule-based taxonomy (8 classes)
      5. ScoringContext builder: token-limited serialization with truncation
      6. Trust evaluation: consecutive Gold+ with failure classification
  exclusions
      7. Proposal validation: schema check + range constraints + CAS version
  check
      8. Background job: auto-apply proposals past veto window

    NEW BACKGROUND JOBS:
      1. Auto-apply checker: polls VETO_WINDOW proposals past auto_apply_at
      2. Proposal expiry: marks PENDING_REVIEW proposals > 7 days as REJECTED

    NEW INTEGRATIONS / EXTERNAL CALLS:
      1. LLM call for proposal generation (M9_004 dim 1.4)

    NEW ERROR/RESCUE PATHS:
      1. All 11 gaps from Section 2 — fail-safe catch + log pattern

  Test Specs Per Item

  Item: scoreRun()
  Type: Unit
  Happy Path: Score = 85 for clean 3-stage run
  Failure Path: Score = 0 for missing usage data
  Edge Case: Boundary: score exactly 70 = Gold
  ────────────────────────────────────────
  Item: Tier assignment
  Type: Unit
  Happy Path: Gold for 75
  Failure Path: Bronze for 0
  Edge Case: Boundary: 39→Bronze, 40→Silver, 69→Silver, 70→Gold, 89→Gold,
    90→Elite
  ────────────────────────────────────────
  Item: Latency baseline
  Type: Unit
  Happy Path: p50 = 45s from 20 runs
  Failure Path: No prior runs → neutral
  Edge Case: Single prior run → p50 = that run
  ────────────────────────────────────────
  Item: Failure classification
  Type: Unit
  Happy Path: TIMEOUT for timed_out=true
  Failure Path: Multiple signals → primary class
  Edge Case: Unknown error → UNKNOWN class
  ────────────────────────────────────────
  Item: ScoringContext
  Type: Unit
  Happy Path: 5 scores serialize to < 512 tokens
  Failure Path: 0 scores → orientation block
  Edge Case: Exactly 512 tokens → no truncation
  ────────────────────────────────────────
  Item: Trust evaluation
  Type: Unit
  Happy Path: 10 Gold → TRUSTED
  Failure Path: 9 Gold + 1 Silver → UNEARNED
  Edge Case: 10 Gold + 1 TIMEOUT (infra) → still TRUSTED
  ────────────────────────────────────────
  Item: Proposal validation
  Type: Unit
  Happy Path: Valid JSON, fields in range
  Failure Path: max_tokens > entitlement limit → reject
  Edge Case: tool_allowlist expands → reject
  ────────────────────────────────────────
  Item: CAS version check
  Type: Integration
  Happy Path: Version matches → apply
  Failure Path: Version changed → REJECTED
  Edge Case: Version null (first profile) → edge
  ────────────────────────────────────────
  Item: Auto-apply job
  Type: Integration
  Happy Path: 24h passes → APPLIED
  Failure Path: Job crashes → reconciler picks up
  Edge Case: Multiple proposals simultaneously
  ────────────────────────────────────────
  Item: API endpoints
  Type: Integration
  Happy Path: 200 with correct JSON
  Failure Path: Cross-workspace → 403
  Edge Case: Empty scores → 200 with []
  ────────────────────────────────────────
  Item: Fail-safe scoring
  Type: Integration
  Happy Path: DB write fails → run completes
  Failure Path: All scoring errors → run completes
  Edge Case: PostHog emission fails → silent

  Test Ambition Check

  Ship at 2am Friday: The fail-safe test. scoreRun() throws on every possible
  error path, and the run still completes with PR opened. If this test passes,
  scoring can never take down production.

  Hostile QA engineer: Score a run, manually mutate the agent_run_scores row to
  change the score, then verify that agent_profiles aggregate is recomputed from
   actual scores, not cached values. Tests whether the aggregate is
  self-healing.

  Chaos test: Kill the worker process between scoring and persistence. Restart.
  Verify the run is visible without a score (not stuck). Verify a reconciler or
  next-run trigger doesn't produce a duplicate score.

  Test pyramid: Many unit tests for scoring formula and tier boundaries. Fewer
  integration tests for DB persistence and API. 1-2 E2E tests for the full flow
  (run → score → inject context → next run).

  Moving to Section 7.

  ---
  SECTION 7: Performance Review

  ┌────────────┬────────────────────────────────────────────────────────────┐
  │  Concern   │                         Assessment                         │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │            │ M9_003 context injection queries last 5 scores + 5         │
  │ N+1        │ analyses per run start. Must be 2 queries, not 10. Use     │
  │ queries    │ WHERE agent_id = $1 ORDER BY scored_at DESC LIMIT 5 with a │
  │            │  join or separate query.                                   │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │ Memory     │ ScoringContext block capped at 512 tokens (~2KB). Scoring  │
  │ usage      │ computation is 4 floats. Negligible.                       │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │            │ Spec includes (agent_id, scored_at DESC) and               │
  │ DB indexes │ (workspace_id, score DESC). Need to add (agent_id,         │
  │            │ created_at DESC) on agent_run_analysis for context         │
  │            │ injection queries.                                         │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │ Table      │ agent_run_scores and agent_run_analysis grow linearly with │
  │ growth     │  runs. At 1000 runs/day, that's 365K rows/year per table.  │
  │            │ Need retention policy or partition by month.               │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │            │ Leaderboard query is expensive (aggregate sort). Cache for │
  │ Caching    │  5 minutes in the API handler. Individual profile lookups  │
  │            │ are cheap (single row by PK).                              │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │ Background │ Auto-apply checker runs every minute, queries WHERE status │
  │  job       │  = 'VETO_WINDOW' AND auto_apply_at <= now(). With an index │
  │            │  on (status, auto_apply_at), this is a single index scan.  │
  ├────────────┼────────────────────────────────────────────────────────────┤
  │            │ Proposal generation is an LLM call. p99 latency could be   │
  │ LLM call   │ 30-60 seconds. Must not block the scoring path — it should │
  │            │  be async (fire after scoring completes).                  │
  └────────────┴────────────────────────────────────────────────────────────┘

  Issue 13: LLM Proposal Generation Must Be Async

  M9_004 dim 1.1 says proposals are triggered when score trajectory declines.
  But proposal generation involves an LLM call (dim 1.4), which takes 30-60
  seconds. If this runs synchronously in the scoring path, it adds 30-60 seconds
   to every run that triggers a proposal.

  The fix: Scoring + persistence + analysis run synchronously (fast,
  deterministic). Proposal trigger check runs synchronously (fast, just a
  comparison). If triggered, enqueue the proposal generation as a separate work
  item — either via Redis stream or a simple proposal_generation_queue table.

  We recommend A: Enqueue proposal generation as async work. The scoring path
  stays fast (< 50ms). Proposal generation happens out-of-band. This also means
  a worker crash during proposal generation doesn't affect the scoring or the
  run.

⏺ User answered Claude's questions:
  ⎿  · We recommend A: Enqueue proposal generation as async work. Scoring +
     persistence + analysis are synchronous (~50ms total). If the rolling avg
     check triggers a proposal, enqueue it as async work (Redis or DB queue).
     LLM call runs out-of-band, doesn't block the scoring path. Worker crash
     during proposal gen doesn't affect run or scores. How should proposal
     generation be triggered? → A) Async enqueue (Recommended)

⏺ Decision 12: Async proposal generation. Scoring path stays fast, LLM call runs
   out-of-band.

  ---
  SECTION 8: Observability & Debuggability

  Required Events (M9)

  Event: agent.run.scored
  Surface: PostHog
  Trigger: After score persist
  Fields: run_id, agent_id, workspace_id, score, tier, axis_scores
  ────────────────────────────────────────
  Event: agent.scoring.failed
  Surface: PostHog + log
  Trigger: Scoring fails (fail-safe)
  Fields: run_id, error_class, error_message
  ────────────────────────────────────────
  Event: agent.trust.earned
  Surface: PostHog
  Trigger: UNEARNED → TRUSTED
  Fields: agent_id, consecutive_count
  ────────────────────────────────────────
  Event: agent.trust.lost
  Surface: PostHog
  Trigger: TRUSTED → UNEARNED
  Fields: agent_id, run_id, reason
  ────────────────────────────────────────
  Event: agent.proposal.generated
  Surface: PostHog
  Trigger: Proposal created
  Fields: agent_id, proposal_id, trigger_reason
  ────────────────────────────────────────
  Event: agent.proposal.applied
  Surface: PostHog
  Trigger: Proposal applied
  Fields: agent_id, proposal_id, approval_mode, fields_changed
  ────────────────────────────────────────
  Event: agent.proposal.vetoed
  Surface: PostHog
  Trigger: Operator vetoes
  Fields: agent_id, proposal_id, reason
  ────────────────────────────────────────
  Event: agent.proposal.rejected
  Surface: PostHog
  Trigger: Rejected/expired
  Fields: agent_id, proposal_id, reason
  ────────────────────────────────────────
  Event: agent.harness.changed
  Surface: PostHog
  Trigger: Harness config mutated
  Fields: agent_id, proposal_id, old_values, new_values
  ────────────────────────────────────────
  Event: agent.improvement.stalled
  Surface: PostHog + log
  Trigger: 3 negative deltas
  Fields: agent_id, delta_history

  Logging

  Structured log lines at each scoring decision point:
  scoring score_computed run_id=<> agent_id=<> score=85 tier=Gold
  axes={completion:95,error:80,latency:70,resource:50}
  scoring persist_ok run_id=<> agent_id=<> score_id=<>
  scoring fail_safe run_id=<> error=pg.Error detail="connection timeout"
  analysis classified run_id=<> failure_class=TIMEOUT signals=[timed_out]
  trust evaluated agent_id=<> trust_level=TRUSTED consecutive=12
  proposal triggered agent_id=<> reason=DECLINING_SCORE rolling_avg=58
  prev_avg=72
  proposal validated agent_id=<> proposal_id=<> fields_changed=2
  rejected_fields=0
  proposal applied agent_id=<> proposal_id=<> applied_by=system:auto

  Metrics (Prometheus)

  agent_score_computed_total{tier}           — counter
  agent_score_value{agent_id}                — gauge (latest score)
  agent_scoring_failed_total                 — counter
  agent_trust_earned_total                   — counter
  agent_trust_lost_total                     — counter
  agent_proposals_generated_total{trigger}   — counter
  agent_proposals_applied_total{mode}        — counter
  agent_proposals_vetoed_total               — counter
  agent_improvement_stalled_total            — counter
  agent_scoring_duration_seconds             — histogram
  agent_proposal_generation_duration_seconds — histogram

  Debuggability

  If a bug is reported 3 weeks post-ship: "Agent X scored Bronze but the
  operator says the run was fine."

  Can reconstruct from logs? Yes — scoring score_computed log line has the axis
  scores. Check each axis. If latency axis scored low, check usage_ledger for
  that run's wall_seconds vs baseline. If baseline was computed from a period of
   unusually fast runs, the "normal" run looks slow by comparison. This is
  debuggable.

  EXPANSION Addition: Joy to Operate

  - zombiectl agent dashboard — ASCII table showing all agents in the workspace
  with sparkline trends, current tier, and any pending proposals. One command,
  full situational awareness.
  - Score drift alert — If workspace average score drops 15% week-over-week,
  emit agent.quality.drifting event. This is the "pager" for agent quality.

  No questions in this section — findings are clear. Moving to Section 9.

  ---
  SECTION 9: Deployment & Rollout

  Migration Safety

  M9 requires new schema migration(s):
  - New tables: agent_run_scores, agent_run_analysis,
  agent_improvement_proposals, harness_change_log, workspace baseline aggregates
  - ALTER existing: agent_profiles — add columns: current_tier, lifetime_runs,
  lifetime_score_avg, consecutive_gold_plus_runs, trust_level, last_scored_at
  - Rename: agent_profiles.profile_id → agent_id (with FK cascade updates)

  The rename is the riskiest migration. It touches
  agent_profile_versions.profile_id, workspace_active_profile,
  profile_compile_jobs.requested_profile_id, entitlement_policy_audit_snapshots,
   and Zig source code references.

  Recommendation: Do the rename in a separate migration (016) before the M9
  additions (017+). This way the rename is isolated, testable, and rollback-able
   independently.

  Feature Flag

  Scoring should be behind a workspace-level feature flag: enable_agent_scoring:
   bool (default: false initially, flipped to true after validation). This
  allows:
  1. Deploy M9 code without scoring being active
  2. Enable per-workspace for testing
  3. Kill switch if scoring causes issues

  The spec already has enable_score_context_injection (M9_003 dim 2.4). Add a
  parent flag enable_agent_scoring that gates M9_001+002. Context injection
  requires scoring to be enabled.

  Rollout Order

  1. Migration 016: Rename profile_id → agent_id (isolated)
  2. Migration 017: Add M9 tables (agent_run_scores, agent_run_analysis, etc.)
  3. Migration 018: Add columns to agent_profiles (tier, streak, trust)
  4. Deploy code with scoring behind feature flag (flag OFF)
  5. Enable flag for one test workspace, run 20 runs, verify scores
  6. Enable flag for all workspaces
  7. Deploy M9_003 context injection (depends on scoring data existing)
  8. Deploy M9_004 proposal generation (async, behind separate flag)

  Rollback Plan

  ┌──────────────────────────┬──────────────────────────────────┬───────────┐
  │         Scenario         │         Rollback Action          │   Time    │
  ├──────────────────────────┼──────────────────────────────────┼───────────┤
  │ Scoring bug              │ Disable feature flag             │ 30        │
  │                          │                                  │ seconds   │
  ├──────────────────────────┼──────────────────────────────────┼───────────┤
  │ Bad migration            │ migrate down 018; migrate down   │ 2 minutes │
  │                          │ 017                              │           │
  ├──────────────────────────┼──────────────────────────────────┼───────────┤
  │ Rename breaks FK         │ migrate down 016 restores        │ 2 minutes │
  │                          │ profile_id                       │           │
  ├──────────────────────────┼──────────────────────────────────┼───────────┤
  │ Proposal applies bad     │ zombiectl agent harness revert   │ 1 minute  │
  │ config                   │                                  │           │
  └──────────────────────────┴──────────────────────────────────┴───────────┘

  Reversibility: 4/5 — most actions are reversible via flag or migration
  rollback. The one-way door is proposal application, which is mitigated by the
  revert CLI.

  EXPANSION Addition: Deploy Infrastructure

  The background job for auto-apply checking needs a scheduler. Options:
  - A) Worker goroutine with a 60-second tick — simplest, runs in the existing
  worker process
  - B) Separate cron service — cleanest separation but adds operational overhead
  - C) Postgres-based job scheduler (pg_cron or equivalent) — zero new processes

  A is right for v1. The worker already has a main loop; adding a periodic check
   is trivial. If proposal volume grows, upgrade to B.

  No questions in this section. Moving to Section 10.

  ---
  SECTION 10: Long-Term Trajectory

  Technical Debt Introduced

  1. Stubbed resource axis — scores 50 until Firecracker lands. This is known
  debt with a clear resolution path.
  2. Agent = Profile mapping — If the product evolves to support multiple agents
   per profile or agents that span profiles, this mapping breaks. Low risk for
  v1.
  3. Single-workspace leaderboard — Cross-workspace analytics is explicitly
  deferred. If enterprise customers want org-level leaderboards, this needs
  rework.

  Path Dependency

  M9 makes future changes easier, not harder:
  - Value-based pricing becomes possible (score-gated billing)
  - Agent SLAs become measurable
  - Harness optimization becomes data-driven instead of intuition-driven

  Knowledge Concentration

  The scoring formula, trust model, and proposal generation logic need to be
  documented in a single place (not spread across 4 spec files). A
  docs/SCORING.md canonical doc — like ARCHITECTURE.md, CONFIGURATION.md,
  OBSERVABILITY.md — would be appropriate.

  Reversibility: 4/5

  Mostly additive. Feature flags provide kill switch. Revert CLI handles bad
  proposals. Only the schema rename (profile_id → agent_id) is moderately hard
  to reverse.

  EXPANSION: What Comes After M9?

  Phase 2 (M10?): Cross-agent learning. Agent A's harness improvements are
  suggested to Agent B in similar workspaces. Requires a similarity model —
  workspaces with similar repo structures, similar stage topologies.

  Phase 3: Agent marketplace. Top-performing harness configs are shared (opt-in)
   across the platform. "This workspace's harness scored Elite 95% of the time —
   try it?" This is the platform play.

  Platform Potential: M9's scoring engine is the foundation for every future
  quality/optimization feature. The leaderboard is a social feature. The trust
  system is a safety primitive. Combined, they make UseZombie's agents a managed
   service with quality guarantees, not just a dumb pipeline.

  ---
  REQUIRED OUTPUTS

  NOT in Scope

  ┌────────────────────────────────┬────────────────────────────────────────┐
  │              Item              │               Rationale                │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Global cross-workspace         │ Privacy concern; deferred to           │
  │ leaderboard                    │ enterprise tier                        │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ LLM-assisted qualitative       │ Adds non-determinism; deferred         │
  │ scoring                        │                                        │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Score appeals or corrections   │ Immutability is a feature              │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Per-workspace weight overrides │ Configurable weights deferred to       │
  │                                │ follow-on                              │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Agent display name management  │ Uses agent_id for now                  │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Score history charts/UI        │ Deferred to Mission Control (v3)       │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Multi-agent cooperative        │ Deferred to Phase 2                    │
  │ improvement                    │                                        │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ Veto window length             │ Fixed at 24h in v1                     │
  │ configuration                  │                                        │
  ├────────────────────────────────┼────────────────────────────────────────┤
  │ GPU passthrough for proposals  │ Out of scope entirely                  │
  └────────────────────────────────┴────────────────────────────────────────┘

  What Already Exists

  ┌─────────────────────┬─────────────────────────────────┬────────────────┐
  │     Sub-Problem     │          Existing Code          │    Reused?     │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Run terminal state  │ state/machine.zig transitions   │ YES — hook     │
  │ detection           │                                 │ scoring here   │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Per-stage           │ AgentResult, usage_ledger       │ YES — scoring  │
  │ token/duration data │                                 │ reads these    │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Harness CRUD        │                                 │ YES —          │
  │ (compile/activate)  │ Harness control plane APIs      │ proposals use  │
  │                     │                                 │ these          │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ PostHog event       │ posthog_events.zig              │ YES — add new  │
  │ emission            │                                 │ events         │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Profile             │                                 │ YES — CAS      │
  │ versioning/audit    │ profile_linkage_audit_artifacts │ check uses     │
  │                     │                                 │ version        │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Workspace           │                                 │ YES — range    │
  │ entitlements        │ entitlements.zig                │ constraints    │
  │                     │                                 │ for proposals  │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Retry/reliability   │                                 │ YES — for      │
  │ wrapper             │ reliable_call.zig               │ proposal LLM   │
  │                     │                                 │ call           │
  ├─────────────────────┼─────────────────────────────────┼────────────────┤
  │ Workspace-scoped    │ RLS tenant isolation (schema    │ YES — all M9   │
  │ queries             │ 007)                            │ queries scoped │
  └─────────────────────┴─────────────────────────────────┴────────────────┘

  Dream State Delta

    12-MONTH IDEAL                           THIS PLAN GETS US
    ────────────────────────────             ─────────────────────
    Self-optimizing agent fleet              ✅ Quality scoring + feedback loop
    Cost-per-quality as metric               ✅ Score axes include efficiency
    Agent SLAs (95% Gold+)                   ✅ Tier tracking enables SLAs
    Operator dashboard (SRE-style)           ⬜ CLI only (dashboard deferred to
  v3)
    Cross-agent learning                     ⬜ Single-agent only (Phase 2)
    Agent marketplace                        ⬜ Not started (Phase 3)
    Value-based pricing                      ✅ Score-gated billing possible
    Auditable improvement trail              ✅ Full harness_change_log

  M9 delivers ~60% of the 12-month vision for the scoring/improvement axis. The
  remaining 40% is UI, cross-agent learning, and marketplace — all of which
  build on M9's data model.

  Error & Rescue Registry

  (Full table from Section 2 — all 11 gaps resolved with fail-safe pattern)

  Failure Modes Registry

    CODEPATH              | FAILURE MODE           | RESCUED? | TEST? | USER
  SEES?      | LOGGED?
    ----------------------|------------------------|----------|-------|---------
  --------|--------
    scoreRun()            | DB write fails         | Y (safe) | Y     |
  Score=null      | Y
    scoreRun()            | Division by zero       | Y (guard)| Y     | Axis=50
          | Y
    persistScore()        | Duplicate INSERT       | Y (UPSRT)| Y     |
  Idempotent      | Y
    analyzeRun()          | Missing metadata       | Y (UNKN) | Y     | UNKNOWN
  class   | Y
    injectContext()       | DB query fails         | Y (safe) | Y     | No
  context      | Y
    injectContext()       | > 512 tokens           | Y (trunc)| Y     |
  Truncated       | N/A
    generateProposal()    | LLM timeout            | Y (async)| Y     | No
  proposal     | Y
    generateProposal()    | LLM malformed          | Y (reject)| Y    | No
  proposal     | Y
    generateProposal()    | Forbidden field        | Y (valid)| Y     | No
  proposal     | Y
    autoApply()           | Compile fails          | Y (rejct)| Y     | REJECTED
          | Y
    autoApply()           | CAS version mismatch   | Y (rejct)| Y     |
  CONFIG_CHANGED  | Y
    autoApply()           | Job crash              | Y (recon)| Y     |
  Stuck→reconciled| Y
    evaluateTrust()       | No profile row         | Y (guard)| Y     | UNEARNED
          | Y

  CRITICAL GAPS: 0 (all resolved)

  Diagrams Produced

  1. System architecture (Section 1) ✅
  2. Dependency graph before/after (Section 1) ✅
  3. Score computation data flow (Section 4) ✅
  4. Context injection data flow (Section 4) ✅
  5. Proposal lifecycle flow (Section 4) ✅
  6. Rollout sequence (Section 9) ✅

  Stale Diagram Audit

  The M9 ARCHITECTURE.md section adds a new Mermaid flowchart. The existing v1
  sequence diagram (lines 165-211) is NOT affected by M9 — it documents the run
  execution lifecycle, which M9 hooks into but doesn't change. No stale
  diagrams.

  ---
  Now let me apply all decisions to the spec files. Here's what I'm updating:

  All 4 M9 specs + ARCHITECTURE.md M9 section with:
  1. agent_id as universal identifier (maps to profile_id, rename noted)
  2. In-worker synchronous scoring
  3. 3 real axes + stubbed resource axis
  4. Latency baseline from usage_ledger
  5. 10 consecutive + failure classification exclusions
  6. Fail-safe scoring pattern
  7. Remove system_prompt_appendix from proposable fields
  8. Numeric range constraints from entitlements
  9. CAS check on profile_version_id before apply
  10. Stderr secret scrubbing
  11. Defer pattern for scoring hook
  12. Async proposal generation (enqueue, not inline)
  13. DB grants for new tables
  14. Retention policy note for score/analysis tables

⏺ Read 1 file (ctrl+o to expand)

⏺ Write(docs/spec/v1/M9_001_AGENT_RUN_QUALITY_SCORING.md)
  ⎿  Added 71 lines, removed 15 lines
      11
      12  ---
      13
      14 +## Entity Model
      15 +
      16 +`agent_id` is the universal identifier for agents throughout M9
         +. It maps to the existing `agent_profiles.profile_id` (UUID) fr
         +om schema 006. A migration renames `profile_id` → `agent_id` ac
         +ross all existing tables and references before M9 tables are cr
         +eated.
      17 +
      18 +User-facing terminology is "agent" everywhere: API paths (`/v1/
         +agents/{agent_id}/...`), CLI (`zombiectl agent ...`), PostHog e
         +vents, and DB table names.
      19 +
      20 +---
      21 +
      22  ## 1.0 Quality Dimensions
      23
      24  **Status:** PENDING
     ...
       27  Each dimension is independently measurable, idempotent, and re
           producible from run metadata alone.
       28
       29  **Dimensions:**
       22 -- 1.1 PENDING Define the four scoring axes: completion (did th
          -e run reach terminal state cleanly?), error rate (unhandled ex
          -ceptions or non-zero exits), latency percentile (p50/p95 vs wo
          -rkspace baseline), resource efficiency (CPU/memory vs declared
          - sandbox limits)
       23 -- 1.2 PENDING Assign stable weights per axis: completion 40%,
          -error rate 30%, latency 20%, resource efficiency 10% — weights
          - are config, not hardcoded
       24 -- 1.3 PENDING Produce a normalized integer score 0–100 per run
          -; score is deterministic given the same run metadata (no rando
          -mness, no LLM calls in scoring path)
       30 +- 1.1 PENDING Define three active scoring axes plus one stubbe
          +d axis:
       31 +  - **Completion (40%)** — did the run reach terminal state cl
          +eanly? `DONE` = 100, `BLOCKED` with retries exhausted = 30, `B
          +LOCKED` by stage graph = 10, any error propagation = 0
       32 +  - **Error rate (30%)** — ratio of stages with `exit_ok=true`
          + to total stages executed. All stages pass = 100, 1 failure ou
          +t of 3 = 67, etc.
       33 +  - **Latency (20%)** — total `wall_seconds` vs workspace roll
          +ing baseline (p50). At or below p50 = 100, linear degradation
          +up to 3x p50 = 0. If no baseline exists (< 5 prior runs), axis
          + scores 50 (neutral).
       34 +  - **Resource efficiency (10%)** — STUBBED at 50 until M4_008
          + (Firecracker sandbox) provides CPU/memory metrics. Score form
          +ula is versioned; historical scores preserved when this axis a
          +ctivates.
       35 +- 1.2 PENDING Assign stable weights per axis: completion 40%,
          +error rate 30%, latency 20%, resource efficiency 10% — weights
          + are config values stored in workspace settings, not hardcoded
          +; default weights ship with these values
       36 +- 1.3 PENDING Produce a normalized integer score 0–100 per run
          +: `score = clamp(0, 100, round(sum(axis_score * weight)))`. Sc
          +ore is deterministic given the same run metadata (no randomnes
          +s, no LLM calls in scoring path). Division-by-zero guards: if
          +baseline_count < 1, latency axis = 50; if no usage rows exist,
          + completion axis = 0 with warning logged.
       37  - 1.4 PENDING Assign a tier label from score: Unranked (no pri
           or runs), Bronze (0–39), Silver (40–69), Gold (70–89), Elite (
           90–100)
       38
       39  ---
       40
       29 -## 2.0 Scoring Event Emission
       41 +## 2.0 Latency Baseline
       42
       43  **Status:** PENDING
       44
       45 +Maintain a rolling workspace-level latency baseline computed f
          +rom `usage_ledger.agent_seconds` data.
       46 +
       47 +**Dimensions:**
       48 +- 2.1 PENDING Compute workspace rolling p50 and p95 from the l
          +ast 50 completed runs (or all runs if < 50 exist). Store as `w
          +orkspace_latency_baseline` row: `workspace_id`, `p50_seconds`,
          + `p95_seconds`, `sample_count`, `computed_at`.
       49 +- 2.2 PENDING Baseline is recomputed after every scored run. I
          +f sample_count < 5, all runs in that workspace score latency a
          +xis at 50 (neutral) until sufficient data accumulates.
       50 +- 2.3 PENDING Baseline is workspace-scoped (not agent-scoped)
          +because agents within a workspace share the same repo and infr
          +a. Agent-scoped baselines can be introduced when there's suffi
          +cient per-agent run volume.
       51 +
       52 +---
       53 +
       54 +## 3.0 Scoring Execution Model
       55 +
       56 +**Status:** PENDING
       57 +
       58 +Score computation is synchronous, in-worker, and fail-safe.
       59 +
       60 +**Dimensions:**
       61 +- 3.1 PENDING Scoring runs in `zombied worker` after `executeR
          +un()` reaches a terminal state (DONE, BLOCKED, NOTIFIED_BLOCKE
          +D). The scoring call is a single deferred function (`defer sco
          +reRunIfTerminal(...)`) that fires on every exit path — not inl
          +ine at each of the 5 exit points.
       62 +- 3.2 PENDING Scoring reads data already in memory from the ru
          +n context (`AgentResult` token counts, wall seconds, exit stat
          +us) plus one DB query for the workspace latency baseline. Tota
          +l overhead: < 50ms.
       63 +- 3.3 PENDING If `scoreRun()` or any downstream persistence fa
          +ils, the error is caught, logged with full context (`run_id`,
          +`workspace_id`, `agent_id`, error class), and an `agent.scorin
          +g.failed` PostHog event is emitted. The run continues finaliza
          +tion normally. **Scoring must NEVER block or fail a run.** Sco
          +re is `null` (absent) for that run; API handles null gracefull
          +y.
       64 +- 3.4 PENDING Feature flag: `enable_agent_scoring: bool` per w
          +orkspace (default: false at initial deploy, flipped to true af
          +ter validation). Context injection (M9_003) requires scoring t
          +o be enabled.
       65 +
       66 +---
       67 +
       68 +## 4.0 Scoring Event Emission
       69 +
       70 +**Status:** PENDING
       71 +
       72  Emit the score as a structured, observable event immediately a
           fter run finalization.
       73
       74  **Dimensions:**
       36 -- 2.1 PENDING Emit `agent.run.scored` event to PostHog with fi
          -elds: `run_id`, `agent_id`, `workspace_id`, `score`, `tier`, `
          -axis_scores` (map), `scored_at`
       37 -- 2.2 PENDING Emit same payload to internal outbox table for p
          -ersistence (reuse M5_004 adapter outbox pattern)
       38 -- 2.3 PENDING Score computation happens in zombied after run r
          -eaches terminal state; no score on in-flight runs
       39 -- 2.4 PENDING Score is immutable once emitted — no retroactive
          - rescoring; if weights change, future runs use new weights, hi
          -storical scores are preserved as-is
       75 +- 4.1 PENDING Emit `agent.run.scored` event to PostHog with fi
          +elds: `run_id`, `agent_id`, `workspace_id`, `score`, `tier`, `
          +axis_scores` (map of axis_name → int), `weight_snapshot` (map
          +of axis_name → float), `scored_at`
       76 +- 4.2 PENDING Emit same payload to internal persistence path f
          +or `agent_run_scores` table write (M9_002)
       77 +- 4.3 PENDING Score computation happens in zombied worker afte
          +r run reaches terminal state; no score on in-flight runs
       78 +- 4.4 PENDING Score is immutable once emitted — no retroactive
          + rescoring; if weights change, future runs use new weights, hi
          +storical scores are preserved as-is with their `weight_snapsho
          +t`
       79
       80  ---
       81
       43 -## 3.0 Acceptance Criteria
       82 +## 5.0 Prometheus Metrics
       83
       84  **Status:** PENDING
       85
       47 -- [ ] 3.1 Score 0–100 produced for every run that reaches term
          -inal state (success or failure)
       48 -- [ ] 3.2 `agent.run.scored` event visible in PostHog within 5
          - seconds of run finalization
       49 -- [ ] 3.3 Tier label correct for all boundary values (0, 39, 4
          -0, 69, 70, 89, 90, 100)
       50 -- [ ] 3.4 Score is identical when computed twice from the same
          - run metadata (determinism test)
       86 +**Dimensions:**
       87 +- 5.1 PENDING `agent_score_computed_total{tier}` — counter, in
          +cremented per scored run
       88 +- 5.2 PENDING `agent_score_value{agent_id}` — gauge, latest sc
          +ore
       89 +- 5.3 PENDING `agent_scoring_failed_total` — counter, incremen
          +ted on fail-safe catch
       90 +- 5.4 PENDING `agent_scoring_duration_seconds` — histogram, ti
          +me spent in scoreRun()
       91
       92  ---
       93
       54 -## 4.0 Out of Scope
       94 +## 6.0 Acceptance Criteria
       95
       96 +**Status:** PENDING
       97 +
       98 +- [ ] 6.1 Score 0–100 produced for every run that reaches term
          +inal state (success or failure) when `enable_agent_scoring` is
          + true
       99 +- [ ] 6.2 `agent.run.scored` event visible in PostHog within 5
          + seconds of run finalization
      100 +- [ ] 6.3 Tier label correct for all boundary values (0, 39, 4
          +0, 69, 70, 89, 90, 100)
      101 +- [ ] 6.4 Score is identical when computed twice from the same
          + run metadata (determinism test)
      102 +- [ ] 6.5 Scoring failure does not block run finalization (fai
          +l-safe test: mock DB failure in scoring path, verify run compl
          +etes with PR opened)
      103 +- [ ] 6.6 Latency axis scores 50 when workspace has < 5 prior
          +runs
      104 +- [ ] 6.7 Resource axis scores 50 (stubbed) for all runs until
          + M4_008 activates
      105 +
      106 +---
      107 +
      108 +## 7.0 Out of Scope
      109 +
      110  - In-flight run scoring (score only at terminal state)
       57 -- User-configurable per-workspace weight overrides (deferred t
          -o M9 follow-on)
      111 +- User-configurable per-workspace weight overrides (deferred t
          +o M9 follow-on; weights are config but use defaults for v1)
      112  - LLM-assisted qualitative scoring (output content analysis)
      113  - Score appeals or corrections
      114 +- Agent-scoped latency baselines (workspace-scoped for v1)
      115 +- Resource axis activation (depends on M4_008 Firecracker)

⏺ Write(docs/spec/v1/M9_002_AGENT_SCORE_PERSISTENCE_AND_API.md)
  ⎿  Added 80 lines, removed 13 lines
      11
      12  ---
      13
      14 +## 0.0 Pre-Requisite Migration: agent_id Rename
      15 +
      16 +**Status:** PENDING
      17 +
      18 +Before M9 tables are created, rename `agent_profiles.profile_id
         +` → `agent_id` across all existing tables and FK references. Th
         +is is a separate migration (016) isolated from M9 additions.
      19 +
      20 +**Tables affected:**
      21 +- `agent_profiles`: `profile_id` → `agent_id` (PK)
      22 +- `agent_profile_versions`: `profile_id` → `agent_id` (FK)
      23 +- `workspace_active_profile`: no change (references `profile_ve
         +rsion_id`, not `profile_id`)
      24 +- `profile_compile_jobs`: `requested_profile_id` → `requested_a
         +gent_id` (FK)
      25 +- `entitlement_policy_audit_snapshots`: `profile_version_id` un
         +changed (version, not agent)
      26 +- `profile_linkage_audit_artifacts`: `profile_version_id` uncha
         +nged
      27 +
      28 +**Source code affected:** All Zig files referencing `profile_id
         +` in harness control plane handlers, entitlements, and profile
         +resolver. Grep for `"profile_id"` in `src/` to find all locatio
         +ns.
      29 +
      30 +---
      31 +
      32  ## 1.0 Data Model
      33
      34  **Status:** PENDING
     ...
      36  Persist scores in the workspace data model using UUIDv7 keys (M
          8_001 contract).
      37
      38  **Dimensions:**
      21 -- 1.1 PENDING Add `agent_run_scores` table: `id` (uuidv7), `run
         -_id` (uuidv7 FK), `agent_id` (uuidv7 FK), `workspace_id` (uuidv
         -7 FK), `score` (int 0–100), `tier` (enum), `axis_scores` (jsonb
         -), `weight_snapshot` (jsonb — weights at time of scoring), `sco
         -red_at` (timestamptz)
      22 -- 1.2 PENDING Add `agent_profiles` table: `agent_id` (uuidv7 PK
         -), `workspace_id`, `current_tier`, `lifetime_runs`, `lifetime_s
         -core_avg`, `streak_days` (consecutive days with at least one Go
         -ld+ run), `last_scored_at` — updated by outbox consumer after e
         -ach score event
      23 -- 1.3 PENDING Migration is additive (new tables only); zero dow
         -ntime; no existing table mutations
      24 -- 1.4 PENDING Index on `(agent_id, scored_at DESC)` for traject
         -ory queries; index on `(workspace_id, score DESC)` for leaderbo
         -ard queries
      39 +- 1.1 PENDING Add `agent_run_scores` table:
      40 +  ```sql
      41 +  CREATE TABLE agent_run_scores (
      42 +      score_id         UUID PRIMARY KEY,
      43 +      run_id           UUID NOT NULL REFERENCES runs(run_id),
      44 +      agent_id         UUID NOT NULL REFERENCES agent_profiles(
         +agent_id),
      45 +      workspace_id     UUID NOT NULL REFERENCES workspaces(work
         +space_id),
      46 +      score            INTEGER NOT NULL CHECK (score >= 0 AND s
         +core <= 100),
      47 +      tier             TEXT NOT NULL CHECK (tier IN ('BRONZE',
         +'SILVER', 'GOLD', 'ELITE')),
      48 +      axis_scores      TEXT NOT NULL,   -- JSON: {"completion":
         +95,"error_rate":80,"latency":70,"resource":50}
      49 +      weight_snapshot  TEXT NOT NULL,   -- JSON: {"completion":
         +0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}
      50 +      scored_at        BIGINT NOT NULL,
      51 +      UNIQUE (run_id),
      52 +      CONSTRAINT ck_agent_run_scores_uuidv7 CHECK (substring(sc
         +ore_id::text from 15 for 1) = '7')
      53 +  );
      54 +  CREATE INDEX idx_agent_run_scores_agent ON agent_run_scores(a
         +gent_id, scored_at DESC);
      55 +  CREATE INDEX idx_agent_run_scores_workspace ON agent_run_scor
         +es(workspace_id, score DESC);
      56 +  ```
      57 +- 1.2 PENDING Add columns to existing `agent_profiles` table:
      58 +  ```sql
      59 +  ALTER TABLE agent_profiles
      60 +      ADD COLUMN current_tier TEXT DEFAULT 'UNRANKED',
      61 +      ADD COLUMN lifetime_runs INTEGER NOT NULL DEFAULT 0,
      62 +      ADD COLUMN lifetime_score_avg NUMERIC(5,2) NOT NULL DEFAU
         +LT 0.0,
      63 +      ADD COLUMN consecutive_gold_plus_runs INTEGER NOT NULL DE
         +FAULT 0,
      64 +      ADD COLUMN trust_level TEXT NOT NULL DEFAULT 'UNEARNED' C
         +HECK (trust_level IN ('UNEARNED', 'TRUSTED')),
      65 +      ADD COLUMN last_scored_at BIGINT;
      66 +  ```
      67 +  Updated by the scoring persistence path after each scored run
         +.
      68 +- 1.3 PENDING Add `workspace_latency_baseline` table:
      69 +  ```sql
      70 +  CREATE TABLE workspace_latency_baseline (
      71 +      workspace_id    UUID PRIMARY KEY REFERENCES workspaces(wo
         +rkspace_id),
      72 +      p50_seconds     BIGINT NOT NULL,
      73 +      p95_seconds     BIGINT NOT NULL,
      74 +      sample_count    INTEGER NOT NULL,
      75 +      computed_at     BIGINT NOT NULL
      76 +  );
      77 +  ```
      78 +- 1.4 PENDING Migration is additive (new tables + ALTER columns
         + only); zero downtime; no existing table mutations beyond the p
         +re-requisite rename
      79 +- 1.5 PENDING Indexes: `(agent_id, scored_at DESC)` for traject
         +ory queries; `(workspace_id, score DESC)` for leaderboard queri
         +es
      80 +- 1.6 PENDING DB grants:
      81 +  ```sql
      82 +  GRANT SELECT, INSERT, UPDATE ON agent_run_scores TO worker_ac
         +cessor;
      83 +  GRANT SELECT ON agent_run_scores TO api_accessor;
      84 +  GRANT SELECT, INSERT, UPDATE ON workspace_latency_baseline TO
         + worker_accessor;
      85 +  GRANT SELECT ON workspace_latency_baseline TO api_accessor;
      86 +  ```
      87 +  Worker writes scores; API reads them. Agent_profiles grants a
         +lready exist (schema 006).
      88 +- 1.7 PENDING Retention policy: `agent_run_scores` rows older t
         +han 365 days may be archived. Aggregate data on `agent_profiles
         +` is sufficient for long-term trend analysis. Retention job is
         +a follow-on, not M9 scope.
      89
      90  ---
      91
     ...
       96  Expose score data via the existing zombied HTTP API following
           current auth and error-code conventions.
       97
       98  **Dimensions:**
       35 -- 2.1 PENDING `GET /v1/agents/{agent_id}/scores?limit=50&curso
          -r=` — paginated run score history, newest first; response incl
          -udes `score`, `tier`, `axis_scores`, `scored_at` per entry
       36 -- 2.2 PENDING `GET /v1/agents/{agent_id}/profile` — returns `a
          -gent_profiles` row: current tier, lifetime stats, streak
       37 -- 2.3 PENDING `GET /v1/workspaces/{workspace_id}/leaderboard?l
          -imit=20` — top agents by `lifetime_score_avg` within workspace
          -; returns agent_id, display name, tier, avg score, streak
       38 -- 2.4 PENDING All three endpoints are read-only; require works
          -pace-scoped auth token; no cross-workspace data leakage
       99 +- 2.1 PENDING `GET /v1/agents/{agent_id}/scores?limit=50&curso
          +r=` — paginated run score history, newest first; response incl
          +udes `score_id`, `run_id`, `score`, `tier`, `axis_scores`, `we
          +ight_snapshot`, `scored_at` per entry. Returns `[]` if no scor
          +es exist.
      100 +- 2.2 PENDING `GET /v1/agents/{agent_id}/profile` — returns ag
          +ent_profiles row: `agent_id`, `name`, `current_tier`, `lifetim
          +e_runs`, `lifetime_score_avg`, `consecutive_gold_plus_runs`, `
          +trust_level`, `last_scored_at`. Returns 404 if agent_id not fo
          +und.
      101 +- 2.3 PENDING `GET /v1/workspaces/{workspace_id}/leaderboard?l
          +imit=20` — top agents by `lifetime_score_avg` within workspace
          +; returns `agent_id`, `name`, `current_tier`, `lifetime_score_
          +avg`, `consecutive_gold_plus_runs`. Cached for 5 minutes at th
          +e handler level.
      102 +- 2.4 PENDING All three endpoints are read-only; require works
          +pace-scoped auth token; workspace_id extracted from auth claim
          +s. Cross-workspace data leakage prevented by `WHERE workspace_
          +id = $auth_workspace_id` on every query.
      103
      104  ---
      105
     ...
      111
      112  **Dimensions:**
      113  - 3.1 PENDING `zombiectl agent scores <agent-id> [--limit 20]
           [--json]` — prints score history table or JSON
       50 -- 3.2 PENDING `zombiectl agent profile <agent-id>` — prints cu
          -rrent tier, streak, lifetime avg
       51 -- 3.3 PENDING `zombiectl workspace leaderboard [--json]` — pri
          -nts workspace leaderboard
      114 +- 3.2 PENDING `zombiectl agent profile <agent-id>` — prints cu
          +rrent tier, trust level, streak, lifetime avg
      115 +- 3.3 PENDING `zombiectl workspace leaderboard [--json]` — pri
          +nts workspace leaderboard table
      116
      117  ---
      118
     ...
      120
      121  **Status:** PENDING
      122
       59 -- [ ] 4.1 Score row written within 2 seconds of `agent.run.sco
          -red` event consumed from outbox
      123 +- [ ] 4.1 Score row written within 2 seconds of run reaching t
          +erminal state (synchronous path, not outbox)
      124  - [ ] 4.2 `agent_profiles` row reflects correct tier and strea
           k after 10 sequential scored runs
      125  - [ ] 4.3 Leaderboard returns correct ordering for a workspace
            with 5 agents and mixed scores
      126  - [ ] 4.4 Cross-workspace isolation: agent in workspace A neve
           r appears in workspace B leaderboard
      127  - [ ] 4.5 CLI commands return `--json` output parseable by `jq
           ` with no extra prose
      128 +- [ ] 4.6 `agent_profiles.consecutive_gold_plus_runs` correctl
          +y excludes infrastructure failures (TIMEOUT, OOM, CONTEXT_OVER
          +FLOW) from streak resets — only agent-attributable failures (B
          +AD_OUTPUT, low score) break the streak (depends on M9_003 fail
          +ure classification)
      129 +- [ ] 4.7 DB grants enforce worker-write / api-read separation
      130
      131  ---
      132
      133  ## 5.0 Out of Scope
      134
      135  - Global cross-workspace leaderboard (privacy concern, deferre
           d)
       70 -- Score history charts or UI visualization (deferred to websit
          -e milestone)
      136 +- Score history charts or UI visualization (deferred to Missio
          +n Control v3)
      137  - Score export / CSV download
       72 -- Agent display name management (uses agent_id for now)
      138 +- Agent display name management (uses agent_profiles.name for
          +now)
      139 +- Retention job for old score rows (follow-on)

⏺ Write(docs/spec/v1/M9_003_AGENT_FAILURE_ANALYSIS_AND_CONTEXT_INJECTION.md)
  ⎿  Added 72 lines, removed 12 lines
      19  Analysis must be deterministic, structured, and LLM-independent
           (LLM may enhance but is not required).
      20
      21  **Dimensions:**
      22 -- 1.1 PENDING Extract failure signals from run terminal state:
         -exit code, stderr tail (last 200 lines), timeout flag, resource
         - limit hit (OOM / CPU throttle), unhandled exception class if s
         -urfaced in run metadata
      23 -- 1.2 PENDING Classify each failure into a stable taxonomy: `TI
         -MEOUT`, `OOM`, `UNHANDLED_EXCEPTION`, `BAD_OUTPUT_FORMAT`, `TOO
         -L_CALL_FAILURE`, `CONTEXT_OVERFLOW`, `AUTH_FAILURE`, `UNKNOWN`
         -— classification is rule-based, not LLM-based
      24 -- 1.3 PENDING Produce `agent_run_analysis` record: `run_id`, `f
         -ailure_class` (enum or null for success), `failure_signals` (js
         -onb array), `improvement_hints` (jsonb — structured pointers, n
         -ot prose), `analyzed_at`
      25 -- 1.4 PENDING For successful runs, record `failure_class = null
         -` and `improvement_hints` focused on efficiency: where latency
         -or resource headroom can be recovered
      22 +- 1.1 PENDING Extract failure signals from run terminal state:
         +exit code per stage (`AgentResult.exit_ok`), timeout flag (run
         +exceeded `RUN_TIMEOUT_MS`), resource limit hit (OOM / CPU throt
         +tle — available only after M4_008 Firecracker), unhandled excep
         +tion class if surfaced in run metadata
      23 +- 1.2 PENDING Classify each failure into a stable taxonomy:
      24 +  - `TIMEOUT` — run exceeded deadline or individual stage timed
         + out
      25 +  - `OOM` — resource limit exceeded (available after M4_008; in
         +ferred from exit signals until then)
      26 +  - `UNHANDLED_EXCEPTION` — stage threw an error not caught by
         +the agent
      27 +  - `BAD_OUTPUT_FORMAT` — agent produced output that didn't mat
         +ch expected schema (e.g., no verdict in Warden output)
      28 +  - `TOOL_CALL_FAILURE` — tool invocation returned error (file
         +not found, shell timeout)
      29 +  - `CONTEXT_OVERFLOW` — agent exceeded token context window
      30 +  - `AUTH_FAILURE` — BYOK key invalid, GitHub token expired, or
         + similar auth error
      31 +  - `UNKNOWN` — failure signals present but no taxonomy match
      32
      33 +  Classification is rule-based, not LLM-based. Each class has a
         + priority; highest-priority matching class wins.
      34 +
      35 +  **Infrastructure vs agent-attributable classification:**
      36 +  - Infrastructure failures: `TIMEOUT`, `OOM`, `CONTEXT_OVERFLO
         +W`, `AUTH_FAILURE`
      37 +  - Agent-attributable failures: `BAD_OUTPUT_FORMAT`, `TOOL_CAL
         +L_FAILURE`, `UNHANDLED_EXCEPTION`
      38 +  - `UNKNOWN` treated as agent-attributable (conservative defau
         +lt)
      39 +
      40 +  This classification is used by M9_004 trust evaluation: infra
         +structure failures do NOT reset the consecutive Gold+ streak; o
         +nly agent-attributable failures reset it.
      41 +
      42 +- 1.3 PENDING Produce `agent_run_analysis` record:
      43 +  ```sql
      44 +  CREATE TABLE agent_run_analysis (
      45 +      analysis_id      UUID PRIMARY KEY,
      46 +      run_id           UUID NOT NULL REFERENCES runs(run_id),
      47 +      agent_id         UUID NOT NULL REFERENCES agent_profiles(
         +agent_id),
      48 +      workspace_id     UUID NOT NULL REFERENCES workspaces(work
         +space_id),
      49 +      failure_class    TEXT,  -- NULL for successful runs
      50 +      failure_is_infra BOOLEAN NOT NULL DEFAULT FALSE,
      51 +      failure_signals  TEXT NOT NULL DEFAULT '[]',  -- JSON arr
         +ay of signal strings
      52 +      improvement_hints TEXT NOT NULL DEFAULT '[]', -- JSON arr
         +ay of structured hints
      53 +      stderr_tail      TEXT,  -- last 200 lines, secrets scrubb
         +ed
      54 +      analyzed_at      BIGINT NOT NULL,
      55 +      UNIQUE (run_id),
      56 +      CONSTRAINT ck_agent_run_analysis_uuidv7 CHECK (substring(
         +analysis_id::text from 15 for 1) = '7')
      57 +  );
      58 +  CREATE INDEX idx_agent_run_analysis_agent ON agent_run_analys
         +is(agent_id, analyzed_at DESC);
      59 +  ```
      60 +  DB grants:
      61 +  ```sql
      62 +  GRANT SELECT, INSERT ON agent_run_analysis TO worker_accessor
         +;
      63 +  GRANT SELECT ON agent_run_analysis TO api_accessor;
      64 +  ```
      65 +
      66 +- 1.4 PENDING For successful runs, record `failure_class = NULL
         +`, `failure_is_infra = FALSE`, and `improvement_hints` focused
         +on efficiency: where latency or resource headroom can be recove
         +red (e.g., "stage 'implement' used 80% of total tokens — invest
         +igate prompt compression")
      67 +
      68 +- 1.5 PENDING **Stderr secret scrubbing:** Before persisting `s
         +tderr_tail`, apply a scrubbing pass that redacts patterns match
         +ing: `API_KEY=...`, `Bearer ...`, `DATABASE_URL=...`, `ENCRYPTI
         +ON_MASTER_KEY=...`, `-----BEGIN.*PRIVATE KEY-----`, and any env
         + var from CONFIGURATION.md's "Keys That Should Never Come From
         +CLI Flags" list. Replace matched values with `[REDACTED]`.
      69 +
      70  ---
      71
      72  ## 2.0 Context Injection Into Next Run
     ...
      76  Before a new run starts, inject the agent's recent score trajec
          tory and failure analysis into the run context so the agent can
           self-correct.
      77
      78  **Dimensions:**
      36 -- 2.1 PENDING Build a `ScoringContext` block from the last 5 sc
         -ored runs: per-run score, tier, failure_class, top improvement_
         -hint — capped at 512 tokens; truncate oldest first if over limi
         -t
      37 -- 2.2 PENDING Inject `ScoringContext` as a structured system-me
         -ssage prefix before the agent's primary instruction; format is
         -stable and versioned (schema version in block header)
      79 +- 2.1 PENDING Build a `ScoringContext` block from the last 5 sc
         +ored runs: per-run `score`, `tier`, `failure_class` (if any), t
         +op `improvement_hint` — capped at 512 tokens; truncate oldest r
         +un first if over limit. Token counting uses a simple byte-based
         + estimate (4 chars ≈ 1 token) for determinism.
      80 +- 2.2 PENDING Inject `ScoringContext` as a structured system-me
         +ssage prefix before the agent's primary instruction. Format is
         +stable and versioned:
      81 +  ```
      82 +  ## Agent Performance Context (v1)
      83 +  Your recent run history:
      84 +  | Run | Score | Tier | Issue |
      85 +  |-----|-------|------|-------|
      86 +  | 5 (latest) | 87 | Gold | — |
      87 +  | 4 | 62 | Silver | TIMEOUT |
      88 +  | 3 | 91 | Elite | — |
      89 +  | 2 | 78 | Gold | — |
      90 +  | 1 | 45 | Silver | BAD_OUTPUT_FORMAT |
      91 +  Trend: improving. Focus: avoid timeouts on large repos.
      92 +  ```
      93  - 2.3 PENDING If agent has no prior runs (Unranked), inject a b
          rief orientation block: "You have no prior score history. Aim f
          or clean terminal states, minimal resource use, and valid outpu
          t format."
      39 -- 2.4 PENDING Injection is opt-in per workspace via a workspace
         - setting `enable_score_context_injection: bool` (default: true)
         -; can be disabled without affecting scoring or persistence
      94 +- 2.4 PENDING Injection is opt-in per workspace via a workspace
         + setting `enable_score_context_injection: bool` (default: true)
         +; can be disabled without affecting scoring or persistence. Req
         +uires `enable_agent_scoring` (M9_001) to be true.
      95 +- 2.5 PENDING If the DB query for last 5 scores fails, inject t
         +he orientation block as fallback (not empty context). Log the e
         +rror. Run continues normally.
      96
      97  ---
      98
     ...
      101  **Status:** PENDING
      102
      103  - [ ] 3.1 `agent_run_analysis` row exists for every run with a
            score (no orphaned scores without analysis)
       48 -- [ ] 3.2 `TIMEOUT` failure correctly classified when run meta
          -data shows `timed_out: true`
       49 -- [ ] 3.3 `ScoringContext` block injected into run context whe
          -n workspace setting is enabled
       50 -- [ ] 3.4 `ScoringContext` block is absent when workspace sett
          -ing is disabled
       51 -- [ ] 3.5 Context block never exceeds 512 tokens (enforced by
          -truncation, not best-effort)
       52 -- [ ] 3.6 Agent with 3 prior TIMEOUT failures and injected con
          -text shows measurable reduction in timeout rate over next 10 r
          -uns (demo evidence required — not a unit test)
      104 +- [ ] 3.2 `TIMEOUT` failure correctly classified when run meta
          +data shows deadline exceeded
      105 +- [ ] 3.3 `failure_is_infra = true` for TIMEOUT, OOM, CONTEXT_
          +OVERFLOW, AUTH_FAILURE
      106 +- [ ] 3.4 `failure_is_infra = false` for BAD_OUTPUT_FORMAT, TO
          +OL_CALL_FAILURE, UNHANDLED_EXCEPTION, UNKNOWN
      107 +- [ ] 3.5 `ScoringContext` block injected into run context whe
          +n workspace setting is enabled
      108 +- [ ] 3.6 `ScoringContext` block is absent when workspace sett
          +ing is disabled
      109 +- [ ] 3.7 Context block never exceeds 512 tokens (enforced by
          +truncation, not best-effort)
      110 +- [ ] 3.8 Stderr tail does not contain any patterns matching t
          +he secret scrubbing list
      111 +- [ ] 3.9 Agent with 3 prior TIMEOUT failures and injected con
          +text shows measurable reduction in timeout rate over next 10 r
          +uns (demo evidence required — not a unit test)
      112
      113  ---
      114
     ...
      118  - Per-run user feedback override of failure classification
      119  - Context injection into non-agent (human-driven) runs
      120  - Injection format changes mid-workspace (versioning handles t
           his; no live migration)
      121 +- Full stderr capture (only last 200 lines, scrubbed)

⏺ Write(docs/spec/v1/M9_004_AGENT_HARNESS_AUTO_IMPROVEMENT.md)
  ⎿  Added 100 lines, removed 26 lines
      15
      16  **Status:** PENDING
      17
      18 -After sufficient score history accumulates (minimum 5 runs), th
         -e agent generates a structured
      19 -improvement proposal targeting its own harness configuration.
      18 +After sufficient score history accumulates (minimum 5 runs), th
         +e system can generate a structured
      19 +improvement proposal targeting the agent's harness configuratio
         +n.
      20
      21  **Dimensions:**
      22 -- 1.1 PENDING Trigger proposal generation when: agent has ≥5 sc
         -ored runs AND current 5-run rolling avg score < previous 5-run
         -rolling avg score (trajectory is declining) OR avg score < 60 f
         -or any 5-run window
      23 -- 1.2 PENDING Proposal is a structured document: `agent_improve
         -ment_proposals` record with `agent_id`, `proposal_id` (uuidv7),
         - `trigger_reason` (enum: `DECLINING_SCORE` | `SUSTAINED_LOW_SCO
         -RE`), `proposed_changes` (jsonb array of `{target_field, curren
         -t_value, proposed_value, rationale}`), `approval_mode` (enum: `
         -AUTO` | `MANUAL`), `status` (enum: `PENDING_REVIEW` | `VETO_WIN
         -DOW` | `APPROVED` | `REJECTED` | `APPLIED` | `VETOED`), `auto_a
         -pply_at` (timestamptz, null if MANUAL), `created_at`
      24 -- 1.3 PENDING `proposed_changes` targets harness-level fields o
         -nly: `max_tokens`, `timeout_seconds`, `tool_allowlist`, `system
         -_prompt_appendix` — no changes to auth, billing, or network con
         -fig
      25 -- 1.4 PENDING Proposal generation uses the agent's own LLM call
         - with a constrained prompt: inject last 10 run analyses + curre
         -nt harness config; output is validated against the `proposed_ch
         -anges` schema before persisting (reject malformed output, do no
         -t retry blindly)
      22 +- 1.1 PENDING Trigger proposal generation when: agent has >= 5
         +scored runs AND current 5-run rolling avg score < previous 5-ru
         +n rolling avg score (trajectory is declining) OR avg score < 60
         + for any 5-run window. Trigger check is synchronous (fast compa
         +rison after score persist). If triggered, **enqueue** proposal
         +generation as async work — do not generate inline.
      23 +- 1.2 PENDING Proposal is a structured document:
      24 +  ```sql
      25 +  CREATE TABLE agent_improvement_proposals (
      26 +      proposal_id          UUID PRIMARY KEY,
      27 +      agent_id             UUID NOT NULL REFERENCES agent_profi
         +les(agent_id),
      28 +      workspace_id         UUID NOT NULL REFERENCES workspaces(
         +workspace_id),
      29 +      trigger_reason       TEXT NOT NULL CHECK (trigger_reason
         +IN ('DECLINING_SCORE', 'SUSTAINED_LOW_SCORE')),
      30 +      proposed_changes     TEXT NOT NULL,  -- JSON array of cha
         +nge objects
      31 +      profile_version_id   UUID NOT NULL,  -- version at time o
         +f proposal (CAS guard)
      32 +      approval_mode        TEXT NOT NULL CHECK (approval_mode I
         +N ('AUTO', 'MANUAL')),
      33 +      status               TEXT NOT NULL DEFAULT 'PENDING_REVIE
         +W'
      34 +                           CHECK (status IN ('PENDING_REVIEW',
         +'VETO_WINDOW', 'APPROVED', 'REJECTED', 'APPLIED', 'VETOED', 'CO
         +NFIG_CHANGED')),
      35 +      rejection_reason     TEXT,
      36 +      auto_apply_at        BIGINT,  -- NULL if MANUAL
      37 +      applied_by           TEXT,    -- 'operator:<identity>' or
         + 'system:auto'
      38 +      created_at           BIGINT NOT NULL,
      39 +      updated_at           BIGINT NOT NULL,
      40 +      CONSTRAINT ck_proposals_uuidv7 CHECK (substring(proposal_
         +id::text from 15 for 1) = '7')
      41 +  );
      42 +  CREATE INDEX idx_proposals_agent ON agent_improvement_proposa
         +ls(agent_id, created_at DESC);
      43 +  CREATE INDEX idx_proposals_veto_window ON agent_improvement_p
         +roposals(status, auto_apply_at)
      44 +      WHERE status = 'VETO_WINDOW';
      45 +  ```
      46 +  DB grants:
      47 +  ```sql
      48 +  GRANT SELECT, INSERT, UPDATE ON agent_improvement_proposals T
         +O worker_accessor;
      49 +  GRANT SELECT, UPDATE ON agent_improvement_proposals TO api_ac
         +cessor;
      50 +  ```
      51 +- 1.3 PENDING `proposed_changes` targets numeric harness-level
         +fields only:
      52 +  - `max_tokens` — bounded between 1000 and workspace entitleme
         +nt max
      53 +  - `timeout_seconds` — bounded between 30 and `RUN_TIMEOUT_MS
         +/ 1000`
      54 +  - `tool_allowlist` — can only **restrict** (remove tools), ne
         +ver expand beyond current profile's allowed tools
      55
      56 +  **Explicitly excluded** from proposable fields (rejected at s
         +chema validation):
      57 +  - `system_prompt_appendix` — direct prompt injection vector;
         +removed to eliminate LLM-generated text in future system prompt
         +s
      58 +  - Any auth, billing, or network config field
      59 +  - Model selection or provider configuration
      60 +
      61 +  Each change object: `{"target_field": "max_tokens", "current_
         +value": 8000, "proposed_value": 4000, "rationale": "last 5 runs
         + averaged 2100 tokens; reducing cap saves cost"}`
      62 +
      63 +  Range validation: if `proposed_value` is outside the bounded
         +range for `target_field`, the entire proposal is rejected with
         +reason `VALUE_OUT_OF_RANGE`.
      64 +
      65 +- 1.4 PENDING Proposal generation uses an LLM call (async, enqu
         +eued as a work item). The prompt includes: last 10 run analyses
         + + current harness config + the bounded field constraints. Outp
         +ut is validated against the `proposed_changes` schema before pe
         +rsisting. Malformed output → reject without retry, log `agent.p
         +roposal.generation_failed`. Empty output or refusal → treat as
         +malformed.
      66 +
      67  ---
      68
      69  ## 2.0 Agent Trust Level
     ...
      74  Trust is computed, not granted — it cannot be manually assigned
          .
      75
      76  **Dimensions:**
      37 -- 2.1 PENDING Define `TRUSTED` threshold: agent has ≥10 consecu
         -tive scored runs all in Gold or Elite tier (score ≥70 each); tr
         -acked as `consecutive_gold_plus_runs` on `agent_profiles`
      38 -- 2.2 PENDING `TRUSTED` status is re-evaluated after every run;
         - a single Bronze or Silver result resets `consecutive_gold_plus
         -_runs` to 0 and drops the agent back to `MANUAL` approval mode
      77 +- 2.1 PENDING Define `TRUSTED` threshold: agent has >= 10 conse
         +cutive scored runs all in Gold or Elite tier (score >= 70 each)
         +; tracked as `consecutive_gold_plus_runs` on `agent_profiles`
      78 +- 2.2 PENDING Trust evaluation uses M9_003 failure classificati
         +on to distinguish infrastructure failures from agent-attributab
         +le failures:
      79 +  - **Infrastructure failures** (`failure_is_infra = true`: TIM
         +EOUT, OOM, CONTEXT_OVERFLOW, AUTH_FAILURE) do NOT reset `consec
         +utive_gold_plus_runs`. The run is excluded from the streak coun
         +t (neither increments nor resets).
      80 +  - **Agent-attributable failures** (`failure_is_infra = false`
         +: BAD_OUTPUT_FORMAT, TOOL_CALL_FAILURE, UNHANDLED_EXCEPTION, UN
         +KNOWN) with score < 70 reset `consecutive_gold_plus_runs` to 0.
      81 +  - **Successful runs scoring Gold+ (>= 70)** increment `consec
         +utive_gold_plus_runs` by 1.
      82 +  This ensures trust measures agent quality, not infrastructure
         + reliability.
      83  - 2.3 PENDING `agent_profiles` exposes `trust_level` (enum: `UN
          EARNED` | `TRUSTED`) and `consecutive_gold_plus_runs` (int); su
          rfaced in `zombiectl agent profile <agent-id>` output
      84  - 2.4 PENDING PostHog event `agent.trust.earned` emitted when a
          gent crosses from UNEARNED → TRUSTED; `agent.trust.lost` emitte
          d on reset — both include `agent_id`, `run_id`, `consecutive_co
          unt_at_event`
      85
     ...
       96  - 3.1 PENDING When a proposal is generated for a TRUSTED agent
           , set `approval_mode = AUTO` and `auto_apply_at = created_at +
            24h`; status transitions to `VETO_WINDOW` immediately
       97  - 3.2 PENDING `zombiectl agent proposals <agent-id>` lists VET
           O_WINDOW proposals prominently with a countdown: `"Auto-applie
           s in 18h 42m — zombiectl agent proposals veto <proposal-id> to
            cancel"`
       98  - 3.3 PENDING `zombiectl agent proposals veto <proposal-id> [-
           -reason "..."]` — operator cancels; status transitions to `VET
           OED`; agent is not penalized but reason is stored; next propos
           al on next trigger
       55 -- 3.4 PENDING A background job checks `auto_apply_at <= now()`
          - and transitions `VETO_WINDOW → APPLIED`; `applied_by` recorde
          -d as `system:auto`; same atomic harness-change path as manual
          -approval
       99 +- 3.4 PENDING A background checker (worker goroutine, 60-secon
          +d tick) queries `WHERE status = 'VETO_WINDOW' AND auto_apply_a
          +t <= now()` and transitions matching proposals through the app
          +ly path. `applied_by` recorded as `system:auto`.
      100 +- 3.5 PENDING **CAS guard before apply:** Before applying any
          +proposal (auto or manual), compare the harness profile's curre
          +nt `profile_version_id` to the proposal's `profile_version_id`
          + field. If they differ (operator changed the harness since the
          + proposal was generated), reject the proposal with status `CON
          +FIG_CHANGED` and reason `CONFIG_CHANGED_SINCE_PROPOSAL`. Notif
          +y operator via PostHog event. This prevents silent overwrite o
          +f manual harness changes.
      101 +- 3.6 PENDING **Reconciler for stuck proposals:** If a proposa
          +l is in `VETO_WINDOW` and `auto_apply_at` is more than 1 hour
          +past (indicating the background job missed it), the reconciler
          + picks it up on next tick. This handles worker crash recovery.
      102
      103  ---
      104
     ...
      109  Agents that have not earned TRUSTED status require explicit op
           erator action on every proposal.
      110
      111  **Dimensions:**
       66 -- 4.1 PENDING `zombiectl agent proposals <agent-id>` — list PE
          -NDING_REVIEW proposals with proposed changes and rationale
       67 -- 4.2 PENDING `zombiectl agent proposals approve <proposal-id>
          -` — operator approves; status transitions to APPROVED then imm
          -ediately APPLIED in the same transaction
      112 +- 4.1 PENDING `zombiectl agent proposals <agent-id>` — list PE
          +NDING_REVIEW proposals with proposed changes, rationale, and b
          +ounded ranges
      113 +- 4.2 PENDING `zombiectl agent proposals approve <proposal-id>
          +` — operator approves; CAS version check, then status transiti
          +ons to APPROVED → APPLIED in the same transaction
      114  - 4.3 PENDING `zombiectl agent proposals reject <proposal-id>
           [--reason "..."]` — operator rejects; status transitions to RE
           JECTED; reason stored
       69 -- 4.4 PENDING Proposals older than 7 days without a decision a
          -uto-expire to REJECTED with reason `EXPIRED`; agent generates
          -a new proposal on next trigger
      115 +- 4.4 PENDING Proposals older than 7 days without a decision a
          +uto-expire to REJECTED with reason `EXPIRED`; agent generates
          +a new proposal on next trigger. Expiry handled by the same bac
          +kground checker as auto-apply.
      116
      117  ---
      118
     ...
      123  Approved proposals (auto or manual) are applied atomically; ev
           ery change is versioned and reversible.
      124
      125  **Dimensions:**
       80 -- 5.1 PENDING On APPROVED or auto-apply, apply `proposed_chang
          -es` to the agent's harness config atomically; create a `harnes
          -s_change_log` record: `agent_id`, `proposal_id`, `field`, `old
          -_value`, `new_value`, `applied_at`, `applied_by` (`operator:<i
          -dentity>` or `system:auto`)
       81 -- 5.2 PENDING Applied change is immediately reflected in the n
          -ext run (no restart required); no mid-run config mutation
       82 -- 5.3 PENDING Revert path: `zombiectl agent harness revert <ag
          -ent-id> --to-change <change-id>` restores the harness to pre-c
          -hange state; creates a new `harness_change_log` entry with `re
          -verted_from` reference; revert does not affect trust level
       83 -- 5.4 PENDING PostHog event `agent.harness.changed` emitted on
          - apply with fields: `agent_id`, `proposal_id`, `approval_mode`
          -, `fields_changed` (array), `trigger_reason`, `score_before_av
          -g`, `score_after_avg` (populated after 5 post-change runs)
      126 +- 5.1 PENDING On APPROVED or auto-apply: apply `proposed_chang
          +es` through the existing harness control plane path (compile →
          + activate). If compile fails, reject proposal with status `REJ
          +ECTED` and reason `COMPILE_FAILED`. If activate fails, reject
          +with `ACTIVATE_FAILED`. Create a `harness_change_log` record p
          +er field changed:
      127 +  ```sql
      128 +  CREATE TABLE harness_change_log (
      129 +      change_id       UUID PRIMARY KEY,
      130 +      agent_id        UUID NOT NULL REFERENCES agent_profiles(
          +agent_id),
      131 +      proposal_id     UUID NOT NULL REFERENCES agent_improveme
          +nt_proposals(proposal_id),
      132 +      workspace_id    UUID NOT NULL REFERENCES workspaces(work
          +space_id),
      133 +      field_name      TEXT NOT NULL,
      134 +      old_value       TEXT NOT NULL,
      135 +      new_value       TEXT NOT NULL,
      136 +      applied_at      BIGINT NOT NULL,
      137 +      applied_by      TEXT NOT NULL,  -- 'operator:<identity>'
          + or 'system:auto'
      138 +      reverted_from   UUID,  -- references change_id if this i
          +s a revert
      139 +      CONSTRAINT ck_harness_change_log_uuidv7 CHECK (substring
          +(change_id::text from 15 for 1) = '7')
      140 +  );
      141 +  CREATE INDEX idx_harness_change_log_agent ON harness_change_
          +log(agent_id, applied_at DESC);
      142 +  ```
      143 +  DB grants:
      144 +  ```sql
      145 +  GRANT SELECT, INSERT ON harness_change_log TO worker_accesso
          +r;
      146 +  GRANT SELECT, INSERT ON harness_change_log TO api_accessor;
      147 +  ```
      148 +- 5.2 PENDING Applied change is immediately reflected in the n
          +ext run's profile resolution (no restart required); harness co
          +ntrol plane already supports this via workspace_active_profile
          +. No mid-run config mutation.
      149 +- 5.3 PENDING Revert path: `zombiectl agent harness revert <ag
          +ent-id> --to-change <change-id>` restores the harness to pre-c
          +hange state via compile → activate with old_value; creates a n
          +ew `harness_change_log` entry with `reverted_from` reference;
          +revert does not affect trust level
      150 +- 5.4 PENDING PostHog event `agent.harness.changed` emitted on
          + apply with fields: `agent_id`, `proposal_id`, `approval_mode`
          +, `fields_changed` (array), `trigger_reason`
      151
      152  ---
      153
     ...
      158  Measure whether applied proposals actually improve the agent's
            score.
      159
      160  **Dimensions:**
       94 -- 6.1 PENDING After each applied change, tag the next 5 runs a
          -s `post_change_window: true` in `agent_run_scores`
       95 -- 6.2 PENDING Compute `score_delta`: avg score of post-change
          -window minus avg score of 5 runs before the change; store on `
          -harness_change_log`
      161 +- 6.1 PENDING After each applied change, tag the next 5 runs a
          +s `post_change_window: true` in `agent_run_scores` (add nullab
          +le `change_id` column referencing the proposal that triggered
          +the change window)
      162 +- 6.2 PENDING Compute `score_delta`: avg score of post-change
          +window minus avg score of 5 runs before the change; store on `
          +harness_change_log` as `score_delta` (nullable, populated afte
          +r window completes)
      163  - 6.3 PENDING `zombiectl agent improvement-report <agent-id>`
           — prints: trust level, proposals generated/approved/vetoed/rej
           ected/applied, avg score delta per applied change, current vs
           baseline tier
      164  - 6.4 PENDING If 3 consecutive applied proposals each produce
           negative `score_delta`, emit `agent.improvement.stalled` event
           , surface warning in CLI profile output, and reset trust level
            to UNEARNED regardless of consecutive_gold_plus_runs count
      165
     ...
      169
      170  **Status:** PENDING
      171
      105 -- [ ] 7.1 Proposal generated within one scored run of hitting
          -the trigger condition
      172 +- [ ] 7.1 Proposal generated (async, within 60 seconds) after
          +the scoring trigger condition is met
      173  - [ ] 7.2 Proposal with malformed `proposed_changes` is reject
           ed at ingest, not silently stored
      107 -- [ ] 7.3 Agent with 10 consecutive Gold+ runs shows `trust_le
          -vel: TRUSTED` in profile output
      108 -- [ ] 7.4 TRUSTED agent proposal enters VETO_WINDOW with corre
          -ct `auto_apply_at` timestamp
      109 -- [ ] 7.5 Operator veto within 24h prevents application; statu
          -s shows VETOED, harness unchanged
      110 -- [ ] 7.6 TRUSTED agent drops a Silver run → `consecutive_gold
          -_plus_runs` resets to 0 → next proposal requires manual approv
          -al
      111 -- [ ] 7.7 Revert restores previous value exactly; `applied_by`
          - on revert row shows operator identity
      112 -- [ ] 7.8 No harness change applied without a proposal record
          -in APPROVED/VETO_WINDOW state (enforced at DB constraint level
          -)
      113 -- [ ] 7.9 Demo evidence: agent earns TRUSTED, generates auto-a
          -pproved proposal, harness updates, score improves over next 5
          -runs
      174 +- [ ] 7.3 Proposal targeting `system_prompt_appendix` is rejec
          +ted at schema validation
      175 +- [ ] 7.4 Proposal with `max_tokens` exceeding entitlement lim
          +it is rejected with VALUE_OUT_OF_RANGE
      176 +- [ ] 7.5 Proposal with `tool_allowlist` that expands beyond c
          +urrent profile is rejected
      177 +- [ ] 7.6 Agent with 10 consecutive Gold+ runs (excluding infr
          +a failures) shows `trust_level: TRUSTED` in profile output
      178 +- [ ] 7.7 TRUSTED agent proposal enters VETO_WINDOW with corre
          +ct `auto_apply_at` timestamp
      179 +- [ ] 7.8 Operator veto within 24h prevents application; statu
          +s shows VETOED, harness unchanged
      180 +- [ ] 7.9 TRUSTED agent drops an agent-attributable Silver run
          + → `consecutive_gold_plus_runs` resets to 0 → next proposal re
          +quires manual approval
      181 +- [ ] 7.10 TRUSTED agent has a TIMEOUT (infra) run → `consecut
          +ive_gold_plus_runs` unchanged → trust preserved
      182 +- [ ] 7.11 CAS version check: proposal generated against profi
          +le_version_id X, operator changes profile, auto-apply attempt
          +rejects with CONFIG_CHANGED_SINCE_PROPOSAL
      183 +- [ ] 7.12 Revert restores previous value exactly; `applied_by
          +` on revert row shows operator identity
      184 +- [ ] 7.13 No harness change applied without a proposal record
          + in APPROVED/VETO_WINDOW state (enforced at application logic
          +level)
      185 +- [ ] 7.14 Demo evidence: agent earns TRUSTED, generates auto-
          +approved proposal, harness updates, score improves over next 5
          + runs
      186
      187  ---
      188
     ...
      190
      191  - Manual override to grant TRUSTED status without earning it t
           hrough run history
      192  - Changes to auth config, billing tier, or network policy via
           proposals
      193 +- `system_prompt_appendix` as a proposable field (prompt injec
          +tion risk)
      194  - Multi-agent cooperative improvement (one agent learning from
            another's harness)
      195  - LLM provider or model selection as a proposable change (defe
           rred)
      196  - Veto window length as a user-configurable setting (fixed at
           24h in v1)
      197 +- Synchronous proposal generation (always async/enqueued)

⏺ Now let me update the ARCHITECTURE.md M9 section to reflect all decisions.

⏺ Read 1 file (ctrl+o to expand)

⏺ Update(docs/ARCHITECTURE.md)
  ⎿  Added 43 lines, removed 13 lines
      358
      359  The M9 milestone closes a feedback loop where every completed
           run scores itself,
      360  injects its score history into the next run, and — when qualit
           y is declining —
      361 -proposes concrete harness changes for operator approval.
      361 +proposes harness changes. Agents that earn **TRUSTED** status
          +(10 consecutive Gold+
      362 +runs, excluding infrastructure failures) get a 24-hour veto wi
          +ndow instead of
      363 +waiting for manual approval.
      364
      365 +**Entity model:** `agent_id` maps to `agent_profiles.agent_id`
          + (renamed from `profile_id`).
      366 +A harness profile IS an agent. Scoring, trust, and proposals a
          +ll scope to the profile.
      367 +
      368  ```mermaid
      369  flowchart TD
      370      A([Agent Run Submitted]) --> B[zombied Worker\nExecutes St
           ages\nEcho → Scout → Warden]
      371      B --> C{Reaches Terminal\nState?}
      372      C -->|In flight| B
      373
      369 -    C -->|Yes| D["<b>M9_001 Scoring Engine</b>\nCompute axes:
          -completion · errors · latency · resources\nNormalize → integer
          - score 0–100\nAssign tier: Bronze / Silver / Gold / Elite"]
      374 +    C -->|Yes| D["M9_001 Scoring Engine\nCompute axes: complet
          +ion · errors · latency · resources\nNormalize 0-100 → tier: Br
          +onze · Silver · Gold · Elite"]
      375
      371 -    D --> E["<b>M9_002 Persist</b>\nagent_run_scores row writt
          -en\nagent_profiles tier + streak updated\nLeaderboard refreshe
          -d"]
      376 +    D --> E["M9_002 Persist\nagent_run_scores written\nagent_p
          +rofiles: tier + streak\nconsecutive_gold_plus_runs recalculate
          +d\nLeaderboard refreshed"]
      377
      373 -    D --> F["<b>M9_003 Failure Analysis</b>\nClassify: TIMEOUT
          - · OOM · BAD_OUTPUT\nUNHANDLED_EXCEPTION · CONTEXT_OVERFLOW\nP
          -roduce improvement_hints (structured)"]
      378 +    D --> F["M9_003 Failure Analysis\nClassify: TIMEOUT · OOM
          +· BAD_OUTPUT\nUNHANDLED_EXCEPTION · CONTEXT_OVERFLOW\nProduce
          +structured improvement_hints"]
      379
      380      F --> G["Build ScoringContext Block\nlast 5 scores + failu
           re classes\ncapped at 512 tokens"]
      381
      377 -    G -->|Prepended to system message| A
      382 +    G -->|Prepended to next run system message| A
      383
      379 -    E --> H{5-run rolling avg\ndeclining OR avg lt 60?}
      380 -    H -->|No — trajectory OK| I([Continue Running])
      384 +    E --> T{trust_level?}
      385
      382 -    H -->|Yes — trigger| J["<b>M9_004 Proposal Generation</b>\
          -nAgent LLM reviews last 10 analyses\n+ current harness config\
          -nOutputs structured proposed_changes\ntargets: timeout · token
          -s · tools · prompt"]
      386 +    T -->|UNEARNED\nconsecutive Gold+ lt 10| H
      387 +    T -->|TRUSTED\n10+ consecutive Gold+| H
      388
      384 -    J --> K[Operator Reviews\nzombiciectl agent proposals list
          -]
      389 +    H{5-run rolling avg\ndeclining OR avg lt 60?}
      390 +    H -->|No| I([Continue Running])
      391
      386 -    K -->|approve| L["Apply Harness Changes\nAtomically update
          - agent config\nharness_change_log written\nPostHog event emitt
          -ed"]
      387 -    K -->|reject / expire 7d| M([Proposal Archived\nNew propos
          -al on next trigger])
      392 +    H -->|Yes — trigger| J["M9_004 Generate Proposal\nLLM revi
          +ews last 10 run analyses\n+ current harness config\nvalidated
          +proposed_changes\ntargets: timeout · tokens · tools"]
      393
      389 -    L --> N["Tag next 5 runs\npost_change_window = true\nCompu
          -te score_delta after window"]
      394 +    J --> AP{approval_mode}
      395
      396 +    AP -->|UNEARNED\nMANUAL| K["PENDING_REVIEW\nOperator must
          +explicitly\napprove or reject\nExpires in 7 days"]
      397 +
      398 +    AP -->|TRUSTED\nAUTO| V["VETO_WINDOW\nauto_apply_at = now
          ++ 24h\nCLI shows countdown\nOperator can veto to cancel"]
      399 +
      400 +    K -->|approve| L
      401 +    K -->|reject / expire| M([Proposal Archived])
      402 +
      403 +    V -->|24h passes, no veto| L
      404 +    V -->|operator vetoes| M
      405 +
      406 +    L["Apply Harness Changes\nCAS check: profile_version_id mu
          +st match\nAtomic compile + activate\nharness_change_log writte
          +n\napplied_by: operator or system:auto\nPostHog agent.harness.
          +changed emitted"]
      407 +
      408 +    L --> N["Tag next 5 runs post_change_window\nCompute score
          +_delta\nIf 3 consecutive negative deltas\nemit agent.improveme
          +nt.stalled\nreset trust to UNEARNED"]
      409 +
      410      N -->|Feeds back into| A
      411
      412      style D fill:#1a3a5c,color:#fff,stroke:#2d6a9f
     ...
      415      style J fill:#1a3a5c,color:#fff,stroke:#2d6a9f
      416      style G fill:#2d4a1e,color:#fff,stroke:#4a7a2e
      417      style L fill:#2d4a1e,color:#fff,stroke:#4a7a2e
      418 +    style V fill:#5c3a1a,color:#fff,stroke:#9f6a2d
      419 +    style K fill:#3a1a1a,color:#fff,stroke:#9f2d2d
      420  ```
      421
      422  **Key invariants:**
      423  - Score is deterministic — identical result from same run meta
           data, no LLM in scoring path.
      424 +- Scoring is fail-safe — errors are caught, logged, and the ru
          +n continues normally. Score is null (absent) on failure.
      425 +- Scoring is in-worker, synchronous — single deferred call at
          +function exit, < 50ms overhead.
      426  - Context injection is bounded — 512 token hard cap, oldest ru
           ns truncated first.
      404 -- Harness changes require explicit operator `approve` — no aut
          -onomous mutation.
      427 +- Trust is earned, not granted — 10 consecutive Gold+ runs req
          +uired; only agent-attributable failures reset the streak (TIME
          +OUT, OOM, CONTEXT_OVERFLOW are excluded as infrastructure fail
          +ures).
      428 +- TRUSTED agents: proposals auto-apply after 24h veto window;
          +operator can cancel anytime.
      429 +- UNEARNED agents: every proposal requires explicit `zombiectl
          + agent proposals approve`.
      430 +- 3 consecutive negative score deltas resets trust to UNEARNED
          + regardless of run history.
      431 +- Proposals target numeric fields only (tokens, timeout, tool_
          +allowlist restrict-only). `system_prompt_appendix` is excluded
          + — no LLM-generated text in future system prompts.
      432  - Proposals targeting auth, billing, or network config are rej
           ected at schema validation.
      433 +- CAS guard: proposal stores `profile_version_id` at creation;
          + before apply, version must match current profile. If operator
          + changed the harness since proposal was generated, proposal is
          + rejected with `CONFIG_CHANGED_SINCE_PROPOSAL`.
      434 +- Proposal generation is async (enqueued, not inline) — LLM ca
          +ll runs out-of-band, does not block scoring path.
      435 +- Resource efficiency axis is stubbed at 50 until M4_008 (Fire
          +cracker) provides sandbox metrics.
      436
      437  **Workstream files:**
      438  - `docs/spec/v1/M9_001_AGENT_RUN_QUALITY_SCORING.md`

⏺ Good. All specs and architecture updated. Now let me produce the remaining
  required outputs.

  ---
  TODOS.md Updates

  The review identified several items that should be tracked. Let me present
  each one.

  Issue 14: Create TODOS.md with retention policy item

  The agent_run_scores and agent_run_analysis tables grow linearly with runs. At
   scale, this needs a retention job (archive rows > 365 days, keep aggregate
  data on agent_profiles).

⏺ User answered Claude's questions:
  ⎿  · TODO: Score/analysis table retention policy. Tables grow linearly with
     runs. At 1000 runs/day = 365K rows/year. Need a job to archive old rows
     (>365 days) while preserving aggregate data on agent_profiles. Effort: S.
     Priority: P3. Blocked by: M9_002 shipping first. Add to TODOS.md? → A) Add
     to TODOS.md (Recommended)

⏺ User answered Claude's questions:
  ⎿  · TODO: Agent-scoped latency baselines. Currently workspace-scoped (all
     agents in a workspace share one baseline). When per-agent run volume is
     sufficient (>50 runs per agent), agent-scoped baselines would give more
     accurate latency scoring. Effort: S. Priority: P2. Blocked by: M9_001
     shipping + sufficient run data. Add to TODOS.md? → A) Add to TODOS.md
     (Recommended)

⏺ User answered Claude's questions:
  ⎿  · TODO: Resource efficiency axis activation. Currently stubbed at 50. When
     M4_008 (Firecracker) ships, activate with real CPU/memory metrics from
     sandbox. Need to define the scoring formula (actual_usage / limit) and
     update scoring engine. Effort: M. Priority: P1 (after M4_008). Blocked by:
     M4_008. Add to TODOS.md? → A) Add to TODOS.md (Recommended)

⏺ Now let me handle the delight opportunities (EXPANSION mode).

  Delight Opportunity 1: Score badge in PR description

⏺ User answered Claude's questions:
  ⎿  · DELIGHT: Add agent score badge to PR description body. 'This PR was
     produced by a Gold-tier agent (87/100).' Social proof for reviewers, shows
     quality at a glance. 30 min effort — the PR creation already happens in
     worker_pr_flow.zig, just append a line. Add to TODOS.md or build now? → A)
     Add to TODOS.md as vision item (Recommended)

⏺ User answered Claude's questions:
  ⎿  · DELIGHT: ASCII sparkline in `zombiectl agent profile`. Show recent score
     trend as a sparkline: ▂▅▇▇▅▇▇▇ — instantly see quality trajectory without
     scrolling through scores. 30 min effort. Add to TODOS.md or build now? → A)
      Add to TODOS.md (Recommended)

⏺ User answered Claude's questions:
  ⎿  · DELIGHT: Score-gated billing credit. Runs scoring below Bronze (score <
     40) are automatically marked non-billable in the credit lifecycle.
     Operators never pay for garbage output. 1 hour effort (hook into
     billing.recordRuntimeStageUsage). Powerful trust signal. Add to TODOS.md or
      build now? → A) Add to TODOS.md

⏺ User answered Claude's questions:
  ⎿  · DELIGHT: Quality drift alert. If workspace average score drops 15%
     week-over-week, emit `agent.quality.drifting` PostHog event. This is the
     'pager' for agent quality. 30 min effort. Add to TODOS.md or build now? →
     A) Add to TODOS.md (Recommended)

⏺ User answered Claude's questions:
  ⎿  · DELIGHT: `zombiectl agent dashboard` — ASCII table showing all agents in
     workspace with sparklines, current tier, trust status, and pending
     proposals. One command, full situational awareness. 2 hour effort. Add to
     TODOS.md or build now? → A) Add to TODOS.md (Recommended)

⏺ Now let me create the TODOS.md with all approved items.

⏺ Write(TODOS.md)
  ⎿  Wrote 121 lines to TODOS.md
       1 # TODOS
       2
       3 Tracked deferred work items. Each has context, rationale, and pr
         iority so anyone
       4 picking it up in 3 months understands the motivation and where t
         o start.
       5
       6 ---
       7
       8 ## M9 Follow-On: Score/Analysis Table Retention Policy
       9
      10 **What:** Archive `agent_run_scores` and `agent_run_analysis` ro
         ws older than 365 days. Preserve aggregate data on `agent_profil
         es`.
     … +111 lines (ctrl+o to expand)

⏺ Now let me produce the final completion summary.

  ---
  COMPLETION SUMMARY

    +====================================================================+
    |            MEGA PLAN REVIEW — COMPLETION SUMMARY                   |
    +====================================================================+
    | Mode selected        | EXPANSION                                   |
    | System Audit         | Pre-launch codebase, 15 migrations,         |
    |                      | M4_008/M7_001/M6_006 open blockers,         |
    |                      | no existing M9 code or schema               |
    | Step 0               | EXPANSION + 4 key decisions resolved        |
    |                      | (scoring placement, sequencing, trust,       |
    |                      | entity model)                               |
    | Section 1  (Arch)    | 2 issues found (agent_id entity, scoring    |
    |                      | axes data gaps)                             |
    | Section 2  (Errors)  | 11 error paths mapped, 0 CRITICAL GAPS     |
    |                      | (all resolved with fail-safe pattern)       |
    | Section 3  (Security)| 7 issues found, 1 High severity             |
    |                      | (LLM prompt injection via proposals —       |
    |                      | mitigated by removing system_prompt_appendix)|
    | Section 4  (Data/UX) | 7 edge cases mapped, 1 critical             |
    |                      | (stale-config race — fixed with CAS guard)  |
    | Section 5  (Quality) | 2 issues found (naming convention,          |
    |                      | module structure)                            |
    | Section 6  (Tests)   | Diagram produced, 11 test specs,            |
    |                      | 3 ambition checks                           |
    | Section 7  (Perf)    | 1 issue found (async proposal generation)   |
    | Section 8  (Observ)  | 0 gaps (10 events, 4 metrics, structured    |
    |                      | logging defined)                            |
    | Section 9  (Deploy)  | 0 risks flagged (feature flag, rollout      |
    |                      | order, rollback plan defined)               |
    | Section 10 (Future)  | Reversibility: 4/5, debt items: 3           |
    +--------------------------------------------------------------------+
    | NOT in scope         | written (9 items)                           |
    | What already exists  | written (8 reuse points)                    |
    | Dream state delta    | written (60% of 12-month vision)            |
    | Error/rescue registry| 13 methods, 0 CRITICAL GAPS                 |
    | Failure modes        | 13 total, 0 CRITICAL GAPS                   |
    | TODOS.md updates     | 9 items created                             |
    | Delight opportunities| 5 identified, all tracked in TODOS.md       |
    | Diagrams produced    | 6 (system arch, dependency graph,           |
    |                      | 3 data flows, rollout sequence)             |
    | Stale diagrams found | 0                                           |
    | Unresolved decisions | 0                                           |
    +====================================================================+

  Key Decisions Made

  ┌─────┬─────────────────────┬─────────────────────────────────────────────┐
  │  #  │      Decision       │               Option Selected               │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 1   │ Review mode         │ EXPANSION                                   │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 2   │ Scoring placement   │ In-worker synchronous                       │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 3   │ M9 sequencing       │ Full M9 parallel to blockers                │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 4   │ Trust threshold     │ 10 consecutive + failure classification     │
  │     │                     │ exclusions                                  │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 5   │ Missing scoring     │ Latency baseline from usage_ledger + stub   │
  │     │ axes                │ resource at 50                              │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 6   │ Agent entity model  │ Agent = Profile (agent_id maps to           │
  │     │                     │ profile_id)                                 │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 7   │ Scoring failure     │ Fail-safe with structured logging           │
  │     │ handling            │                                             │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 8   │ LLM proposal        │ Remove system_prompt_appendix, constrain    │
  │     │ security            │ numerics                                    │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 9   │ Stale config race   │ CAS check on profile_version_id before      │
  │     │                     │ apply                                       │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 10  │ Naming convention   │ "Agent" user-facing, agent_id universal     │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 11  │ profile_id rename   │ Rename to agent_id in separate migration    │
  ├─────┼─────────────────────┼─────────────────────────────────────────────┤
  │ 12  │ Proposal generation │ Async enqueue, not inline                   │
  │     │  timing             │                                             │
  └─────┴─────────────────────┴─────────────────────────────────────────────┘

  Files Modified

  - docs/spec/v1/M9_001_AGENT_RUN_QUALITY_SCORING.md — fully rewritten with all
  decisions
  - docs/spec/v1/M9_002_AGENT_SCORE_PERSISTENCE_AND_API.md — fully rewritten
  with schema, grants, rename pre-req
  - docs/spec/v1/M9_003_AGENT_FAILURE_ANALYSIS_AND_CONTEXT_INJECTION.md — fully
  rewritten with failure taxonomy, infra classification, secret scrubbing
  - docs/spec/v1/M9_004_AGENT_HARNESS_AUTO_IMPROVEMENT.md — fully rewritten with
   trust model, CAS guard, async proposals, range constraints
  - docs/ARCHITECTURE.md — M9 section updated with all invariants and trust
  model
  - TODOS.md — created with 9 tracked items (3 follow-ons, 5 delight
  opportunities, 1 pre-requisite)
