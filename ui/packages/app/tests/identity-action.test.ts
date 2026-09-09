import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { authMock, getUserListMock } = vi.hoisted(() => ({
  authMock: vi.fn(),
  getUserListMock: vi.fn(),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: authMock,
  clerkClient: () => Promise.resolve({ users: { getUserList: getUserListMock } }),
}));

import { resolvePeopleAction } from "@/app/actions/identity";

const ALICE = "user_3HizL5hdEfQ9Gy4e6Qsuq9nkKCu";
const SWEEPER = "system:approval_gate_sweeper";

function clerkUser(over: Record<string, unknown> = {}) {
  return {
    id: ALICE,
    fullName: "Ada Lovelace",
    username: "ada",
    primaryEmailAddress: { emailAddress: "ada@example.com" },
    ...over,
  };
}

beforeEach(() => {
  authMock.mockResolvedValue({ userId: "user_viewer" });
  getUserListMock.mockResolvedValue({ data: [clerkUser()] });
});

afterEach(() => {
  authMock.mockReset();
  getUserListMock.mockReset();
});

describe("resolvePeopleAction", () => {
  it("returns the name Clerk holds for a subject", async () => {
    expect(await resolvePeopleAction([ALICE])).toEqual({ [ALICE]: "Ada Lovelace" });
  });

  // The secret behind this lookup is the server's, so an unauthenticated caller
  // gets nothing rather than a directory.
  it("answers a signed-out caller with nothing, and never asks Clerk", async () => {
    authMock.mockResolvedValue({ userId: null });
    expect(await resolvePeopleAction([ALICE])).toEqual({});
    expect(getUserListMock).not.toHaveBeenCalled();
  });

  it("drops anything that is not a Clerk subject before asking", async () => {
    await resolvePeopleAction([ALICE, SWEEPER, ""]);
    expect(getUserListMock).toHaveBeenCalledWith({ userId: [ALICE], limit: 1 });
  });

  it("asks nothing when every actor is a sentinel", async () => {
    expect(await resolvePeopleAction([SWEEPER])).toEqual({});
    expect(getUserListMock).not.toHaveBeenCalled();
  });

  it("de-duplicates a table that asks about one person many times", async () => {
    await resolvePeopleAction([ALICE, ALICE, ALICE]);
    expect(getUserListMock).toHaveBeenCalledWith({ userId: [ALICE], limit: 1 });
  });

  // One page of Clerk's list. A caller that asks for more gets the cap, not an
  // unbounded fan-out on a request the client shaped.
  it("caps the batch at one page", async () => {
    const many = Array.from({ length: 150 }, (_, i) => `user_${i}aaaaaaaaaa`);
    await resolvePeopleAction(many);
    expect(getUserListMock.mock.calls[0]![0].userId).toHaveLength(100);
  });

  it("prefers the handle when there is no name, and the address when there is neither", async () => {
    getUserListMock.mockResolvedValue({ data: [clerkUser({ fullName: "  " })] });
    expect(await resolvePeopleAction([ALICE])).toEqual({ [ALICE]: "ada" });
    getUserListMock.mockResolvedValue({ data: [clerkUser({ fullName: null, username: null })] });
    expect(await resolvePeopleAction([ALICE])).toEqual({ [ALICE]: "ada@example.com" });
  });

  it("falls back to the shortened subject for an account with nothing to say", async () => {
    getUserListMock.mockResolvedValue({
      data: [clerkUser({ fullName: null, username: null, primaryEmailAddress: null })],
    });
    expect(await resolvePeopleAction([ALICE])).toEqual({ [ALICE]: "user_3HizL…kKCu" });
  });

  // A directory that will not answer is not a page that should fail.
  it("returns nothing when Clerk refuses, rather than throwing at the caller", async () => {
    getUserListMock.mockRejectedValue(new Error("clerk is down"));
    expect(await resolvePeopleAction([ALICE])).toEqual({});
  });
});
