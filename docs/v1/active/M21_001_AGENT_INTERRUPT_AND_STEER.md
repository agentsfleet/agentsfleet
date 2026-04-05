# M21_001: Human-in-the-Loop Agent Interrupt and Steer

**Prototype:** v1.0.0
**Milestone:** M21
**Workstream:** 001
**Date:** Mar 30, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m21-001-interrupt-steer
**Priority:** P1 — No way to redirect a running agent without aborting it; user must kill the run and restart from scratch
**Batch:** B1
**Depends on:** M18_001 (SSE stream must be live before interrupt UX can be built on top); `stream.zig` must patch `TERMINAL_STATES` to include `"ABORTED"` — without this, acceptance criteria 5.4 cannot pass (stream hangs indefinitely after abort instead of closing with `run_complete`)

---

## Overview

A user watching a live agent run via SSE (`GET /v1/runs/{id}:stream`) must be able to steer the agent mid-run — either instantly (injected into the current executor turn) or at the next clean checkpoint (between gate iterations). The agent absorbs the command and continues; it does not abort or restart.

Two entry points:
- **CLI chat bar** — terminal split: transcript above, input below, shown when `--watch` is active
- **Desktop/UI chat overlay** — chat window alongside the SSE stream view, with optional voice input

Two interrupt modes:
- **Instant** — message injected into the executor IPC stream mid-turn (Codex-style); executor sees it as an in-band user message in the current stage
- **Queued** — message written to Redis, picked up at the next gate iteration checkpoint; safer, no subprocess disruption

---

## 1.0 Interrupt API and Storage

**Status:** PENDING

`POST /v1/runs/{id}:interrupt` accepts `{"message": "...", "mode": "instant" | "queued"}`. For `queued` mode, the API writes the message to Redis key `run:{id}:interrupt` with a 300s TTL. For `instant` mode, it additionally signals the executor via the existing IPC channel if an active execution is in flight. The endpoint requires the same workspace auth as other run endpoints. Returns `{"ack": true, "mode": "instant"|"queued"}`.

**Dimensions:**
- 1.1 PENDING `POST /v1/runs/{id}:interrupt` endpoint: validates run exists, checks workspace auth, accepts `message` + `mode` fields
- 1.2 PENDING Queued path: write `run:{id}:interrupt` Redis key (SETEX 300); return ack immediately; **last-write-wins**: if two interrupts arrive before the next gate checkpoint, the second silently overwrites the first — this is intentional v1 behavior; callers must not assume delivery of every POST
- 1.3 PENDING Instant path: additionally call executor IPC `injectUserMessage` if an active `execution_id` exists on the run row; if IPC returns an error (including TOCTOU: executor terminated after DB read), fall back to the queued path — write Redis key, return `mode: queued` in ack; never silently drop
- 1.4 PENDING SSE stream emits `event: interrupt_ack` with `{"mode":"queued"|"instant","received_at":...}` after the interrupt is stored or delivered

---

## 2.0 Worker Checkpoint Polling

**Status:** PENDING

The worker gate loop polls for a pending interrupt between gate iterations. When found, it deletes the Redis key (exactly-once delivery), then injects the message as a new executor stage turn before starting the next gate. The agent receives the message as user context — it does not reset state, it amends course. Results from the corrected turn stream back via the existing SSE path.

**Dimensions:**
- 2.1 PENDING Worker gate loop polls `GET run:{id}:interrupt` (GETDEL) at the top of each repair iteration before running the next gate command
- 2.2 PENDING When interrupt found: call `executor.injectUserMessage(execution_id, message)` — executor sends it as an in-band user turn to the current agent stage
- 2.3 PENDING After injection, resume gate loop normally — the next gate command runs with the corrected agent state; no retry counter reset
- 2.4 PENDING If no active executor (run is between stages): log the interrupt message as a `run_transitions` annotation and continue — never silently drop

---

## 3.0 CLI Chat Bar

**Status:** PENDING

When `zombied run <spec> --watch` is active, the terminal renders a split view: SSE transcript scrolls above a fixed one-line input bar at the bottom. The user types a message and hits Enter (or uses `/` prefix for Oracle commands). The CLI sends `POST /v1/runs/{id}:interrupt` and shows a confirmation on the transcript line. Ctrl+C cancels the run via `POST /v1/runs/{id}:abort`.

**Dimensions:**
- 3.1 PENDING CLI `--watch` renders a fixed bottom input bar using ANSI escape sequences; transcript scrolls in the region above it
- 3.2 PENDING On Enter: send `POST /v1/runs/{id}:interrupt {"message": input, "mode": "queued"}`; on `interrupt_ack` SSE event, show `→ steered` inline in the transcript
- 3.3 PENDING Oracle command prefix `/`: `/stop` → abort, `/retry` → retry, `/skip <gate>` → queued interrupt "skip the <gate> gate this iteration", free text → queued interrupt as-is
- 3.4 PENDING Ctrl+C sends `POST /v1/runs/{id}:abort` and exits; transcript line shows `✗ aborted`

---

## 4.0 Desktop and Voice Interface

**Status:** PENDING

The Desktop App and web UI show a chat overlay alongside the SSE stream view. The chat input sends interrupts; the transcript area displays SSE events. A microphone button activates browser `SpeechRecognition` (Web Speech API) — transcribed text auto-populates the input and submits on pause. The `interrupt_ack` SSE event clears the input and adds a confirmation marker in the transcript.

**Dimensions:**
- 4.1 PENDING UI chat overlay: fixed panel below the SSE transcript; input box + Send button + mic button; visible only while run state is non-terminal
- 4.2 PENDING On Send: `POST /v1/runs/{id}:interrupt {"message": ..., "mode": "queued"}`; on `interrupt_ack`, render `→ Oracle received` below the sent message
- 4.3 PENDING Voice: browser `SpeechRecognition` API; mic button toggles listen state; transcribed text fills the input box; auto-submit on 1.5s silence
- 4.4 PENDING `run_complete` SSE event disables the input bar and mic button; shows final state badge

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 User sends a text command via CLI chat bar while a gate loop is running; next gate iteration picks it up and the agent corrects course — no restart, no abort
- [ ] 5.2 User sends a command via Desktop UI chat overlay; `interrupt_ack` SSE event confirms receipt within 1s of POST
- [ ] 5.3 Voice input transcribes speech to text and submits an interrupt; agent receives the spoken command
- [ ] 5.4 `/stop` from CLI chat bar aborts the run; SSE stream closes with `run_complete` event showing `state: ABORTED`
- [ ] 5.5 Interrupt with `mode: instant` delivers the message to the active executor turn without waiting for the next gate iteration
- [ ] 5.6 If no active executor, interrupt is annotated on `run_transitions` and not silently dropped

---

## 6.0 Out of Scope

- WebSocket transport (SSE + POST interrupt is sufficient; no bidirectional socket needed)
- Server-side speech-to-text (browser Web Speech API handles transcription client-side)
- Interrupt history or replay of past interrupts
- Multi-agent fan-out (broadcasting one interrupt to multiple concurrent agents on the same run)
- Rate limiting interrupts per run (v2 concern)
