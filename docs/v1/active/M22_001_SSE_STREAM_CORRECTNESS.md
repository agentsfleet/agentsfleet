# M22_001: SSE Stream Correctness Fixes

**Prototype:** v1.0.0
**Milestone:** M22
**Workstream:** 001
**Date:** Mar 30, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m22-001-sse-stream-correctness
**Priority:** P0 — SSE stream is behaviorally broken in four ways; real-time watching and reconnect are non-functional
**Batch:** B1
**Depends on:** M18_001 (SSE stream and --watch must exist before these fixes apply)

---

## Overview

M18_001 shipped the SSE stream endpoint and `--watch` CLI flag. Four correctness bugs were identified post-implementation that make the feature non-functional in production:

1. `--watch` uses a buffered HTTP fetch — the stream is not real-time
2. Heartbeat cannot fire while `readMessage()` is blocking — proxies will drop the connection
3. `Last-Event-ID` reconnect replay is broken — two incompatible ID namespaces cause duplicate floods
4. Race: run can complete between terminal-state check and pub/sub subscribe — stream blocks forever

All four must be fixed before M18_001 features can be considered production-ready.

---

## 1.0 CLI Streaming Fix (`--watch` real-time output)

**Status:** PENDING

`streamRunOutput` in `src/cmd/run.zig` uses `std.http.Client.fetch()` with `response_writer`, which buffers the entire response body before returning. For an SSE stream that holds the connection open until `run_complete`, this means all events are delivered in one batch after the run finishes — defeating the purpose of `--watch`.

The fix is to use `std.http.Client.open()` + `request.reader()` in an incremental read loop, parsing SSE lines as they arrive and printing each gate result immediately.

**Dimensions:**
- 1.1 PENDING Replace `client.fetch()` in `streamRunOutput` with `client.open()` + `req.send()` + `req.wait()` + `req.reader()` incremental read loop
- 1.2 PENDING Read line-by-line from `req.reader()`: accumulate `data:` and `event:` fields, dispatch on blank-line boundary — same parser logic as current `renderSseEvent`
- 1.3 PENDING Print each `gate_result` event immediately as it arrives: `[{gate_name}] {outcome} (loop {n}, {ms}ms)`
- 1.4 PENDING Exit the read loop and return when a `run_complete` event is received or when the server closes the connection

---

## 2.0 Heartbeat Socket Timeout

**Status:** PENDING

In `src/http/handlers/runs/stream.zig`, the heartbeat guard checks elapsed time, then immediately blocks on `subscriber.readMessage()` — a synchronous socket read with no timeout. If a gate takes longer than 30 seconds without emitting a pub/sub event, `readMessage()` never returns, the heartbeat check never executes again, and proxy/load-balancer idle timeouts drop the connection.

The fix is a read timeout on the pub/sub subscriber socket, set to 25 seconds — short enough to fire the heartbeat before the 30-second proxy threshold.

**Dimensions:**
- 2.1 PENDING Add `setReadTimeout(socket, timeout_ms)` call in `Subscriber.subscribe()` in `src/queue/redis_pubsub.zig` after the subscription confirmation is received; use `SO_RCVTIMEO` via `std.posix.setsockopt`
- 2.2 PENDING `Subscriber.readMessage()` returns `null` (not an error) on `error.WouldBlock` or `error.ConnectionTimedOut` — these indicate a timeout, not a fatal failure
- 2.3 PENDING Stream handler: when `readMessage()` returns `null`, check elapsed time and emit heartbeat if due, then continue the loop — no connection drop
- 2.4 PENDING Default read timeout constant: `pub const PUBSUB_READ_TIMEOUT_MS: u32 = 25_000;` in `redis_pubsub.zig`

---

## 3.0 Unified SSE Event ID Namespace

**Status:** PENDING

`streamStoredEvents` assigns `id: {created_at}` (Unix timestamp in ms, e.g. `1743000000000`). The live pub/sub loop assigns `id: {event_seq}` which resets to `0` on every new connection and increments by `1` per event.

When a client reconnects and sends `Last-Event-ID: 3`, the replay query executes `WHERE created_at > 3` — returning every row in the table because all real timestamps are large integers. This floods the client with all historical events as duplicates.

The fix is a single ID namespace: use `created_at` (Unix ms) for both stored replay and live events. Gate events published to Redis pub/sub must include `created_at` in their payload; the SSE `id:` line uses that value.

**Dimensions:**
- 3.1 PENDING Worker gate event payload (`worker_gate_loop.zig`) includes `"created_at":{unix_ms}` field alongside existing fields; use `std.time.milliTimestamp()` at publish time
- 3.2 PENDING Live SSE loop in `stream.zig`: extract `created_at` from parsed pub/sub event JSON; use it as the SSE `id:` value instead of `event_seq`
- 3.3 PENDING Reconnect path: parse `Last-Event-ID` header as `i64` Unix ms; pass to `streamStoredEvents` (parameter renamed from `after_created_at` to `after_event_id`) — replays only events with `created_at > after_event_id`; unified namespace documented
- 3.4 PENDING `streamViaPoll` fallback: emit `created_at` (Unix ms from stored event row) as the SSE `id:` value instead of the per-connection `seq` counter; ensures clients reconnecting from the fallback path are not flooded by the `id: 1/2/3...` vs large-timestamp mismatch

