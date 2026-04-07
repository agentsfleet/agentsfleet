# UseZombie — Website Content Map

Date: Mar 28, 2026
Status: SUPERSEDED — canonical source of truth is now `docs/v2/usezombie-v2.md`

Maps to: `Hero.tsx`, `FeatureSection.tsx`, `FeatureFlow.tsx`, `HowItWorks.tsx`, `Home.tsx`, `Agents.tsx`, `Pricing.tsx`

> This file describes the spec-to-PR product surface. For the current product strategy and
> pricing, see `docs/v2/usezombie-v2.md`. Website components should be updated to match the v2 doc.

---

## Hero

```yaml
badge: "For engineering teams · BYOK · No token markup"
line1: "Submit a spec."
line2: "Get a validated PR."
kicker: >
  UseZombie turns specs into validated pull requests with self-repairing
  agents, run quality scoring, and evidence-backed scorecards so teams
  ship with confidence, not babysitting.
cta_primary: "Connect GitHub, automate PRs"
cta_secondary: "View pricing"
```

---

## How It Works (3 steps)

```yaml
steps:
  - title: "Write a spec"
    description: >
      Describe what you want built in markdown. Use your own format,
      OpenSpec, gstack plans, or UseZombie's milestone specs. No CI
      pipeline to configure. No YAML. Just a spec file.
  - title: "Agents implement with self-repair"
    description: >
      An agent implements your spec, then runs lint, test, and build gates.
      If a gate fails, the agent reads the error and fixes it automatically,
      up to 3 repair loops.
  - title: "Review a scored PR"
    description: >
      A passing run opens a PR with an agent-generated explanation,
      a scorecard (gate results, repair loops, wall time, tokens consumed),
      and the full diff. Review one PR instead of babysitting ten agent sessions.
```

---

## Features (6 cards)

```yaml
features:
  - number: "01"
    title: "Spec-driven, not chat-driven"
    description: >
      Work starts from versioned markdown specs with deterministic run state.
      No prompt archaeology. No lost chat threads.

  - number: "02"
    title: "Self-repair gate loop"
    description: >
      Agents run make lint, make test, and make build, then fix their own
      errors before the PR lands. You review a passing build, not a broken draft.

  - number: "03"
    title: "Scored agent output"
    description: >
      Every run produces a quality scorecard: wall time, repair loop count,
      token consumption, test delta. See whether the agent struggled or sailed.

  - number: "04"
    title: "Evidence-backed PRs"
    description: >
      PRs include an agent-generated explanation of what changed and why,
      a scorecard comment with gate results, and the full diff. Review in
      30 seconds, not 10 minutes.

  - number: "05"
    title: "Cost control built in"
    description: >
      Per-run token budgets, wall time limits, and repair loop caps prevent
      runaway costs. Cancel runs mid-flight. One bad spec never becomes an
      infinite token burn.

  - number: "06"
    title: "Sandboxed execution"
    description: >
      Agent code runs inside Landlock filesystem isolation, cgroup memory
      and CPU limits, and network deny-by-default. Agents cannot exfiltrate
      data or escape their workspace.
```

---

## Spec Sources — Bring Your Own Spec Format

```yaml
headline: "Write specs your way. UseZombie runs them."
subline: >
  UseZombie is spec-format agnostic. Bring specs from your existing workflow,
  your planning agent, or an open standard. No CI pipeline to configure.
  No YAML to write. Just specs in, PRs out.

sources:
  - name: "Milestone Spec-Driven Development"
    description: >
      UseZombie's native format. Markdown specs organized as
      Milestones → Workstreams → Sections → Dimensions. Each spec
      is a self-contained unit of work with acceptance criteria
      and verification dimensions. Ship entire milestones in parallel
      with multiple agents.
    link: null
    example: "docs/spec/v1/M16_001_SPEC_TO_PR_GATE_LOOP.md"

  - name: "OpenSpec by Fission AI"
    description: >
      Open-source spec framework for AI-driven development. Markdown-based
      specs stored in Git with proposal, design, and task artifacts.
      Works with 20+ AI assistants. UseZombie consumes OpenSpec artifacts
      as run input — propose with OpenSpec, execute with UseZombie.
    link: "https://github.com/Fission-AI/OpenSpec"
    example: "opsx/my-feature/specs/feature.md"

  - name: "gstack /autoplan"
    description: >
      AI builder framework that produces structured plans through
      CEO review, eng review, and design review pipelines. Plans
      output implementation specs with scope decisions, architecture
      diagrams, and test requirements. Feed the plan output directly
      to UseZombie as a spec.
    link: "https://garryslist.org"
    example: "Plan from /office-hours → /plan-eng-review → zombiectl run"

  - name: "Any markdown file"
    description: >
      No framework required. A plain markdown file describing what
      you want built is a valid spec. UseZombie validates file
      references and flags ambiguities before the agent runs.
    link: null
    example: "docs/build-webhook-endpoint.md"

zero_config_message: >
  No GitHub Actions to configure. No CI YAML to write. No deploy
  pipeline to maintain. Submit a spec, UseZombie handles lint, test,
  build, self-repair, and PR creation inside a sandboxed environment.
  Your repo stays clean.
```

