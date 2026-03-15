import { Writable } from "node:stream";
import { ApiError } from "../src/lib/http.js";

export { ApiError };

/** Discard-all writable stream (use one per test to avoid state leaks). */
export function makeNoop() {
  return new Writable({ write(_c, _e, cb) { cb(); } });
}

/** Writable that buffers output; call .read() to inspect. */
export function makeBufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

/** Passthrough UI theme (no ANSI escapes). */
export const ui = { ok: (s) => s, err: (s) => s, info: (s) => s, dim: (s) => s };

// ── Stable test constants ─────────────────────────────────────────────────────
export const AGENT_ID   = "0195b4ba-8d3a-7f13-8abc-000000000001";
export const AGENT_NAME = "my-agent";
export const WS_ID      = "0195b4ba-8d3a-7f13-8abc-000000000010";
export const SCORE_ID_1 = "0195b4ba-8d3a-7f13-8abc-000000000021";
export const SCORE_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000022";
export const RUN_ID_1   = "0195b4ba-8d3a-7f13-8abc-000000000031";
export const RUN_ID_2   = "0195b4ba-8d3a-7f13-8abc-000000000032";
export const PVER_ID    = "0195b4ba-8d3a-7f13-8abc-000000000041";
