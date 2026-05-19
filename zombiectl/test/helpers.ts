import { Writable } from "node:stream";
import { ApiError, type FetchImpl } from "../src/lib/http.ts";

import type { ParsedArgs } from "../src/commands/types.ts";

export { ApiError };
export type { FetchImpl };

// Structural Response mocks for tests that hit apiRequest / streamFetch
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

export interface UiTheme {
  ok: (s: string) => string;
  err: (s: string) => string;
  info: (s: string) => string;
  dim: (s: string) => string;
  head: (s: string) => string;
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

/** Passthrough UI theme (no ANSI escapes). */
export const ui: UiTheme = {
  ok: (s) => s,
  err: (s) => s,
  info: (s) => s,
  dim: (s) => s,
  head: (s) => s,
};

// Build the parsed = { options, positionals } shape that leaf handlers
// expect from a flat token array. Test-only utility — production now
// flows through commander (cli-tree.ts). Matches the legacy parseFlags
// surface byte-for-byte so direct handler tests can keep synthesising
// parsed objects from `["--limit", "20", "<positional>"]` token lists.
export function buildParsed(tokens: readonly string[] = []): ParsedArgs {
  const options: ParsedArgs["options"] = {};
  const positionals: string[] = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token === undefined) continue;
    if (!token.startsWith("--")) { positionals.push(token); continue; }
    const eq = token.indexOf("=");
    if (eq !== -1) {
      options[token.slice(2, eq)] = token.slice(eq + 1);
      continue;
    }
    const key = token.slice(2);
    const next = tokens[i + 1];
    if (next !== undefined && !next.startsWith("--")) {
      options[key] = next;
      i += 1;
    } else {
      options[key] = true;
    }
  }
  return { options, positionals };
}

// ── Stable test constants ─────────────────────────────────────────────────────
export const AGENT_ID   = "0195b4ba-8d3a-7f13-8abc-000000000001";
export const AGENT_NAME = "my-agent";
export const WS_ID      = "0195b4ba-8d3a-7f13-8abc-000000000010";
export const SCORE_ID_1 = "0195b4ba-8d3a-7f13-8abc-000000000021";
export const SCORE_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000022";
export const RUN_ID_1   = "0195b4ba-8d3a-7f13-8abc-000000000031";
export const RUN_ID_2   = "0195b4ba-8d3a-7f13-8abc-000000000032";
export const PVER_ID    = "0195b4ba-8d3a-7f13-8abc-000000000041";
