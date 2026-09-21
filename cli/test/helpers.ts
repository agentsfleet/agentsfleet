import { Writable } from "node:stream";
import { ApiError, type FetchImpl } from "../src/lib/http.ts";


export { ApiError };
export type { FetchImpl };

// Structural Response mocks for tests that hit apiRequest
// only need ok/status/statusText/headers.get/text (+ optional body for SSE).
// Double-cast widens to FetchImpl at the test→prod boundary so production
// code paths still face full strict-mode pressure.
export interface ResponseLike {
  ok: boolean;
  status: number;
  statusText: string;
  headers: { get: (name: string) => string | null };
  text: () => Promise<string>;
  body?: unknown;
}

export const asFetchImpl = (
  impl: (url: string, init?: RequestInit) => Promise<ResponseLike>,
): FetchImpl => impl as unknown as FetchImpl;

// runCli's RunCliIo.fetchImpl expects the full `typeof fetch` shape
// (including `preconnect`). Structural test mocks only implement what
// production reads — widen at the boundary so internal code paths
// still face full strict-mode pressure.
export const asFetchOverride = (
  impl: (url: string, init?: RequestInit) => Promise<ResponseLike>,
): typeof fetch => impl as unknown as typeof fetch;

// `Map<string, string>.get` returns `string | undefined`, but ResponseLike's
// `headers.get` is `string | null`. Wrap a Map so the missing-key shape lines
// up with the production Headers contract.
export function makeHeaders(
  entries: ReadonlyArray<readonly [string, string]>,
): { get: (name: string) => string | null } {
  const map = new Map(entries);
  return { get: (name) => map.get(name) ?? null };
}

// Tests mutate `stream.isTTY = true` to flip color/spinner code paths
// (capability.ts reads it for !isTTY → NONE). The Node `Writable` class
// has no `isTTY` field; the intersection makes the test-set safe under
// strict types without an `as` cast at every assignment site.
export type TestStream = Writable & { isTTY?: boolean };

/** Discard-all writable stream (use one per test to avoid state leaks). */
export function makeNoop(): TestStream {
  return new Writable({ write(_c, _e, cb) { cb(); } });
}

/** Writable that buffers output; call .read() to inspect. */
export function makeBufferStream(): { stream: TestStream; read: () => string } {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

export interface UiTheme {
  ok: (s: string) => string;
  err: (s: string) => string;
  info: (s: string) => string;
  dim: (s: string) => string;
  head: (s: string) => string;
}

/** Passthrough UI theme (no ANSI escapes). */
export const ui: UiTheme = {
  ok: (s) => s,
  err: (s) => s,
  info: (s) => s,
  dim: (s) => s,
  head: (s) => s,
};

export const WS_ID      = "0195b4ba-8d3a-7f13-8abc-000000000010";