---

## 4.0 Post-Subscribe Race Fix

**Status:** PENDING

In `stream.zig`, the handler checks `isTerminalState(initial_state)` at line 78, then connects and subscribes to Redis pub/sub at lines 101–116 — a window of at least one network round-trip. If the run transitions to a terminal state and publishes its last event during this window, the subscriber is set up but never receives the event. `readMessage()` blocks indefinitely: the stream hangs open without emitting `run_complete`, and the client never knows the run finished.

The fix is a second terminal-state check immediately after `subscriber.subscribe()` returns, before entering the read loop.

**Dimensions:**
- 4.1 PENDING After `subscriber.subscribe(channel)` succeeds, re-query `SELECT state FROM runs WHERE run_id = $1`
- 4.2 PENDING If the post-subscribe state is terminal: call `streamStoredEvents` to replay missed gate results (from `after_event_id = last_event_id`), emit `run_complete` event, return — no read loop entered; for fresh connections `last_event_id = 0` so all events replay; for reconnects only the gap is replayed
- 4.3 PENDING If the post-subscribe state is non-terminal: proceed to read loop as before — race window is now closed
- 4.4 PENDING Add a test in `stream_test.zig` (or the existing `runs/tests.zig`) that simulates a run completing between the initial check and subscribe: verify the handler emits all events and closes cleanly

---

## 5.0 `zombiectl` npm CLI — `--watch` and `runs replay`

**Status:** PENDING

The `--watch` flag and `zombiectl runs replay` command exist only in the Zig `zombied` binary (M18_001). The `@usezombie/zombiectl` npm package (JavaScript) has no implementation of either. These must be ported so users installing from npm get the same capability.

**`--watch` (src/commands/core.js or a new src/commands/run-watch.js):**

- 5.1 PENDING After posting to `/v1/runs`, if `--watch` is passed, open an SSE connection to `GET /v1/runs/{run_id}:stream` using Node.js `fetch()` with a streaming response body reader (not buffered — read incrementally with `response.body.getReader()`)
- 5.2 PENDING Parse SSE lines: accumulate `event:` and `data:` fields, dispatch on blank-line boundary; on `gate_result` print `[{gate_name}] {outcome} (loop {n}, {ms}ms)` immediately
- 5.3 PENDING Exit the read loop and resolve when a `run_complete` event is received or when the server closes the stream
- 5.4 PENDING Pass `Authorization: Bearer $ZOMBIE_TOKEN` header on the SSE request (same auth as other API calls)

**`runs replay` (src/commands/runs.js):**

- 5.5 PENDING Add `runs replay <run_id>` subcommand that calls `GET /v1/runs/{run_id}:replay`
- 5.6 PENDING Render the structured gate narrative: for each gate entry print gate name, outcome, loop count, wall time, and stdout/stderr tail (mirror the Zig CLI output format from `src/cmd/runs.zig`)
- 5.7 PENDING Wire the subcommand into the CLI entry point (same pattern as existing `runs list`, `runs status`)

**Acceptance:**
- 5.8 PENDING `zombiectl run --spec <file> --watch` prints gate results in real time (not after the run completes)
- 5.9 PENDING `zombiectl runs replay <run_id>` prints a per-gate narrative for a completed run
- 5.10 PENDING Both commands work against the same API base URL and token as the rest of the CLI

---

## 6.0 Acceptance Criteria

**Status:** PENDING

- [ ] 6.1 `zombied run <spec> --watch` prints each gate result to stdout as it completes — not after the run finishes
- [ ] 6.2 SSE stream sends a heartbeat comment within 30 seconds when no gate events arrive (gate takes >30s)
- [ ] 6.3 Client disconnects after receiving 3 live events, reconnects with `Last-Event-ID` equal to the last received event's `id`; server replays exactly the missed events — no duplicates, no gaps
- [ ] 6.4 Run completes between initial state check and pub/sub subscribe; handler detects this, replays missed gate results (from `last_event_id`, or from `0` for fresh connections), emits `run_complete`, and closes — no indefinite hang, no duplicate events on reconnect
- [ ] 6.5 `zombiectl run --spec <file> --watch` (npm CLI) prints gate results in real time
- [ ] 6.6 `zombiectl runs replay <run_id>` (npm CLI) prints a per-gate narrative for a completed run
- [ ] 6.7 Cross-compile passes on all three CI targets (no regression)

---

## 7.0 Out of Scope

- Replacing Redis pub/sub with a different transport
- Horizontal SSE fan-out across multiple API instances (requires a shared broker; v2)
- Client-side reconnect logic (that is CLI/UI responsibility)
- Changing the SSE heartbeat interval (30s is specified in M18_001)
