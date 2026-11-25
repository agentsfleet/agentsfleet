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

/** Writable that buffers output; call .read() to inspect. */
export function makeBufferStream(): { stream: TestStream; read: () => string } {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}
