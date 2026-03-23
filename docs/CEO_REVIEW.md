# CEO Review Decisions

**Date:** Mar 22, 2026
**Status:** Active decision log for scope-shaping reviews that change product or platform direction

---

## M4_008 Sandbox Direction

### Decision

UseZombie will not keep the worker and NullClaw in the same runtime boundary for the long term.

v1.5 direction:
- Worker talks to a local `zombied-executor` over a typed executor API.
- The first implementation is a Unix-socket gRPC sidecar on the same host.
- `zombied-executor` embeds NullClaw and owns Linux enforcement: bubblewrap, Landlock, cgroup/systemd scope management, network policy, timeout teardown, and usage reporting.
- Worker owns orchestration, retries, billing, run state transitions, and artifact persistence.

v2 direction:
- Firecracker becomes another executor backend behind the same worker-facing API.
- The worker does not need a second orchestration model for host sandbox vs VM sandbox.

### Why this replaced the older v1 story

The older story said NullClaw ran directly on the worker host with a local shell wrapper. That kept too much blast radius inside the worker process:
- timeout and OOM failure handling were tied to the worker's own process state
- sandbox metrics were secondary rather than authoritative
- future migration to Firecracker would require another control-path rewrite

The executor boundary solves the right problem:
- isolate dangerous execution state
- keep the worker deterministic and restartable
- preserve one control-plane contract across host sandbox and Firecracker

### Explicit non-goals for this decision

- No attempt to preserve in-flight token generation across executor restart
- No claim that worker upgrade is transparent to a currently running agent
- No closed-loop harness auto-approval changes in the same branch

---

## Restart And Upgrade Contract

### Current contract

UseZombie is **stage-boundary durable**, not mid-token durable.

That means:
- persisted run state in Postgres is durable
- queued work in Redis is durable
- the live agent process inside `zombied-executor` is disposable

If `zombied-executor` dies mid-stage:
1. the worker treats the execution as failed or lost
2. the run is marked with a typed infrastructure failure
3. the worker or a restarted worker can reclaim and retry from the last persisted stage boundary

If the worker dies mid-stage:
1. the executor should lose its lease/parent health stream and cancel the run
2. the run is reclaimed after timeout/visibility rules
3. the new worker instance restarts the stage rather than resuming hidden process memory

### Operational meaning

- Agent continuity is **logical continuity**, not process continuity.
- Upgrading worker or executor during an active run will interrupt that run unless we later add drain mode.
- The correct operator expectation is "safe retry" rather than "live migration."

### Follow-on hardening

Future work may add:
- drain mode for upgrades (`worker` stops claiming new work, waits for active stages to finish)
- executor lease heartbeats
- resumable stage checkpoints for long-running agent sessions

Those are improvements, not current guarantees.

---

## Review Outcomes

### Accepted

- Expand scope beyond shell-wrapper-only sandboxing.
- Standardize on a worker-facing executor API.
- Make the local gRPC sidecar the first implementation.
- Keep closed-loop harness policy changes out of this branch.

### Deferred

- Proposal auto-approval redesign
- Mid-stage resume/checkpointing
- Firecracker implementation

