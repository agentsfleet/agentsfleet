# M18_001: CLI Progress Streaming and Failure Replay

**Prototype:** v1.0.0
**Milestone:** M18
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** DONE
**Priority:** P1 — No real-time visibility into running gates; post-mortem debugging requires manual log inspection
**Batch:** B4
**Depends on:** M16_001 (Gate Loop — core spec-to-PR loop must be green)

---

## 1.0 SSE Progress Streaming

**Status:** DONE

`zombied run <spec_path> --watch` opens an SSE connection to the API after enqueuing the run. The API reads worker events from Redis pub/sub (worker publishes gate results as they complete). The CLI renders formatted progress in real time: `[gate_name] outcome (loop N, Xms)`. An SSE heartbeat fires every 30s to prevent proxy and load-balancer timeout. On API restart or connection drop, the API replays missed events from the `gate_results` table using `Last-Event-ID` so no gate result is lost. Redis pub/sub uses `res.chunk()` for SSE delivery; falls back to DB polling if pub/sub fails.

**Dimensions:**
- 1.1 DONE Worker publishes gate completion events to Redis pub/sub channel `run:{run_id}:events` with gate name, outcome, and loop count
- 1.2 DONE API SSE endpoint `GET /v1/runs/{id}/stream` subscribes to Redis pub/sub channel and emits SSE events; sends heartbeat comment every 30s
- 1.3 DONE CLI `--watch` flag connects to SSE stream after enqueue and renders formatted gate progress output to stdout
- 1.4 DONE SSE reconnect: API replays missed gate events from `gate_results` table when `Last-Event-ID` header is provided

---

## 2.0 Failure Replay

**Status:** DONE

`zombied runs replay <id>` displays a structured narrative of what happened during a run. The data source is gate stdout/stderr persisted per loop iteration in the `gate_results` table. The output is formatted as a readable story: one section per gate, one entry per loop iteration, showing outcome and captured output. Replay works for both failed and successful runs.

**Dimensions:**
- 2.1 DONE Worker persists gate stdout/stderr and outcome per loop iteration in `gate_results` table after each gate completes (via existing `persistGateResults`)
- 2.2 DONE API endpoint `GET /v1/runs/{id}/replay` returns the structured gate results as a typed JSON response
- 2.3 DONE CLI `runs replay <id>` fetches replay data and formats gate results as a human-readable narrative (gate name, loop count, outcome, captured output)
- 2.4 DONE Replay is available for both failed and successful runs; successful runs show the clean path with loop counts

---

## 3.0 httpz SSE Feasibility Spike

**Status:** DONE

httpz supports streaming responses via `res.chunk(data)`, which sends chunked transfer-encoded data. SSE events are written as chunks in the standard `data: ...\n\n` format. Heartbeat is a comment chunk `: heartbeat\n\n`. `res.header("Content-Type", "text/event-stream")` must be set before the first chunk. No fallback to raw socket writers is needed — `res.chunk()` provides flush-on-write semantics and keeps the connection open for the lifetime of the handler function.

**Dimensions:**
- 3.1 DONE Spike: httpz `res.chunk()` provides chunked transfer encoding with flush-on-write semantics — confirmed adequate for SSE
- 3.2 DONE Native httpz SSE via `res.chunk()` is sufficient; no raw socket fallback needed
- 3.3 DONE Chosen approach: `res.header("Content-Type", "text/event-stream")` before first chunk; SSE events as `id: N\nevent: gate_result\ndata: {...}\n\n`; heartbeat as `: heartbeat\n\n`

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 `zombied run <spec_path> --watch` displays gate results in real time as the worker completes each gate
- [x] 4.2 SSE stream survives API restart: API replays missed events from `gate_results` when `Last-Event-ID` header is provided; falls back to DB polling if pub/sub fails
- [x] 4.3 `zombied runs replay <id>` renders a structured narrative for a failed run showing each gate's outcome and captured output
- [x] 4.4 Replay is available for successful runs as well as failed runs
- [x] 4.5 httpz SSE feasibility spike is complete and approach documented in section 3.0

---

## 5.0 Out of Scope

- WebSocket transport (SSE is sufficient for unidirectional server-to-client streaming)
- Dashboard or browser-based progress UI
- Email or webhook notifications for gate completion
- Log aggregation or external observability pipeline integration
- Real-time progress for runs triggered outside the CLI
