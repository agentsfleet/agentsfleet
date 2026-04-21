# Prompt: usezombie v1 → v2 Milestone Specs (TEMPLATE.md compliant)

You have read access to the `usezombie/usezombie` GitHub repo. That repo
is building v2. You also have access to the canonical spec template at
`docs/TEMPLATE.md`. Read the template in full before writing
anything. Every spec you produce MUST conform to it — file naming,
hierarchy (Prototype → Milestone → Workstream → Section → Dimension),
guardrails (≤4 workstreams per milestone, ≤4 dimensions per section),
prohibitions (no time estimates, no owners, no percent-complete), and
the mandatory Spec Template blocks (Overview, Files Changed, Applicable
Rules, Sections, Interfaces, Failure Modes, Implementation Constraints,
Invariants, Test Specification, Execution Plan, Acceptance Criteria,
Eval Commands, Dead Code Sweep, Verification Evidence, Out of Scope).

This is not a request for a design doc. It is a request for milestone
specs that an implementation agent can execute against, with machine-
checkable dimensions, executable eval commands, and dead-code sweeps.

---

## What v2 is

One-line pitch:

    An open-source agent runtime that lets developers ship agents
    which touch real infrastructure safely — starting with homelab
    and side-project devs, then production ops teams.

## v1 → v2 direction (the input to your milestone decomposition)

1. **Launch vertical shifts from lead-collector / sales zombies to
   Homelab Zombie.** A conversational diagnostic agent for self-hosted
   infrastructure. Audience: r/selfhosted, r/homelab, HN. Read-only
   only in v2.0 (kubectl, docker, ssh). Writes with approval gates in
   a later milestone.

2. **Runtime architecture stays.** zombied (Zig control plane) +
   zombie-worker (Zig execution agent in customer network tailscale like now) + nullclaw
   (bubblewrap + landlock sandbox) + zombiectl (JS/Bun CLI) + clawhub
   (skill registry). Do NOT propose swapping bubblewrap for libkrun in
   v2. That's v2.x+.

3. **Credential firewall stays and is load-bearing.** Vault stores
   credentials like now. Sandbox sees a placeholder. Do we need this? or use the firewall feature? Proxy at
   network boundary swaps placeholder for real credential only for
   allowlisted hosts. The real credential never enters the agent's
   LLM context. For Kubernetes: mTLS re-origination. For bearer-token
   APIs: HTTP-header injection. Acknowledge Microsandbox as prior art
   (same placeholder pattern); UseZombie adds skill registry + audit +
   approval layer + infra-specific policy.

4. **Agent UX pivots to conversational.** Primary interaction:

       $ zombie
       → Homelab zombie ready. What's up?
       > Jellyfin pods keep restarting
       [reasoning loop]
       > now check immich too
       [continues with context]

   One-shot: `zombie --once "..."`. If v1 has command-tree subcommands
   like `zombie diagnose "..."`, reshape to the REPL-style session.
or propose how we could do this on a conversational model.

5. **Skills are markdown + policy, not custom code.** A new read-only
   CLI skill should be a new frontmatter-annotated README.md on top of
   a shared shell-allowlist implementation. Zero net new worker code
   for typical skills.
This is already in place?

6. **Samples:** DELETE `samples/lead-collector/`. CREATE
   `samples/homelab/`, `samples/side-project-resurrector/` (stub in
   v2.0, implemented later), `samples/migration-zombie/` (stub),
   `samples/homebox-audit/` (stub), `samples/skills/kubectl-readonly/`,
   `samples/skills/docker-readonly/`, `samples/skills/ssh-readonly/`.

   I have already drafted canonical markdown for homelab, side-project-
   resurrector, migration-zombie, homebox-audit, kubectl-readonly,
   docker-readonly. They exist in a branch or attachment — find them
   before regenerating. If you cannot locate them, stop and ask.

7. **Docs:** Rewrite as part of the milestone `~/Projects/docs/quickstart.md` to the homelab-zombie walkthrough.
   Canonical version already drafted — find it, don't regenerate,

8. **Brand / naming:** Unchanged. usezombie. usezombie.com / usezombie.sh /
   docs.usezombie.com. GitHub org usezombie. Logo: broken loop arrow
   (Mark H). Flag any remnants of clawd / clawable / specfire.

