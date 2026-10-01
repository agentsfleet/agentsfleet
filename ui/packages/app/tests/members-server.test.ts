import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";

// Only the identity provider and the network are stood in for: the actions run
// the real `withToken`, the real client, and the real decoders.
const { credential, redirect } = vi.hoisted(() => ({
  credential: vi.fn<() => Promise<string | null>>(),
  redirect: vi.fn((path: string) => {
    throw new Error(`NEXT_REDIRECT:${path}`);
  }),
}));
vi.mock("@/lib/auth/credential", () => ({
  credential,
  requireCredential: async () => (await credential()) ?? redirect("/sign-in"),
}));
vi.mock("next/navigation", () => ({ redirect }));

import MembersPage from "@/app/(dashboard)/settings/members/page";
import {
  createInviteAction,
  loadTeamAction,
  removeMemberAction,
  revokeInviteAction,
  sendInviteEmailAction,
} from "@/app/(dashboard)/settings/members/actions";
import { ERROR_CODE } from "@/lib/errors";
import { SIGN_IN_PATH } from "@/lib/auth/sign-in-redirect";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces";

const MEMBERS_PATH = "/v1/tenants/me/members";
const INVITES_PATH = "/v1/tenants/me/invites";
const OWNER = { user_id: "user_john", display_name: "John", email: "john@example.com", role: ACCOUNT_ROLE.owner, joined_at: 1 };
const INVITE = {
  id: "inv_1",
  email: "bob@example.com",
  role: ACCOUNT_ROLE.member,
  expires_at: 2,
  created_at: 1,
  link: "https://app.agentsfleet.net/invites/inv_1",
  email_status: "sent",
  email_sent_at: 1,
};

function json(status: number, body?: unknown): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

// Answers by path, so the two parallel reads may arrive in either order.
function serve(routes: Record<string, () => Response>) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    // The client always sends a string URL.
    const url = input as string;
    const hit = Object.keys(routes).find((path) => url.includes(path));
    if (!hit) throw new Error(`unexpected request ${url}`);
    return routes[hit]!();
  });
}

beforeEach(() => credential.mockResolvedValue("tok_owner"));
afterEach(() => vi.restoreAllMocks());

describe("members server actions", () => {
  it("should load both lists together for the page to replace its state with", async () => {
    serve({
      [MEMBERS_PATH]: () => json(200, { items: [OWNER], total: 1, next_cursor: null }),
      [INVITES_PATH]: () => json(200, { items: [INVITE], total: 1, next_cursor: null }),
    });
    await expect(loadTeamAction()).resolves.toEqual({ ok: true, data: { members: [OWNER], invites: [INVITE] } });
  });

  it("should fail the whole load when either list fails, so the page never shows half a team", async () => {
    serve({
      [MEMBERS_PATH]: () => json(200, { items: [OWNER], total: 1, next_cursor: null }),
      [INVITES_PATH]: () => json(403, { title: "forbidden", status: 403, error_code: "UZ-AUTH-001", user_message: "No." }),
    });
    await expect(loadTeamAction()).resolves.toMatchObject({ ok: false, status: 403, errorCode: "UZ-AUTH-001" });
  });

  it("should return a duplicate invite as a failed result carrying the backend's code", async () => {
    serve({ [INVITES_PATH]: () => json(409, { title: "conflict", status: 409, error_code: "UZ-INV-003", user_message: "Already invited." }) });
    await expect(createInviteAction("bob@example.com")).resolves.toEqual({
      ok: false,
      status: 409,
      errorCode: "UZ-INV-003",
      error: "Already invited.",
    });
  });

  it("should revoke and remove through the owner's routes", async () => {
    const spy = serve({ [INVITES_PATH]: () => json(204), [MEMBERS_PATH]: () => json(204) });
    await expect(revokeInviteAction("inv_1")).resolves.toEqual({ ok: true, data: undefined });
    await expect(removeMemberAction("user_bob")).resolves.toEqual({ ok: true, data: undefined });
    expect(spy.mock.calls.map(([url]) => url as string)).toEqual([
      expect.stringContaining(`${INVITES_PATH}/inv_1`),
      expect.stringContaining(`${MEMBERS_PATH}/user_bob`),
    ]);
  });

  it("should send an invite's email again through its send route", async () => {
    const spy = serve({ [`${INVITES_PATH}/inv_1/send`]: () => json(200, { email_status: "sent" }) });
    await expect(sendInviteEmailAction("inv_1")).resolves.toEqual({ ok: true, data: undefined });
    expect(spy.mock.calls.map(([url]) => url as string)).toEqual([expect.stringContaining(`${INVITES_PATH}/inv_1/send`)]);
  });

  it("should refuse without calling the backend when there is no session", async () => {
    credential.mockResolvedValue(null);
    const spy = serve({});
    await expect(removeMemberAction("user_bob")).resolves.toMatchObject({ ok: false, status: 401, errorCode: ERROR_CODE.AUTH_401 });
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("members page", () => {
  it("should hand the view both lists from one render", async () => {
    serve({
      [MEMBERS_PATH]: () => json(200, { items: [OWNER], total: 1, next_cursor: null }),
      [INVITES_PATH]: () => json(200, { items: [INVITE], total: 1, next_cursor: null }),
    });
    const element = (await MembersPage()) as ReactElement<{ initialMembers: unknown; initialInvites: unknown }>;
    expect(element.props).toEqual({ initialMembers: [OWNER], initialInvites: [INVITE] });
  });

  it("should send an expired session to sign in", async () => {
    serve({
      [MEMBERS_PATH]: () => json(401, { title: "unauthorized", status: 401, error_code: "UZ-AUTH-002" }),
      [INVITES_PATH]: () => json(200, { items: [], total: 0, next_cursor: null }),
    });
    await expect(MembersPage()).rejects.toThrow(`NEXT_REDIRECT:${SIGN_IN_PATH}`);
  });

  it("should let any other failure reach the error boundary rather than render an empty team", async () => {
    serve({
      [MEMBERS_PATH]: () => json(404, { title: "not found", status: 404, error_code: "UZ-REQ-404" }),
      [INVITES_PATH]: () => json(200, { items: [], total: 0, next_cursor: null }),
    });
    await expect(MembersPage()).rejects.toMatchObject({ status: 404 });
    expect(redirect).not.toHaveBeenCalled();
  });
});
