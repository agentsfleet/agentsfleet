// @vitest-environment node
// The page is a Server Component: in Node the API client takes the absolute
// API origin, as it does on the server, not the browser's `/backend` proxy.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import NoWorkspaceEmptyState from "@/components/layout/NoWorkspaceEmptyState";
import { ApiError } from "@/lib/api/errors";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import { ERROR_CODE } from "@/lib/errors";
import { workspacePath } from "@/lib/workspace-routes";
import { AccountSetup } from "./AccountSetup";
import DashboardIndexPage from "./page";

// The dashboard entry decides, from one refused request, whether a new
// account is still opening: Clerk has the person, but the daemon's
// `user.created` handler has not yet written the tenant back, so the token
// carries none. Only that case may wait on `AccountSetup`; every other refusal
// must reach the error boundary, where "try again" is the honest answer.
//
// Only the boundaries are doubles: Clerk's `auth()` and the network. The page,
// the credential reads, the workspace walk, the API client and Next's
// `redirect` run for real, so a refusal reaches the page the way the daemon's
// problem body produces it.
const clerk = vi.hoisted(() => ({ token: "session-token", claims: {} as Record<string, unknown> }));
vi.mock("@clerk/nextjs/server", () => ({
  auth: () => Promise.resolve({ getToken: () => Promise.resolve(clerk.token), sessionClaims: clerk.claims }),
  reverificationError: () => null,
}));

const TENANT = "01900000-0000-7000-8000-00000000c0de";
const WORKSPACE = "01900000-0000-7000-8000-0000000000a1";
/** A session token minted before signup's writeback: no tenant yet. */
const OPENING_CLAIMS = { sub: "user_opening" };
/** The same person's token after the writeback, as the session template projects it. */
const OPENED_CLAIMS = { sub: "user_opened", metadata: { tenant_id: TENANT } };
const OWNED_WORKSPACE = {
  id: WORKSPACE,
  name: "Support triage",
  created_at: "2026-11-18T13:00:00Z",
  account: { tenant_id: TENANT, owner_name: "Fixture Owner" },
  role: ACCOUNT_ROLE.owner,
};

const network = vi.fn<typeof fetch>();

/** The daemon's problem body for a refused request. */
function problem(status: number, code: string): Response {
  return new Response(JSON.stringify({ type: `https://docs.agentsfleet.net/errors/${code}`, title: "Refused", status, error_code: code }), {
    status,
    headers: { "content-type": "application/problem+json" },
  });
}

/** One page of `GET /v1/tenants/me/workspaces`. */
function workspacePage(items: readonly unknown[]): Response {
  return new Response(JSON.stringify({ items, tenant_id: TENANT, total: null, next_cursor: null }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

/** The destination Next's `redirect` encodes in the error it throws: `NEXT_REDIRECT;<type>;<url>;<status>;`. */
function redirectTarget(thrown: unknown): string | null {
  if (typeof thrown !== "object" || thrown === null || !("digest" in thrown) || typeof thrown.digest !== "string") return null;
  const parts = thrown.digest.split(";");
  return parts[0] === "NEXT_REDIRECT" ? parts.slice(2, -2).join(";") : null;
}

beforeEach(() => {
  network.mockReset();
  vi.stubGlobal("fetch", network);
  clerk.claims = OPENING_CLAIMS;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("dashboard entry — an account still opening", () => {
  it.each([
    ["at authentication", 401, ERROR_CODE.AUTH_UNAUTHORIZED],
    ["at the scope gate", 403, ERROR_CODE.INSUFFICIENT_SCOPE],
  ])("waits on account setup when a token with no tenant is refused %s", async (_where, status, code) => {
    network.mockResolvedValueOnce(problem(status, code));
    const entry = await DashboardIndexPage();
    expect(entry.type).toBe(AccountSetup);
  });

  it("sends a refused token that already carries a tenant to the error page: waiting cannot help it", async () => {
    clerk.claims = OPENED_CLAIMS;
    network.mockResolvedValueOnce(problem(401, ERROR_CODE.AUTH_UNAUTHORIZED));
    await expect(DashboardIndexPage()).rejects.toMatchObject({ status: 401, code: ERROR_CODE.AUTH_UNAUTHORIZED });
  });

  it("never mistakes an outage for an opening account, even before the tenant lands", async () => {
    network.mockResolvedValueOnce(problem(503, "UZ-INTERNAL-003"));
    await expect(DashboardIndexPage()).rejects.toMatchObject({ status: 503, code: "UZ-INTERNAL-003" });
  });

  it("sends a failure that is not an API answer to the error page, even one that carries an opening code", async () => {
    // The client passes a transport failure through unchanged, so only the
    // page's `ApiError` check keeps this coded failure away from setup.
    const failure = Object.assign(new TypeError("fetch failed"), { code: ERROR_CODE.AUTH_UNAUTHORIZED });
    network.mockRejectedValueOnce(failure);
    const thrown = await DashboardIndexPage().then(() => null, (error: unknown) => error);
    expect(thrown).toBe(failure);
    expect(thrown).not.toBeInstanceOf(ApiError);
  });
});

describe("dashboard entry — an opened account", () => {
  beforeEach(() => {
    clerk.claims = OPENED_CLAIMS;
  });

  it("lists workspaces with the session token and redirects once to the owned workspace's new fleet page", async () => {
    network.mockResolvedValueOnce(workspacePage([OWNED_WORKSPACE]));
    const thrown = await DashboardIndexPage().then(() => null, (error: unknown) => error);
    expect(redirectTarget(thrown)).toBe(workspacePath(WORKSPACE, "fleets"));
    expect(network).toHaveBeenCalledTimes(1);
    const [input, init] = network.mock.calls[0] ?? [""];
    const target = input instanceof Request ? input.url : input.toString();
    expect(new URL(target).pathname).toBe("/v1/tenants/me/workspaces");
    expect(new Headers(init?.headers).get("authorization")).toBe(`Bearer ${clerk.token}`);
  });

  it("shows the create-workspace empty state only for a successful empty list", async () => {
    network.mockResolvedValueOnce(workspacePage([]));
    const entry = await DashboardIndexPage();
    expect(entry.type).toBe(NoWorkspaceEmptyState);
  });
});