---

## What I want you to produce

### Project 0: `docs/v1/inventory/CURRENT_STATE.md`

One pre-flight document. Not a spec. A structured inventory of v1 as
it exists now. For each top-level module (zombied, zombie-worker,
nullclaw/zombie-executor, zombiectl, clawhub, samples, docs, anything
else you find):

- What it is today (2-4 sentences, grounded in actual source you read)
- Key files and role, with paths
- State: solid / partial / stub / broken
- Dependencies on other modules
- v1 assumptions v2 breaks (e.g., "zombiectl has `zombie lead collect`
  that assumes lead-collector exists")

Be concrete. Cite paths. If something I described above doesn't exist
in v1, say so.

### PROJECT 1: Milestone plan `docs/v1/MILESTONES.md`

Before writing any individual spec, decompose the v1→v2 work into
milestones. Each milestone is a working prototype capability that can
be demoed end-to-end with evidence. Propose M1 through M{N} with:

- Milestone number and name
- One-line goal (testable, demoable)
- Which workstreams belong to it (≤4 per milestone; 5 only if a
  cross-cutting concern that would lose coherence if split)
- Dependency on earlier milestones
- Priority (P0 / P1 / P2)
- Batch assignment (B1, B2, B3 — workstreams in same batch can run
  concurrently; batches are sequential)

Target composition (suggested, adjust if the v1 code warrants it):

- M1: Homelab Zombie v2.0 shippable (conversational zombie, worker,
  credential firewall, kubectl-readonly, docker-readonly, audit log,
  kill switch)
- M2: Samples restructure + docs rewrite (delete lead-collector,
  add homelab + stub others, rewrite quickstart)
- M3: Landing page + blog launch surface
- M4: Approval gates + first write skills (post-launch)

If you propose a different decomposition, justify it in the MILESTONES.md
intro. Every milestone must respect the ≤4 workstream limit.

### Project 2: One spec file per workstream

For each workstream in your milestone plan, create a spec at:

    docs/v1/pending/M{N}_{WWW}_{DESCRIPTIVE_NAME}.md

Following the exact structure from `docs/TEMPLATE.md`. Mandatory
sections, in order:

1. Header block (Prototype, Milestone, Workstream, Date, Status,
   Priority, Batch, Branch, Depends on)
2. Overview (Goal — testable, Problem, Solution summary)
3. Files Changed (blast radius) — every CREATE / MODIFY / DELETE with
   real file paths
4. Applicable Rules — cite rule IDs from any RULES.md in the repo
   (if none, write "Standard set only")
5. Sections — each `### §N — {Title}` with Status, description, and
   a Dimensions table (≤4 dimensions per section, each with Dim ID,
   Status, Target file:fn, Input, Expected, Test type)
6. Interfaces — Public Functions (exact signatures in the right
   language; Zig for daemon/worker/sandbox, TypeScript for zombiectl),
   Input Contracts, Output Contracts, Error Contracts
7. Failure Modes — enumerated table + platform constraints
8. Implementation Constraints (Enforceable) — measurable, with verify
   command
9. Invariants (Hard Guardrails) — compile-time / lint-time enforcement
   only; no documentation-level invariants
10. Test Specification — Unit Tests, Integration Tests, Negative Tests,
    Edge Case Tests, Regression Tests (or N/A — greenfield),
    Leak Detection Tests (use std.testing.allocator for Zig),
    Spec-Claim Tracing
11. Execution Plan (Ordered) — numbered steps with verify commands;
    codebase MUST build and pass tests after every step
12. Acceptance Criteria — every criterion verifiable by command
13. Eval Commands (Post-Implementation Verification) — copy-pasteable
    block with E1..E10, including orphan sweep, leak check, build,
    tests, lint, cross-compile (zig build -Dtarget=x86_64-linux and
    aarch64-linux), gitleaks, 350-line gate, domain-specific lints
    (uncomment what applies)
14. Dead Code Sweep — mandatory if deleting/replacing files; three
    checks (file deletion, orphan references, main.zig test discovery)
15. Verification Evidence — empty table, filled in during VERIFY phase
16. Out of Scope

Respect the prohibitions: no time estimates, no effort/complexity
columns, no percentage complete, no assigned owners, no implementation
dates. Use Priority and Dependencies for sequencing only.


## Ground rules for your output

- **Read current v2 before writing.** Every Files Changed path must be real
  (or explicitly marked CREATE with justification).

- **Conform to TEMPLATE.md exactly.** Section headers, table formats,
  language conventions. If you deviate, note it explicitly in the spec
  intro and justify.

- **Dimensions are machine-readable test blueprints, not prose.** Wrong:
  "Test that the proxy swaps placeholders." Right: Target
  `zombie-worker/src/proxy.zig:swap Placeholder` — Input: `{placeholder:
  "<UZ_KUBE_TOKEN_01j9x7y>", host: "10.0.0.1:6443"}` — Expected: real
  bearer token injected in outbound Authorization header — Test type:
  integration.

- **Invariants must be compile-time.** If a human could violate it
  silently in a PR, it's not an invariant. Use comptime asserts in Zig,
  TypeScript satisfies/assert patterns, or lint rules.

- **Eval commands must be copy-pasteable and executable.** Not
  "run tests" — the actual shell block.

- **Call out conflicts.** If v1 reality contradicts v2 direction,
  surface the conflict and ask. Don't guess.

- **Honest pushback encouraged.** If a milestone can't fit in 4
  workstreams without breaking the hierarchy, propose two milestones
  instead. If a credential-firewall test is impossible to verify without
  real infrastructure, say so and propose a hermetic alternative.

- **No marketing prose.** Engineering specs.

---

## Language and toolchain conventions (non-negotiable)

- zombied, zombie-worker, nullclaw: Zig 0.15.x (confirm exact version
  from build.zig in v1). Cross-compile targets: x86_64-linux,
  aarch64-linux. Use std.testing.allocator for leak detection.
- zombiectl: TypeScript + Bun. No Node-only dependencies.
- Skills: Markdown with YAML frontmatter. No per-skill code for v2.0
  CLI-based skills.
- Docs site: Mintlify (docs.usezombie.com is already Mintlify-based —
  preserve that).
- License: Apache-2.0 for everything.

---

## Output order — two-stage with a checkpoint

This is a two-stage process. Do NOT produce all specs in one pass.

### Stage 1 — Pilot pass (stop and wait for my review)

Produce these, in order, and then STOP:

1. `docs/v2/inventory/CURRENT_STATE.md` (Project 0)
2. `docs/v2/MILESTONES.md` (Project 1)
3. Exactly ONE fully-written spec — the highest-priority P0 workstream
   from M1 (typically the credential firewall or the worker boot/auth
   path, whichever is the hardest-edges piece that will expose the most
   template-fit issues). All 16 mandatory sections complete.

After producing those three artifacts, STOP. Write a short handoff note
summarizing:

- Which milestones and workstreams you decomposed into, and why
- Which workstream you chose for the pilot spec and why
- Any ambiguities you hit that need my decision before Stage 2
- Any v1 code that contradicted my v2 direction (with file paths)
- Any parts of TEMPLATE.md you found unclear or that you had to interpret

I will review the pilot spec for template-fit, grounding in actual v1
code, and whether the Dimensions / Invariants / Eval Commands are
genuinely machine-checkable. I will either:

- Approve and say "continue with Stage 2", OR
- Give you template-fit corrections to apply to the pilot, then proceed

Do NOT start Stage 2 until I say so explicitly.

### Stage 2 — Full decomposition (after my approval)

1. Apply any corrections from my Stage 1 review to the pilot spec.
2. Produce every remaining workstream spec in priority order (P0 before
   P1 before P2, B1 before B2 before B3). One at a time. Complete each
   spec fully — all 16 sections — before starting the next.

Rationale: template-fit errors compound. Fixing them on twelve specs
is ten hours of rework; fixing them on one is thirty minutes. The
pilot-pass checkpoint is load-bearing for this prompt, not optional.

Within each stage, do not ask for confirmation at intermediate
boundaries unless there's a real ambiguity only I can resolve. The
ONLY mandatory stop is the end of Stage 1.
