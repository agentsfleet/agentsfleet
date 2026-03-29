# UseZombie Use Cases

Date: Mar 28, 2026

## Shared Runtime Contract

All use cases below share the same execution model:

```text
User or agent submits work
  -> API persists run + enqueues work
  -> worker claims run
  -> worker resolves active harness/profile
  -> worker calls zombied-executor
  -> zombied-executor runs NullClaw inside sandbox policy
  -> worker persists artifacts, retries, or opens PR
```

Important operator expectation:
- work is durable at stage boundaries
- in-flight executor state is not durable across crash or upgrade
- active runs should be drained before worker/executor rollout if interruption is unacceptable
- Free plan admission is checked before run/sync/harness execution starts; completed runtime is billed after finalization only
- If free credit reaches `$0`, new execution attempts are rejected with an explicit credit exhaustion error and a Scale upgrade path
- **Self-repair gate loop:** after execution, the agent runs `make lint`, `make test`, and `make build`; if any gate fails, the agent attempts to fix the errors and retries up to `max_repair_loops`; runs that exhaust repair attempts without passing all gates are failed, not opened as PRs
- **Spec validation before execution:** malformed or structurally invalid specs are rejected at submission time before any worker claims the run, preventing token burn on specs that cannot produce a valid result
- **Run dedup:** submitting the same spec + repo + base commit combination returns the existing run instead of creating a duplicate; idempotent for both human and agent callers
- **Cost control:** each run carries a per-run token budget and a wall time limit; runs that exceed either limit are cancelled with a deterministic budget exhaustion error and the partial scorecard is persisted

## 1. Solo Builder

**Profile:** one developer with a few repos and a spec backlog.

### Workflow

1. Connect repo as a workspace.
2. Write a spec, or use `zombiectl spec init` to generate a template.
3. Submit via `zombiectl run --spec <file>`.
4. Agent implements the spec, then runs `make lint`, `make test`, and `make build` gate loop with self-repair.
5. On all gates passing: a PR is opened with an agent-generated explanation and scorecard.
6. Builder reviews one PR instead of babysitting agent sessions.

### Outcome

The builder writes specs and reviews PRs. Everything between is autonomous.

## 2. Small Team

**Profile:** a small engineering team using UseZombie as a backlog-to-PR pipeline.

### Workflow

1. Engineers add specs to the queue.
2. Each spec runs through the autonomous gate loop: implement, then `make lint` / `make test` / `make build` with self-repair.
3. PRs include a scorecard: gate results, repair loop count, wall time, and tokens consumed.
4. Team reviews scored PRs, not raw agent output.
5. Cost control prevents runaway token usage: per-run budgets are enforced before the run reaches the PR stage.

### Outcome

The team gets clearer operational boundaries: orchestration failures do not masquerade as agent-quality failures, and every PR arrives with evidence of what the agent actually did.

## 3. Agent-To-Agent

**Profile:** another agent writes specs and UseZombie turns them into validated PRs.

### Workflow

1. An external PM/planner agent writes a spec and optionally validates it via the API before submitting; malformed specs are rejected immediately without burning tokens.
2. It triggers a run through the API.
3. Run dedup is applied: if the same spec + repo + base commit was already submitted, the existing run ID is returned and no duplicate work is created.
4. Worker resolves the active harness and calls `zombied-executor`.
5. `zombied-executor` runs the requested stage topology.
6. Results, scorecard, and gate outcomes are persisted and exposed back through the control plane; the upstream agent can assess quality from the scorecard without re-running the code.

### Outcome

The upstream agent depends on one stable control-plane contract and does not need to know whether execution is host-sandboxed or later Firecracker-backed.

## 4. Rollout / Upgrade Use Case

**Profile:** operator upgrades `zombied worker` or `zombied-executor`.

### Safe rollout expectation

1. Stop claiming new work.
2. Let active stages finish, or accept that they will be retried.
3. Restart worker/executor.
4. Reclaim queued or interrupted runs from persisted state.

### Non-goal

UseZombie does not currently guarantee mid-stage live migration or mid-token session survival during upgrade.

## 5. Free Plan Exhaustion And Conversion

**Profile:** a solo builder runs on the Hobby plan until the included credit is consumed.

### Workflow

1. User creates a workspace and receives `$10` of free credit with no expiry.
2. `zombiectl run`, `workspace sync`, and operator harness mutations are admitted only if free-plan credit is still available.
3. Worker records runtime usage during execution, but free-plan credit is deducted only when the run reaches a completed billable state.
4. Failed or incomplete runs remain free.
5. Once remaining credit reaches `$0`, the API returns a deterministic credit exhaustion error and CLI output shows the Scale upgrade command.

### Outcome

The free tier stays abuse-resistant without pretending mid-run interruption exists. Operators see one explicit policy: admit before execution, bill on successful completion, then require a Scale upgrade for the next run.

## 6. Workspace Operator Controls

**Profile:** a workspace operator manages harnesses, skill secrets, and scoring controls without granting every workspace user mutation access.

### Workflow

1. Identity tokens carry a normalized role claim: `user`, `operator`, or `admin`.
2. Workspace-scoped mutation endpoints declare their minimum required role and optional credit policy through typed guard rules.
3. Operator-only surfaces reject non-operator tokens with deterministic `INSUFFICIENT_ROLE` responses.
4. Admin-only billing lifecycle actions stay reserved for admin tokens or admin API keys.
5. The RBAC contract is verified end-to-end with live HTTP tests across harness, skill-secret, and billing-event routes.

### Outcome

Workspace collaboration remains safe by default: normal users can consume the product, operators can manage workspace control-plane surfaces, and admin-only billing mutations stay fenced.

## 7. Scored Agent Selection (Phase 2)

**Profile:** a workspace operator runs multiple agent profiles competing on the same spec to select the best result by evidence.

### Workflow

1. Workspace operator defines two or three agent profiles as markdown files, each describing a different implementation strategy or model configuration.
2. When a spec is submitted, the runtime spawns each agent profile in an isolated worktree; profiles run concurrently.
3. Each agent is scored on completion: gate pass rate, repair loop count, wall time, tokens consumed, and diff quality.
4. The highest-scoring agent's branch is opened as the PR. Losing branches are abandoned without opening PRs.
5. Score history accumulates per agent profile over time. Profiles that consistently underperform can be retired by the operator.

### Outcome

The best agent for the job is selected by evidence, not by configuration preference. Score history makes agent quality legible and improvable over time.
