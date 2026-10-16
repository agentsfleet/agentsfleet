// Stream frames only wake durable reads; private text never comes from a hint.

import { Effect, Redacted } from "effect";
import { ServerError } from "../../errors/index.ts";
import type { FetchImpl } from "../../lib/http.ts";
import { parseSseFrame } from "../../lib/sse.ts";

const STREAM_WAIT_MS = 2_000;
const FRAME_LIMIT_BYTES = 64 * 1024;
const FRAME_SEPARATOR = "\n\n";
const DELETED_EVENT = "draft.deleted";
const ACCESS_STATUSES = new Set([401, 403, 404, 410]);
type NoticeInput = {
  readonly url: string;
  readonly token: Redacted.Redacted<string>;
  readonly sequence: string;
  readonly state: string;
  readonly fetchImpl?: FetchImpl;
};

export function waitNotice(input: NoticeInput): Effect.Effect<void, ServerError> {
  return Effect.tryPromise({
    try: (signal) => readNotice(input, signal),
    catch: (cause) => cause instanceof ServerError ? cause : new ServerError({
      detail: "The private draft stream ended", suggestion: "resume the conversation to reconcile its durable state",
      status: 503, code: "CHAT_STREAM_ENDED", requestId: null,
    }),
  }).pipe(Effect.catchIf((error) => !ACCESS_STATUSES.has(error.status), () => Effect.void));
}

async function readNotice(input: NoticeInput, signal: AbortSignal): Promise<void> {
  const timeout = new AbortController();
  const timer = setTimeout(() => timeout.abort(), STREAM_WAIT_MS);
  try {
    const response = await (input.fetchImpl ?? fetch)(input.url, {
      headers: {Authorization: `Bearer ${Redacted.value(input.token)}`, Accept: "text/event-stream"},
      signal: AbortSignal.any([signal, timeout.signal]),
    });
    if (!response.ok) {
      await response.body?.cancel();
      if (ACCESS_STATUSES.has(response.status)) throw unavailable(response.status);
      return;
    }
    if (!response.body) return;
    await consume(response.body, input);
  } finally {
    timeout.abort();
    clearTimeout(timer);
  }
}

async function consume(body: ReadableStream<Uint8Array>, input: NoticeInput): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffered = "";
  try {
    while (true) {
      const {value, done} = await reader.read();
      if (done) return;
      // Refuse before decoding or accumulating an oversized transport chunk.
      if (value.byteLength + buffered.length > FRAME_LIMIT_BYTES) return;
      buffered = (buffered + decoder.decode(value, {stream: true})).replaceAll("\r\n", "\n");
      let boundary = buffered.indexOf(FRAME_SEPARATOR);
      while (boundary !== -1) {
        const event = parseSseFrame(buffered.slice(0, boundary));
        buffered = buffered.slice(boundary + FRAME_SEPARATOR.length);
        if (event?.type === DELETED_EVENT) throw unavailable(404);
        if (event && changed(event.data, input)) return;
        boundary = buffered.indexOf(FRAME_SEPARATOR);
      }
    }
  } finally {
    // Cleanup must preserve an access refusal if the transport is already closed.
    try { await reader.cancel().catch(() => undefined); } finally { reader.releaseLock(); }
  }
}

function changed(value: unknown, input: NoticeInput): boolean {
  return typeof value === "object" && value !== null && "sequence" in value && "state" in value
    && value.sequence === input.sequence && typeof value.state === "string" && value.state !== input.state;
}

function unavailable(status: number): ServerError {
  return new ServerError({detail: "This private conversation is no longer accessible",
    suggestion: "verify your sign-in and workspace access", status, code: "CHAT_ACCESS_ENDED", requestId: null});
}
