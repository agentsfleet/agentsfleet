# M4_005: Harden Events, Observability (Langfuse), And Config Hygiene

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 005
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P1 — required for free plan metering
**Batch:** B3 — needs M4_007
**Depends on:** M4_007 (Define Runtime, Observability, And Config Contracts)

---

## 1.0 Singular Function

**Status:** PENDING

Implement one working hardening function for D4/D8/D19/D20 runtime concerns.

**Dimensions:**
- 1.1 PENDING Add durable event persistence/replay boundary
- 1.2 PENDING Add canonical trace context model
- 1.3 PENDING Add OTEL-friendly export path without Prometheus regression
- 1.4 PENDING Add key-versioned config/secret envelope and rotation verification
- 1.5 PENDING Integrate Langfuse as LLM/agent tracing backend (token cost, run traces, latency)

---

## 2.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Unit test: replay model survives restart without duplicate side effects
- 2.2 PENDING Unit test: trace fields are present across HTTP/worker paths
- 2.3 PENDING Unit test: key rotation path preserves decryptability during transition
- 2.4 PENDING Integration test: Langfuse traces are emitted for agent runs and contain token/cost metadata

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Deferred hardening dimensions are implemented and test-backed
- [ ] 3.2 Runtime observability and config hygiene stay deterministic under failure
- [ ] 3.3 Demo evidence captured for replay + trace + rotation checks

---

## 4.0 Out of Scope

- Full distributed tracing backend operations runbook
- Dashboard/UI observability features


I would like you to give me prompt to ask Claude Opus 4.6 to perform the following.

I want to make my observability stack documentation all the md files in ~/Projects/runbooks/docs/observability/ to be followable, installable by target audience of an infra team that installs the observability stack, technical teams that needs to know the architectural decisions made, technical teams (engineering implementation) that will implement the gRPC based implementation in their code to be integrated.

Prior starting the review, rhere is an existing ~/Projects/runbooks/docs/observability/ORACLE_REVIEW.md -- verify if all the earlier findings are complete.

When i say human, as mermaidjs sequence would help and less test. If its an agent, the may be create a toggle so its more md focussed? Need your suggestion here.

After that,
Oracle must review all the docs/observability/**.md stack to see if the documentation
- is structured for midway between humans and agents (I believe it is now almost)
structured mean
docs/observability/README.md(links and brief very brief - preferrably pointers)
docs/observability/ARCHITECTURE.md(overall)
docs/observability/API.md(overall)
docs/observability/DISTRIBUTED_CLICKHOUSE_SCHEMA.md(overall)
docs/observability/ORACLE_REVIEW_FINAL.md(your review now)
docs/observability/logging/USECASE.md(should this move to outside docs/observability/USECASE.md outside -- overall covering all three -- logging, metrics, tracing human+agent focussed?)
docs/observability/logging/NATS.md
docs/observability/logging/README.md(is this needed?)
docs/observability/logging/INSTALLATION.md(move to outside, to overall)
docs/observability/logging/VECTOR.md
docs/observability/logging/BILLING.md(should this be moved outside to docs/observability/ to cover overall)
docs/observability/logging/CAPACITY_PLANNING(should this be moved to outside docs/observability/CAPACITY_PLANNING to cover overall)
docs/observability/metrics/METRICS.md(what is in here? Is this usecase, if so merge it with docs/onbservability/USECASE.md)
docs/observability/tracing/TRACING.md(what is in here? Is this usecase of tracing or API, is so cant we merge with the relevant? )
Also do we need to bifurcate or create folders like docs/observability/logging, docs/observability/metrics... why not have it under docs/observability?


Oracle cut down duplicates, brutally honest review for an infra engineer to read through the architecture, agent can standup the stack by reading the INSTALLATION.md on an existing k8s clsuter, and a staff software engineer to consume gRPC endpoints for the usecases of logs listed in docs/observability/USECASE.md,docs/tracing/TRACING.md, docs/metrics/METRICS.md - logging, metrics, tracing?

Oracle must review all the @docs/observability/**.md and list them what they reviewed, any links and cross references must be complete.

Oracle must also point out if there is bloat of schema, meaning if the schema is inline(since it can change, its best to point to the url of this repo https://awakeninggit.e2enetworks.net/infra/e2e-logging-platform/ (local copy can be used for reference ~/Projects/e2e-logging-platform/)). The theory is to avoid drift.

docs/observability/README.md must have a very small heading with link for why clickhouse?

1. Why clickhouse for logging, metrics, tracing
- Comparison on VictorMetrics vs Signoz (link - https://workdrive.zoho.com/file/8crvk88bab0247e2a41a69a20ef7215216f73)
- Prometheus vs Signoz (https://workdrive.zoho.com/file/8crvkc904bdd335b24255b4bd51fa3cf5ba9f)
- OTel Collector vs Vector (https://workdrive.zoho.com/file/8crvkc8c87db8599749659181c0db2fb5f74c)

I want a mid way documentation for an agent humans, examples usecases or drawings must be there for a human but LLM will have text and sequence of statements that they can do with autonomy or prompt if some information isnt available.

I want the API.md to be agent friendly so it can be pointed to it and understood by a LLM on what grpc endpoint and how to consume the same in their application.

Preferrably the migrations, table schema will point to the  this repo https://awakeninggit.e2enetworks.net/infra/e2e-logging-platform/ (local copy can be used for reference if cloned, ~/Projects/e2e-logging-platform/))
The theory is to avoid drift.
