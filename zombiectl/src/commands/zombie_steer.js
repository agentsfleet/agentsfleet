// `zombiectl steer <zombie_id> "<message>"` — batch mode.
//
// 1. POST /messages → captures `event_id` from the 202 response.
// 2. Opens GET /events/stream (SSE) with the user's bearer token.
// 3. For every frame whose `event_id` matches, prints `[claw] <chunk>`
//    on `chunk` frames; closes the stream and returns 0 on
//    `event_complete` with `status=processed`, non-zero on `agent_error`
//    or `status=gate_blocked`.
// 4. If the SSE connection drops mid-event, falls back to polling
//    GET /events?since=<event_id_ms>&limit=1 until the row reaches a
//    terminal status (60 s timeout).
//
// Interactive REPL (no message) lands in a follow-up. The batch mode is
// the primary integration with chat-style UIs and operator scripts.

import { wsZombieMessagesPath, wsZombieEventsPath, wsZombieEventsStreamPath } from "../lib/api-paths.js";
import { streamGet as defaultStreamGet } from "../lib/sse.js";

const SSE_FALLBACK_TIMEOUT_MS = 60_000;
const FALLBACK_POLL_MS = 1_500;

export async function commandSteer(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  // streamGet is optionally injectable for tests; production resolves to
  // the real fetch-backed implementation. Same shape, same contract.
  const streamGet = deps.streamGet || defaultStreamGet;
  const parsed = parseFlags(args);
  const zombieId = parsed.positionals[0];
  const message = parsed.positionals[1];

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }
  if (!zombieId) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl steer <zombie_id> \"<message>\"", deps);
    return 2;
  }
  if (!message || typeof message !== "string" || message.trim().length === 0) {
    // Interactive REPL is a follow-up; for now require a message.
    writeError(ctx, "MISSING_ARGUMENT", "interactive steer is not yet implemented. Pass a message: zombiectl steer <zombie_id> \"<msg>\"", deps);
    return 2;
  }

  // Step 1 — POST /messages.
  const post = await request(ctx, wsZombieMessagesPath(wsId, zombieId), {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ message }),
  });
  const eventId = post?.event_id;
  if (!eventId) {
    writeError(ctx, "BAD_RESPONSE", "messages response missing event_id", deps);
    return 1;
  }

  // Step 2 — open SSE and filter on the captured event_id. SSE failures
  // (network, server close) drop us to the polling fallback.
  let outcome = await tailEventStream(ctx, wsId, zombieId, eventId, deps, streamGet).catch((err) => ({ kind: "sse_error", err }));

  if (outcome?.kind !== "complete") {
    outcome = await pollEventTerminal(ctx, wsId, zombieId, eventId, deps);
  }

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { event_id: eventId, ...outcome });
  } else if (outcome.kind === "complete") {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.ok(`event ${eventId} ${outcome.status}`));
  } else if (outcome.kind === "timeout") {
    writeLine(ctx.stderr, ui.err(`event ${eventId} still in flight after ${Math.round(SSE_FALLBACK_TIMEOUT_MS / 1000)}s — check: zombiectl events ${zombieId}`));
  } else {
    writeLine(ctx.stderr, ui.err(`message failed: ${outcome.kind}${outcome.detail ? ` — ${outcome.detail}` : ""}`));
  }

  return outcome.kind === "complete" && outcome.status === "processed" ? 0 : 1;
}

async function tailEventStream(ctx, wsId, zombieId, eventId, deps, streamGet) {
  const { ui, writeLine } = deps;
  const url = `${ctx.apiUrl}${wsZombieEventsStreamPath(wsId, zombieId)}`;
  const headers = { ...buildBearer(ctx) };

  let outcome = { kind: "sse_disconnected" };
  await streamGet(url, headers, (event) => {
    const payload = event.data;
    if (!payload || typeof payload !== "object") return undefined;
    if (payload.event_id && payload.event_id !== eventId) return undefined;
    if (event.type === "chunk" && typeof payload.text === "string") {
      writeLine(ctx.stdout, `${ui.dim("[claw]")} ${payload.text}`);
      return undefined;
    }
    if (event.type === "tool_call_started" && typeof payload.name === "string") {
      writeLine(ctx.stdout, `${ui.dim("[tool]")} ${payload.name} starting`);
      return undefined;
    }
    if (event.type === "tool_call_completed" && typeof payload.name === "string") {
      const ms = typeof payload.ms === "number" ? `${payload.ms}ms` : "";
      writeLine(ctx.stdout, `${ui.dim("[tool]")} ${payload.name} done ${ms}`);
      return undefined;
    }
    if (event.type === "event_complete") {
      outcome = { kind: "complete", status: payload.status || "unknown" };
      return false; // stop the stream
    }
    return undefined;
  });
  return outcome;
}

async function pollEventTerminal(ctx, wsId, zombieId, eventId, deps) {
  const { request, apiHeaders } = deps;
  const deadline = Date.now() + SSE_FALLBACK_TIMEOUT_MS;
  const sinceParam = eventIdToSince(eventId);
  while (Date.now() < deadline) {
    const url = `${wsZombieEventsPath(wsId, zombieId)}?limit=200${sinceParam ? `&since=${encodeURIComponent(sinceParam)}` : ""}`;
    const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) }).catch(() => null);
    const match = res?.items?.find((row) => row.event_id === eventId);
    if (match && isTerminal(match.status)) {
      return { kind: "complete", status: match.status };
    }
    await new Promise((resolve) => setTimeout(resolve, FALLBACK_POLL_MS));
  }
  return { kind: "timeout" };
}

// Redis stream IDs are `<ms>-<seq>`. The events endpoint's `since=` accepts
// RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`, no fractional seconds). Convert the
// milliseconds prefix back to that form, rounded to the start of the second
// the message was XADDed so the row itself is included.
function eventIdToSince(eventId) {
  const dash = eventId.indexOf("-");
  if (dash <= 0) return null;
  const ms = parseInt(eventId.slice(0, dash), 10);
  if (!Number.isFinite(ms)) return null;
  const floored = ms - (ms % 1000);
  const iso = new Date(floored).toISOString();
  return iso.replace(/\.\d{3}Z$/, "Z");
}

function isTerminal(status) {
  return status === "processed" || status === "agent_error" || status === "gate_blocked";
}

function buildBearer(ctx) {
  if (ctx.token) return { Authorization: `Bearer ${ctx.token}` };
  if (ctx.apiKey) return { Authorization: `Bearer ${ctx.apiKey}` };
  return {};
}
