# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: login-existing-zombie-lifecycle.spec.ts >> login → platform-ops lifecycle >> persistent fixture walks observe → bill → halt on a fresh platform-ops zombie
- Location: tests/e2e/auth/login-existing-zombie-lifecycle.spec.ts:45:3

# Error details

```
Error: POST /v1/workspaces/019e180c-bc91-7007-bfd4-49ba05f78230/zombies → 400: {"docs_uri":"https://docs.usezombie.com/error-codes#UZ-ZMB-008","title":"Invalid zombie config","detail":"Config JSON is not valid. Check trigger, tools, budget; `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars.","error_code":"UZ-ZMB-008","request_id":"req_d6423eec6310"}
```

# Test source

```ts
  1  | /**
  2  |  * Minimal authenticated fetch wrapper for fixture-driven API calls.
  3  |  *
  4  |  * Reads the JWT cache produced by global-setup.ts and returns a typed client
  5  |  * bound to a specific fixture user. No retry logic — fixture seeding/teardown
  6  |  * runs in a controlled environment, failures should fail tests loudly.
  7  |  *
  8  |  * ClientHandle accepts either a cached fixture key (persistent regular/admin
  9  |  * fixtures from .fixture-jwts.json) OR a raw `{ sessionJwt }` object for the
  10 |  * ephemeral signup-flow user, which is minted mid-test and is NOT in the
  11 |  * cache. One entrypoint, one fetch implementation — no duplicated request
  12 |  * logic (RULE UFS).
  13 |  */
  14 | import * as fs from "node:fs";
  15 | import * as path from "node:path";
  16 | import type { FixtureKey } from "./constants";
  17 | 
  18 | const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");
  19 | 
  20 | interface JwtEntry {
  21 |   email: string;
  22 |   clerkUserId: string;
  23 |   sessionJwt: string;
  24 |   cookieJwt: string;
  25 | }
  26 | 
  27 | function loadEntry(key: FixtureKey): JwtEntry {
  28 |   if (!fs.existsSync(JWT_CACHE_PATH)) {
  29 |     throw new Error(`Fixture JWT cache missing at ${JWT_CACHE_PATH}; globalSetup must run first.`);
  30 |   }
  31 |   const cache = JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as Record<string, JwtEntry>;
  32 |   const entry = cache[key];
  33 |   if (!entry) throw new Error(`No fixture entry for key '${key}'`);
  34 |   return entry;
  35 | }
  36 | 
  37 | function apiBase(): string {
  38 |   const url = process.env.NEXT_PUBLIC_API_URL;
  39 |   if (!url) throw new Error("NEXT_PUBLIC_API_URL must be set");
  40 |   return url;
  41 | }
  42 | 
  43 | export interface ApiClient {
  44 |   get<T>(p: string): Promise<T>;
  45 |   post<T>(p: string, body: unknown): Promise<T>;
  46 |   patch<T>(p: string, body: unknown): Promise<T>;
  47 |   delete(p: string): Promise<void>;
  48 | }
  49 | 
  50 | export type ClientHandle = FixtureKey | { sessionJwt: string };
  51 | 
  52 | export function clientFor(handle: ClientHandle): ApiClient {
  53 |   const sessionJwt =
  54 |     typeof handle === "string" ? loadEntry(handle).sessionJwt : handle.sessionJwt;
  55 |   const base = apiBase();
  56 |   const headers = { Authorization: `Bearer ${sessionJwt}` };
  57 | 
  58 |   async function request(method: string, p: string, body?: unknown): Promise<Response> {
  59 |     const res = await fetch(`${base}${p}`, {
  60 |       method,
  61 |       headers: body !== undefined ? { ...headers, "Content-Type": "application/json" } : headers,
  62 |       body: body !== undefined ? JSON.stringify(body) : undefined,
  63 |     });
  64 |     if (!res.ok) {
  65 |       const detail = await res.text();
> 66 |       throw new Error(`${method} ${p} → ${res.status}: ${detail}`);
     |             ^ Error: POST /v1/workspaces/019e180c-bc91-7007-bfd4-49ba05f78230/zombies → 400: {"docs_uri":"https://docs.usezombie.com/error-codes#UZ-ZMB-008","title":"Invalid zombie config","detail":"Config JSON is not valid. Check trigger, tools, budget; `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars.","error_code":"UZ-ZMB-008","request_id":"req_d6423eec6310"}
  67 |     }
  68 |     return res;
  69 |   }
  70 | 
  71 |   return {
  72 |     async get<T>(p: string): Promise<T> {
  73 |       const res = await request("GET", p);
  74 |       return (await res.json()) as T;
  75 |     },
  76 |     async post<T>(p: string, body: unknown): Promise<T> {
  77 |       const res = await request("POST", p, body);
  78 |       return (await res.json()) as T;
  79 |     },
  80 |     async patch<T>(p: string, body: unknown): Promise<T> {
  81 |       const res = await request("PATCH", p, body);
  82 |       return (await res.json()) as T;
  83 |     },
  84 |     async delete(p: string): Promise<void> {
  85 |       await request("DELETE", p);
  86 |     },
  87 |   };
  88 | }
  89 | 
```