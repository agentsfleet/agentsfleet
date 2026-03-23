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

