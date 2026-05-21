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
import type { FixtureKey } from "./constants";
import { refreshSessionToken } from "./clerk-admin";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");
const UNAUTHORIZED = 401;

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
  get<T>(p: string): Promise<T>;
  post<T>(p: string, body: unknown): Promise<T>;
  patch<T>(p: string, body: unknown): Promise<T>;
  delete(p: string): Promise<void>;
}

export type ClientHandle = FixtureKey | { sessionJwt: string; sessionId?: string };

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

  async function request(method: string, p: string, body?: unknown, reminted = false): Promise<Response> {
    const auth = { Authorization: `Bearer ${sessionJwt}` };
    const res = await fetch(`${base}${p}`, {
      method,
      headers: body !== undefined ? { ...auth, "Content-Type": "application/json" } : auth,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (res.status === UNAUTHORIZED && sessionId && !reminted) {
      sessionJwt = await refreshSessionToken(sessionId);
      return request(method, p, body, true);
    }
    if (!res.ok) {
      const detail = await res.text();
      throw new Error(`${method} ${p} → ${res.status}: ${detail}`);
    }
    return res;
  }

  return {
    async get<T>(p: string): Promise<T> {
      const res = await request("GET", p);
      return (await res.json()) as T;
    },
    async post<T>(p: string, body: unknown): Promise<T> {
      const res = await request("POST", p, body);
      return (await res.json()) as T;
    },
    async patch<T>(p: string, body: unknown): Promise<T> {
      const res = await request("PATCH", p, body);
      return (await res.json()) as T;
    },
    async delete(p: string): Promise<void> {
      await request("DELETE", p);
    },
  };
}
