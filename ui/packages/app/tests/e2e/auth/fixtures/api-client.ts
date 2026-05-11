/**
 * Minimal authenticated fetch wrapper for fixture-driven API calls.
 *
 * Reads the JWT cache produced by global-setup.ts and returns a typed client
 * bound to a specific fixture user. No retry logic — fixture seeding/teardown
 * runs in a controlled environment, failures should fail tests loudly.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { FixtureKey } from "./constants";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

interface JwtEntry {
  email: string;
  clerkUserId: string;
  sessionJwt: string;
  cookieJwt: string;
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

export function clientFor(key: FixtureKey): ApiClient {
  const entry = loadEntry(key);
  const base = apiBase();
  const headers = { Authorization: `Bearer ${entry.sessionJwt}` };

  async function request(method: string, p: string, body?: unknown): Promise<Response> {
    const res = await fetch(`${base}${p}`, {
      method,
      headers: body !== undefined ? { ...headers, "Content-Type": "application/json" } : headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
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
