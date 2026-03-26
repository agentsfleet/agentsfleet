# UseZombie Use Cases

Date: Mar 22, 2026

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

## 1. Solo Builder

**Profile:** one developer with a few repos and a spec backlog.

### Workflow

1. Connect repo as a workspace.
2. Sync specs.
3. Trigger a run.
4. Worker delegates execution to `zombied-executor`.
5. On success, a PR is opened.
6. On executor crash or timeout, the run restarts from persisted stage state rather than trying to continue hidden process memory.

### Outcome

The builder reviews PRs instead of hand-driving a coding agent session.

## 2. Small Team

**Profile:** a small engineering team using UseZombie as a backlog-to-PR pipeline.

### Workflow

1. Engineers add specs to the queue.
2. Worker processes runs deterministically.
3. `zombied-executor` isolates dangerous agent execution from the worker.
4. Failures are classified clearly:
   - policy/sandbox failure
   - executor infrastructure failure
   - code/test validation failure

### Outcome

The team gets clearer operational boundaries: orchestration failures do not masquerade as agent-quality failures.

## 3. Agent-To-Agent

**Profile:** another agent writes specs and UseZombie turns them into validated PRs.

### Workflow

1. An external PM/planner agent writes a spec.
2. It triggers a run through the API.
3. Worker resolves the active harness and calls `zombied-executor`.
4. `zombied-executor` runs the requested stage topology.
5. Results are persisted and exposed back through the control plane.

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
