import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { acceptInvite, createInvite, listInvites, listWaitingInvites, revokeInvite } from "./invites";

// The network is the only thing stood in for: every call runs the real client,
// the real problem-body parsing, and the real decoders.

const TOKEN = "tok_owner";
const EXPIRES_AT = Date.UTC(2026, 9, 7);
const CREATED_AT = Date.UTC(2026, 8, 30);
const INVITE = {
  id: "0195b4ba-8d3a-7f13-8abc-0000000000a1",
  email: "bob@example.com",
  role: "member",
  expires_at: EXPIRES_AT,
  created_at: CREATED_AT,
  link: "https://app.agentsfleet.net/invites/0195b4ba-8d3a-7f13-8abc-0000000000a1",
};
const WAITING = {
  id: INVITE.id,
  account: { tenant_id: "0195b4ba-8d3a-7f13-8abc-0000000000t1", owner_name: "John" },
  expires_at: EXPIRES_AT,
};

afterEach(() => vi.restoreAllMocks());

function answer(status: number, body?: unknown) {
  return vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json" },
    }),
  );
}

function problem(status: number, code: string, message: string) {
  return answer(status, { title: "refused", status, error_code: code, user_message: message });
}

function sent(spy: ReturnType<typeof answer>): { url: string; init: RequestInit } {
  const [url, init] = spy.mock.calls[0] ?? [];
  // The client always sends a string URL and a string body.
  return { url: url as string, init: init as RequestInit };
}

describe("owner invites", () => {
  it("should POST the address to the account's invites and return the decoded invite", async () => {
    const spy = answer(201, INVITE);
    await expect(createInvite(TOKEN, "bob@example.com")).resolves.toEqual(INVITE);
    const { url, init } = sent(spy);
    expect(url).toContain("/v1/tenants/me/invites");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({ email: "bob@example.com" });
  });

  it("should reject a created invite that carries no link, since the page has nothing to copy", async () => {
    const { link: _link, ...withoutLink } = INVITE;
    answer(201, withoutLink);
    await expect(createInvite(TOKEN, "bob@example.com")).rejects.toThrow("invite is invalid");
  });

  it("should surface a duplicate invite as the backend's 409 with its code and message", async () => {
    problem(409, "UZ-INV-003", "That address already has a pending invite or is a member.");
    const error = await createInvite(TOKEN, "bob@example.com").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 409,
      code: "UZ-INV-003",
      message: "That address already has a pending invite or is a member.",
    });
  });

  it("should list the account's pending invites from the one page", async () => {
    const spy = answer(200, { items: [INVITE], total: 1, next_cursor: null });
    await expect(listInvites(TOKEN)).resolves.toEqual([INVITE]);
    expect(sent(spy).init.method).toBe("GET");
  });

  it("should reject a list response that omits its items", async () => {
    answer(200, { total: 0, next_cursor: null });
    await expect(listInvites(TOKEN)).rejects.toThrow("list response omitted items");
  });

  it("should reject an invite whose expiry is not an integer timestamp", async () => {
    answer(200, { items: [{ ...INVITE, expires_at: "2026-10-07" }], total: 1, next_cursor: null });
    await expect(listInvites(TOKEN)).rejects.toThrow("invite is invalid");
  });

  it("should DELETE the invite by an encoded id and resolve on 204", async () => {
    const spy = answer(204);
    await expect(revokeInvite(TOKEN, "a/b")).resolves.toBeUndefined();
    const { url, init } = sent(spy);
    expect(url).toContain("/v1/tenants/me/invites/a%2Fb");
    expect(init.method).toBe("DELETE");
  });
});

describe("invites waiting for the caller", () => {
  it("should list waiting invites with the account each one joins", async () => {
    const spy = answer(200, { items: [WAITING], total: 1, next_cursor: null });
    await expect(listWaitingInvites(TOKEN)).resolves.toEqual([WAITING]);
    expect(sent(spy).url).toContain("/v1/me/invites");
  });

  it("should reject a waiting invite with no id, since the accept route names one", async () => {
    const { id: _id, ...withoutId } = WAITING;
    answer(200, { items: [withoutId], total: 1, next_cursor: null });
    await expect(listWaitingInvites(TOKEN)).rejects.toThrow("waiting invite is invalid");
  });

  it("should reject a waiting invite whose account has a blank owner name", async () => {
    answer(200, { items: [{ ...WAITING, account: { ...WAITING.account, owner_name: " " } }], total: 1, next_cursor: null });
    await expect(listWaitingInvites(TOKEN)).rejects.toThrow("workspace account is invalid");
  });

  it("should POST the accept and return the joined account's workspaces", async () => {
    const accepted = { tenant_id: WAITING.account.tenant_id, workspace_ids: ["ws_1", "ws_2"] };
    const spy = answer(200, accepted);
    await expect(acceptInvite(TOKEN, INVITE.id)).resolves.toEqual(accepted);
    const { url, init } = sent(spy);
    expect(url).toContain(`/v1/me/invites/${INVITE.id}/accept`);
    expect(init.method).toBe("POST");
  });

  it("should reject an accept response listing a workspace id that is not a string", async () => {
    answer(200, { tenant_id: WAITING.account.tenant_id, workspace_ids: ["ws_1", 7] });
    await expect(acceptInvite(TOKEN, INVITE.id)).rejects.toThrow("accepted invite is invalid");
  });

  it("should surface an accept for another address as 403 UZ-INV-002, naming no address", async () => {
    problem(403, "UZ-INV-002", "This invite was sent to a different email address. Sign in with that address to accept it.");
    const error = await acceptInvite(TOKEN, INVITE.id).catch((e: unknown) => e);
    expect(error).toMatchObject({ status: 403, code: "UZ-INV-002" });
    expect((error as Error).message).not.toContain("@");
  });

  it("should surface an expired or revoked invite as 404 UZ-INV-001", async () => {
    problem(404, "UZ-INV-001", "This invite has expired or was revoked.");
    await expect(acceptInvite(TOKEN, INVITE.id)).rejects.toMatchObject({ status: 404, code: "UZ-INV-001" });
  });
});
