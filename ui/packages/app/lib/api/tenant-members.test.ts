import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { listMembers, listWorkspaceMembers, removeMember } from "./tenant-members";
import { ACCOUNT_ROLE } from "./workspaces-types";

const TOKEN = "tok_owner";
const OWNER = { user_id: "user_john", display_name: "John", email: "john@example.com", role: ACCOUNT_ROLE.owner, joined_at: 1 };
const MEMBER = { user_id: "user_bob", display_name: null, email: "bob@example.com", role: ACCOUNT_ROLE.member, joined_at: 2 };

afterEach(() => vi.restoreAllMocks());

function answer(status: number, body?: unknown) {
  return vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json" },
    }),
  );
}

describe("listMembers", () => {
  it("should decode every member, keeping a missing display name as null", async () => {
    const spy = answer(200, { items: [OWNER, MEMBER], total: 2, next_cursor: null });
    await expect(listMembers(TOKEN)).resolves.toEqual([OWNER, MEMBER]);
    expect(spy.mock.calls[0]?.[0] as string).toContain("/v1/tenants/me/members");
  });

  it("should reject a role this dashboard does not know rather than guess one", async () => {
    answer(200, { items: [{ ...MEMBER, role: "admin" }], total: 1, next_cursor: null });
    await expect(listMembers(TOKEN)).rejects.toThrow("member is invalid");
  });

  it("should reject a member with no address", async () => {
    answer(200, { items: [{ ...MEMBER, email: "" }], total: 1, next_cursor: null });
    await expect(listMembers(TOKEN)).rejects.toThrow("member is invalid");
  });

  it("should reject a display name that is neither text nor null", async () => {
    answer(200, { items: [{ ...MEMBER, display_name: 42 }], total: 1, next_cursor: null });
    await expect(listMembers(TOKEN)).rejects.toThrow("member is invalid");
  });

  it("should reject a member with no join time rather than show a wrong one", async () => {
    answer(200, { items: [{ ...MEMBER, joined_at: undefined }], total: 1, next_cursor: null });
    await expect(listMembers(TOKEN)).rejects.toThrow("member is invalid");
  });
});

describe("removeMember", () => {
  it("should DELETE the member by an encoded id and resolve on 204", async () => {
    const spy = answer(204);
    await expect(removeMember(TOKEN, "user bob")).resolves.toBeUndefined();
    const [url, init] = spy.mock.calls[0] ?? [];
    expect(url as string).toContain("/v1/tenants/me/members/user%20bob");
    expect((init as RequestInit).method).toBe("DELETE");
  });

  it("should surface removing the last owner as 409 UZ-INV-004", async () => {
    answer(409, { title: "conflict", status: 409, error_code: "UZ-INV-004", user_message: "The account's last owner cannot be removed." });
    const error = await removeMember(TOKEN, OWNER.user_id).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({ status: 409, code: "UZ-INV-004" });
  });
});

describe("listWorkspaceMembers", () => {
  const WORKSPACE = "ws 1";
  const BOB = { user_id: "user_bob", display_name: "Bob", role: ACCOUNT_ROLE.member, actor: "steer:user_bob" };

  it("should decode each member with the actor their messages record", async () => {
    const spy = answer(200, { items: [BOB, { ...BOB, user_id: "user_x", display_name: null }], total: 2, next_cursor: null });
    await expect(listWorkspaceMembers(WORKSPACE, TOKEN)).resolves.toEqual([
      BOB,
      { ...BOB, user_id: "user_x", display_name: null },
    ]);
    expect(spy.mock.calls[0]?.[0] as string).toContain("/v1/workspaces/ws%201/members");
  });

  it("should reject a member with no actor, which no message could match", async () => {
    answer(200, { items: [{ ...BOB, actor: "" }], total: 1, next_cursor: null });
    await expect(listWorkspaceMembers(WORKSPACE, TOKEN)).rejects.toThrow("workspace member is invalid");
  });

  it("should reject a display name that is neither text nor null", async () => {
    answer(200, { items: [{ ...BOB, display_name: 7 }], total: 1, next_cursor: null });
    await expect(listWorkspaceMembers(WORKSPACE, TOKEN)).rejects.toThrow("workspace member is invalid");
  });
});
