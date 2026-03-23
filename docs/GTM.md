# GTM: UseZombie — Agent Delivery Control Plane

Date: Mar 22, 2026
Status: Active positioning baseline

## Category and Positioning

1. Category: **Agent Delivery Control Plane**
2. Entry message: "Deterministic execution for autonomous engineering teams."
3. Promise: spec-driven pipelines, typed execution boundaries, replayable artifacts, and validated PRs.

## What Makes UseZombie Different

1. **Spec-driven, not chat-thread driven.** Work starts from versioned specs and deterministic run state.
2. **Control plane, not just an agent shell.** Worker orchestration, retry policy, audit artifacts, and PR creation are first-class.
3. **Typed execution boundary.** The worker does not directly host dangerous agent execution forever; it drives a `zombied-executor` that can evolve from host sandboxing to Firecracker without rewriting the control plane.
4. **Feedback as artifacts.** Validation failures and run outcomes are persisted, queryable, and reviewable.
5. **Honest restart semantics.** Runs recover from persisted stage state, not hidden in-memory chat state.

## Current Product Story

### v1

- CLI-first delivery
- local worker + local `zombied-executor`
- host-level Linux sandboxing
- GitHub App automation

### v2

- Firecracker backend behind the same executor contract
- stronger multi-tenant isolation

## Messaging Guardrails

Do say:
- deterministic
- policy-controlled
- restartable
- auditable
- execution-isolated

Do not say:
- "agents keep running through upgrades"
- "fully resumable sessions"
- "single binary" unless that becomes true again

