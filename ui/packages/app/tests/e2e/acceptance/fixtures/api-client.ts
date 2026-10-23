/**
 * Minimal authenticated fetch wrapper for fixture-driven API calls.
 *
 * Reads the JWT cache produced by global-setup.ts and returns a typed client
 * bound to a specific fixture user. The only retry is a single re-mint on a
 * 401: the cached Bearer outlives its ~15-min TTL on a long suite run, so we
 * mint a fresh token on the same Clerk session and retry once. Any other
 * failure throws loudly — fixture seeding/teardown is a controlled path.
 *
 * ClientHandle accepts either a cached fixture key (persistent regular/admin
 * fixtures from .fixture-jwts.json) OR a raw `{ sessionJwt }` object for the
 * ephemeral signup-flow user, which is minted mid-test and is NOT in the
 * cache. One entrypoint, one fetch implementation — no duplicated request
 * logic (RULE UFS).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { JWT_CACHE_PATH, type FixtureKey } from "./constants";
import { refreshSessionToken } from "./clerk-admin";

const UNAUTHORIZED = 401;
const REQUEST_TIMEOUT_MS = 30_000;
const RESPONSE_MAX_BYTES = 4 * 1024 * 1024;

interface JwtEntry {
  email: string;
  clerkUserId: string;
  sessionId: string;
  sessionJwt: string;
}

function loadEntry(key: FixtureKey): JwtEntry {
  if (!fs.existsSync(JWT_CACHE_PATH)) {
    throw new Error(`Fixture JWT cache missing at ${JWT_CACHE_PATH}; globalSetup must run first.`);
  }
  const cache = JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as Record<string, JwtEntry>;
  const entry = cache[key];
  if (!entry) throw new Error(`No fixture entry for key '${key}'`);
  return entry;
}

function apiBase(): string {
  const url = process.env.NEXT_PUBLIC_API_URL;
  if (!url) throw new Error("NEXT_PUBLIC_API_URL must be set");
  return url;
}

export interface ApiClient {
  get<T>(p: string, headers?: Readonly<Record<string, string>>): Promise<T>;
  post<T>(p: string, body: unknown, headers?: Readonly<Record<string, string>>): Promise<T>;
  patch<T>(p: string, body: unknown, headers?: Readonly<Record<string, string>>): Promise<T>;
  delete(p: string, headers?: Readonly<Record<string, string>>): Promise<void>;
}

export type ClientHandle = FixtureKey | { sessionJwt: string; sessionId?: string };

/** Preserve HTTP status so fixture cleanup can distinguish absence from failure. */
export class FixtureHttpError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
    this.name = "FixtureHttpError";
  }
}

export function clientFor(handle: ClientHandle): ApiClient {
  // `sessionJwt` is mutable: a long suite run outlives the cached token's TTL,
  // so on a 401 we re-mint a fresh token on the same Clerk session and retry
  // once (AUTH.md: re-mint on 401). The ephemeral signup handle has no
  // sessionId, so it can't re-mint — but it's used immediately after mint.
  let sessionJwt: string;
  let sessionId: string | null;
  if (typeof handle === "string") {
    const entry = loadEntry(handle);
    sessionJwt = entry.sessionJwt;
    sessionId = entry.sessionId;
  } else {
    sessionJwt = handle.sessionJwt;
    sessionId = handle.sessionId ?? null;
  }
  const base = apiBase();

  async function request(method: string, p: string, body?: unknown,
    headers?: Readonly<Record<string, string>>, reminted = false): Promise<unknown> {
    const signal = AbortSignal.timeout(REQUEST_TIMEOUT_MS);
    const res = await fetch(`${base}${p}`, {
      method, signal, redirect: "error", cache: "no-store",
      headers: { Authorization: `Bearer ${sessionJwt}`,
        ...(body === undefined ? {} : { "Content-Type": "application/json" }), ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (res.status === UNAUTHORIZED && sessionId && !reminted) {
      await res.body?.cancel();
      sessionJwt = await refreshSessionToken(sessionId);
      return request(method, p, body, headers, true);
    }
    const value = res.status === 204 ? null : await readJson(res);
    if (!res.ok) {
      const record = typeof value === "object" && value !== null ? value : {};
      const errorCode: unknown = Reflect.get(record, "error_code");
      const requestId: unknown = Reflect.get(record, "request_id");
      const problem = { error_code: typeof errorCode === "string" ? errorCode : undefined,
        request_id: typeof requestId === "string" ? requestId : undefined };
      throw new FixtureHttpError(res.status, `${method} ${p} → ${res.status}: ${JSON.stringify(problem)}`);
    }
    return value;
  }

  return {
    async get<T>(p: string, headers?: Readonly<Record<string, string>>): Promise<T> {
      return await request("GET", p, undefined, headers) as T;
    },
    async post<T>(p: string, body: unknown, headers?: Readonly<Record<string, string>>): Promise<T> {
      return await request("POST", p, body, headers) as T;
    },
    async patch<T>(p: string, body: unknown, headers?: Readonly<Record<string, string>>): Promise<T> {
      return await request("PATCH", p, body, headers) as T;
    },
    async delete(p: string, headers?: Readonly<Record<string, string>>): Promise<void> {
      await request("DELETE", p, undefined, headers);
    },
  };
}

async function readJson(response: Response): Promise<unknown> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Fixture response is empty");
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const chunk = await reader.read();
      if (chunk.done) break;
      bytes += chunk.value.byteLength;
      if (bytes > RESPONSE_MAX_BYTES) throw new Error("Fixture response exceeds its bound");
      chunks.push(chunk.value);
    }
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } finally { await reader.cancel(); reader.releaseLock(); }
}