---

## Feature Flow (visual pipeline)

```yaml
flow:
  - label: "Spec"
    detail: "Markdown spec submitted via CLI or API"
  - label: "Validate"
    detail: "File references checked, ambiguities flagged"
  - label: "Agent"
    detail: "NullClaw agent implements in sandboxed worktree"
  - label: "Gates"
    detail: "lint → test → build with self-repair loops"
  - label: "Score"
    detail: "Quality scorecard: loops, time, tokens, tests"
  - label: "PR"
    detail: "Branch pushed, PR opened with explanation + scorecard"
```

---

## Target Users

```yaml
users:
  - name: "Solo builder"
    hook: "Submit a spec, get a PR, review instead of babysit."
    detail: >
      Agent handles the implementation loop and lint/test/build gates.
      You write specs and review PRs. Everything between is autonomous.

  - name: "Small team (2-5)"
    hook: "Convert your spec backlog into a PR pipeline."
    detail: >
      Scorecards make agent quality visible and comparable across runs.
      Cost control prevents runaway usage. Every PR arrives with evidence.

  - name: "Agent-to-agent"
    hook: "Your planner agent triggers runs via API."
    detail: >
      UseZombie is the execution layer. Upstream agent validates spec,
      submits run, gets scorecard back. One stable API contract.
```

---

## Agents Page (`Agents.tsx`)

```yaml
headline: "Agents are markdown files."
subline: >
  An agent profile is a markdown file with frontmatter config: model,
  repair strategy, gate settings. The agent's score follows it across runs.

agent_profiles:
  - name: "fast-shipper"
    strategy: "Speed-optimized. Minimal diff. One repair loop max."
    model: "claude-sonnet-4-6"

  - name: "test-heavy"
    strategy: "Maximizes test coverage. Adds edge case tests. Thorough."
    model: "claude-opus-4-6"

  - name: "safe-conservative"
    strategy: "Minimal changes. Defensive coding. Three repair loops."
    model: "claude-sonnet-4-6"

scoring_dimensions:
  - "Gate pass rate (lint, test, build)"
  - "Repair loop count (fewer = better)"
  - "Wall time (faster = better)"
  - "Token consumption (efficient = better)"
  - "Test delta (more tests added = better)"
  - "Diff size (smaller = better for same outcome)"

phase2_teaser: >
  Phase 2: multiple agents compete on the same spec in isolated worktrees.
  The highest-scoring agent's PR is opened. Losers' branches are abandoned.
  Score history accumulates. Poorly-performing agents get retired.
```

---

## Pricing Page (`Pricing.tsx`)

```yaml
plans:
  - name: "Hobby"
    price: "Free"
    credit: "$10 included credit"
    features:
      - "Unlimited specs"
      - "Self-repair gate loop"
      - "Scored PRs with evidence"
      - "1 workspace"
      - "Community support"

  - name: "Scale"
    price: "Pay as you go"
    features:
      - "Everything in Hobby"
      - "Unlimited workspaces"
      - "Priority execution queue"
      - "Cost control dashboards"
      - "Email support"
```

---

## Messaging Guardrails

**Do say:** deterministic, scored, self-repairing, spec-driven, evidence-backed, autonomous

**Do not say:**
- "AI writes your code" — UseZombie delivers validated PRs, not raw code
- "fully autonomous" — until auto-merge ships in v2, a human reviews before merge
- "agents keep running through upgrades"
- "single binary"

---

## v1 / v2 Product Story (internal reference)

### v1 (shipping)

- TypeScript CLI (`zombiectl`) — spec submission and run management
- Zig control plane (`zombied` API + worker + executor sidecar)
- Fly.io API, bare-metal workers connected via Tailscale
- Host-level Linux sandboxing (Landlock, cgroups, network deny)
- GitHub App automation (PR creation, branch push, scorecard comment)
- Self-repair gate loop (`make lint` / `make test` / `make build` with agent self-fix)
- Spec validation and run deduplication
- Cost control (token budgets, wall time limits, cancellation)

### v2 (next)

- Multi-agent competition with scored selection
- Score-gated auto-merge
- Progress streaming (SSE)
- Failure replay narratives
- Firecracker sandbox backend
