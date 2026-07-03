import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// vi.mock is hoisted above the static `./platform` import, so the mock fn must
// be created via vi.hoisted() to exist when the factory runs (see runners.test.ts).
const { authMock } = vi.hoisted(() => ({ authMock: vi.fn() }));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));

import { readSessionScopes, hasScope } from "./platform";
import { expandScopes } from "@/lib/auth/scopes";

beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.resetAllMocks());

describe("readSessionScopes", () => {
  it("parses the top-level space-delimited scopes claim into a set (with closure)", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: { scopes: "runner:read runner:enroll model:admin" } });
    const scopes = await readSessionScopes();
    // model:admin closes down to model:read; runner:read already explicit.
    expect([...scopes].sort()).toEqual(["model:admin", "model:read", "runner:enroll", "runner:read"]);
  });

  it("accepts a JSON-array scopes claim too (backend-tolerant reader parity)", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: { scopes: ["runner:read", "model:read"] } });
    const scopes = await readSessionScopes();
    expect([...scopes].sort()).toEqual(["model:read", "runner:read"]);
  });

  it("collapses arbitrary whitespace and ignores empty tokens", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: { scopes: "  runner:read   model:read  " } });
    const scopes = await readSessionScopes();
    expect([...scopes].sort()).toEqual(["model:read", "runner:read"]);
  });

  it("expands the downward closure — a held write/admin scope satisfies its read rung", async () => {
    // Mirrors the backend parseClaim closure: the documented operator set carries
    // runner:write / model:admin (not the :read rungs) — the read-gated pages
    // must still resolve, matching the backend requireScope decision.
    authMock.mockResolvedValueOnce({ sessionClaims: { scopes: "runner:write model:admin" } });
    const scopes = await readSessionScopes();
    expect(scopes.has("runner:read")).toBe(true);
    expect(scopes.has("model:read")).toBe(true);
    expect(scopes.has("runner:write")).toBe(true);
    expect(scopes.has("model:admin")).toBe(true);
  });

  it("is empty (fail-closed) when the scopes claim is absent", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { tenant_id: "t1" } } });
    expect((await readSessionScopes()).size).toBe(0);
  });

  it("is empty (fail-closed) — the legacy metadata.platform_admin boolean is never consulted", async () => {
    // A session carrying only the retired boolean grants nothing now.
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { platform_admin: true } } });
    expect((await readSessionScopes()).size).toBe(0);
  });

  it("is empty (fail-closed) for an anonymous session with no claims", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: null });
    expect((await readSessionScopes()).size).toBe(0);
  });

  it("is empty (fail-closed) when the auth provider throws", async () => {
    authMock.mockRejectedValueOnce(new Error("clerk unavailable"));
    expect((await readSessionScopes()).size).toBe(0);
  });
});

describe("hasScope", () => {
  it("is true only for a scope the session token actually carries", async () => {
    authMock.mockResolvedValue({ sessionClaims: { scopes: "runner:read model:read" } });
    await expect(hasScope("runner:read")).resolves.toBe(true);
    await expect(hasScope("runner:enroll")).resolves.toBe(false);
  });

  it("is false (fail-closed) when the auth provider throws", async () => {
    authMock.mockRejectedValueOnce(new Error("clerk unavailable"));
    await expect(hasScope("runner:read")).resolves.toBe(false);
  });
});

// expandScopes downward-closure contract (security: no privilege escalation).
describe("expandScopes", () => {
  it("adds the downward closure of a held ladder scope", () => {
    expect([...expandScopes(["model:admin"])].sort()).toEqual(["model:admin", "model:read"]);
    expect([...expandScopes(["runner:write"])].sort()).toEqual(["runner:read", "runner:write"]);
    expect([...expandScopes(["fleet:admin"])].sort()).toEqual(["fleet:admin", "fleet:read", "fleet:write"]);
  });

  it("never closes UPWARD — a read scope must not grant write/admin (no escalation)", () => {
    const s = expandScopes(["model:read"]);
    expect(s.has("model:admin")).toBe(false);
    const r = expandScopes(["runner:read"]);
    expect(r.has("runner:write")).toBe(false);
    expect(r.has("runner:enroll")).toBe(false); // enroll is independent, never implied
  });

  it("passes an unknown scope through verbatim, granting nothing extra", () => {
    expect([...expandScopes(["totally:bogus"])]).toEqual(["totally:bogus"]);
  });

  it("returns an empty set for no held scopes", () => {
    expect(expandScopes([]).size).toBe(0);
  });
});

// Dimension 4.2 / Invariant 3: no operator surface is gated on the legacy
// `platform_admin` boolean after §4. Scan the operator source (not tests) for
// the retired identifiers — the source of truth the E9 eval command also greps.
describe("platform_admin fully retired (Invariant 3)", () => {
  const ROOT = join(__dirname, "..", "..");
  const SCANNED = [
    join(ROOT, "lib", "auth"),
    join(ROOT, "app", "(dashboard)", "admin"),
    join(ROOT, "components", "layout"),
  ];
  const FORBIDDEN = /platform_admin|readPlatformAdminClaim|isPlatformAdmin/;

  function sourceFiles(dir: string): string[] {
    const out: string[] = [];
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) out.push(...sourceFiles(full));
      else if (/\.(ts|tsx)$/.test(entry.name) && !entry.name.includes(".test.")) out.push(full);
    }
    return out;
  }

  it("no operator source file references the retired platform_admin boolean", () => {
    const offenders = SCANNED.flatMap(sourceFiles).filter((f) => FORBIDDEN.test(readFileSync(f, "utf8")));
    expect(offenders).toEqual([]);
  });
});
