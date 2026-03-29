# M18_001: CLI Progress Streaming and Failure Replay

**Prototype:** v1.0.0
**Milestone:** M18
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** PENDING
**Priority:** P1 — No real-time visibility into running gates; post-mortem debugging requires manual log inspection
**Batch:** B4
**Depends on:** M16_001 (Gate Loop — core spec-to-PR loop must be green)

---

## 1.0 SSE Progress Streaming

**Status:** PENDING

`zombiectl run --spec <file> --watch` opens an SSE connection to the API after enqueuing the run. The API reads worker events from Redis pub/sub (worker publishes gate results as they complete). The CLI renders formatted progress in real time: `[1/3] lint ... PASS (0 loops)`. An SSE heartbeat fires every 30s to prevent proxy and load-balancer timeout. On API restart or connection drop, the CLI reconnects with `Last-Event-ID` and the API replays missed events from the Postgres run record so no gate result is lost.

**Dimensions:**
- 1.1 PENDING Worker publishes gate completion events to Redis pub/sub channel `run:{run_id}:events` with gate name, outcome, and loop count
- 1.2 PENDING API SSE endpoint `GET /v1/runs/{id}/stream` subscribes to Redis pub/sub channel and emits SSE events; sends heartbeat comment every 30s
- 1.3 PENDING CLI `--watch` flag connects to SSE stream after enqueue and renders formatted gate progress output to stdout
- 1.4 PENDING SSE reconnect: CLI sends `Last-Event-ID` on reconnect; API replays missed gate events from `gate_results` JSONB on run row

---

## 2.0 Failure Replay

**Status:** PENDING

`zombiectl runs replay <id>` displays a structured narrative of what happened during a run. The data source is gate stdout/stderr persisted per loop iteration as a JSONB array on the run row (`gate_results` column). The output is not a raw log dump — it is formatted as a readable story: one section per gate, one entry per loop iteration, showing outcome and captured output. Replay works for both failed and successful runs.

**Dimensions:**
- 2.1 PENDING Worker persists gate stdout/stderr and outcome per loop iteration as JSONB array on run row (`gate_results` column) after each gate completes
- 2.2 PENDING API endpoint `GET /v1/runs/{id}/replay` returns the structured `gate_results` JSONB as a typed response
- 2.3 PENDING CLI `runs replay <id>` fetches replay data and formats gate results as a human-readable narrative (gate name, loop count, outcome, captured output)
- 2.4 PENDING Replay is available for both failed and successful runs; successful runs show the clean path with loop counts

---

## 3.0 httpz SSE Feasibility Spike

**Status:** PENDING

Before implementing section 1.0, verify that httpz supports streaming responses adequate for SSE. SSE requires the server to hold an HTTP connection open and flush newline-delimited event blocks incrementally. If httpz does not expose a streaming write API, a fallback using chunked transfer encoding over a raw socket writer must be evaluated. The outcome of this spike must be recorded in this spec (update section 1.0 if the implementation approach changes) before any SSE implementation begins.

**Dimensions:**
- 3.1 PENDING Spike: test httpz response streaming with `Transfer-Encoding: chunked` — verify flush-on-write semantics with a minimal SSE test handler
- 3.2 PENDING If httpz lacks native SSE support, implement a minimal SSE writer on top of the raw socket/response writer interface
- 3.3 PENDING Document the chosen SSE implementation approach in this spec (update section 1.0 with precise API calls used)

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `zombiectl run --spec <file> --watch` displays gate results in real time as the worker completes each gate
- [ ] 4.2 SSE stream survives API restart: CLI reconnects with `Last-Event-ID` and no gate event is duplicated or dropped
- [ ] 4.3 `zombiectl runs replay <id>` renders a structured narrative for a failed run showing each gate's outcome and captured output
- [ ] 4.4 Replay is available for successful runs as well as failed runs
- [ ] 4.5 httpz SSE feasibility spike is complete and approach is documented before 1.0 implementation begins

---

## 5.0 Out of Scope

- WebSocket transport (SSE is sufficient for unidirectional server-to-client streaming)
- Dashboard or browser-based progress UI
- Email or webhook notifications for gate completion
- Log aggregation or external observability pipeline integration
- Real-time progress for runs triggered outside the CLI
