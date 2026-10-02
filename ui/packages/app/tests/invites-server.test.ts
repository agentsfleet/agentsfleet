import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Only the identity provider, Next's cache and router hooks, and the network
// are stood in for; the action and loader run the real client and decoders.
const { credential, redirect, revalidatePath } = vi.hoisted(() => ({
  credential: vi.fn<() => Promise<string | null>>(),
  redirect: vi.fn((path: string) => {
    throw new Error(`NEXT_REDIRECT:${path}`);
  }),
  revalidatePath: vi.fn(),
}));
vi.mock("@/lib/auth/credential", () => ({
  credential,
  requireCredential: async () => (await credential()) ?? redirect("/sign-in"),
}));
vi.mock("next/navigation", () => ({ redirect }));
vi.mock("next/cache", () => ({ revalidatePath }));

import { acceptInviteAction } from "@/app/(dashboard)/invites/actions";
import { loadWaitingInvites } from "@/app/(dashboard)/invites/load";
import { SIGN_IN_PATH } from "@/lib/auth/sign-in-redirect";

const WAITING = { id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: 2 };

// A fresh Response per call: a body can be read once, and a page may read twice.
function answer(status: number, body?: unknown) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async () =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json" },
    }),
  );
}

beforeEach(() => credential.mockResolvedValue("tok_bob"));
afterEach(() => vi.restoreAllMocks());

describe("acceptInviteAction", () => {
  it("should revalidate the dashboard layout once the account is joined", async () => {
    answer(200, { tenant_id: "t_john", workspace_ids: ["ws_1"] });
    await expect(acceptInviteAction("inv_1")).resolves.toEqual({
      ok: true,
      data: { tenant_id: "t_john", workspace_ids: ["ws_1"] },
    });
    expect(revalidatePath).toHaveBeenCalledExactlyOnceWith("/", "layout");
  });

  it("should leave the layout alone when the accept is refused", async () => {
    answer(403, { title: "forbidden", status: 403, error_code: "UZ-INV-002", user_message: "Sign in with that address to accept it." });
    await expect(acceptInviteAction("inv_1")).resolves.toMatchObject({ ok: false, status: 403, errorCode: "UZ-INV-002" });
    expect(revalidatePath).not.toHaveBeenCalled();
  });
});

describe("loadWaitingInvites", () => {
  it("should return the invites waiting for the signed-in address", async () => {
    answer(200, { items: [WAITING], total: 1, next_cursor: null });
    await expect(loadWaitingInvites()).resolves.toEqual([WAITING]);
  });

  it("should send an expired session to sign in", async () => {
    answer(401, { title: "unauthorized", status: 401, error_code: "UZ-AUTH-002" });
    await expect(loadWaitingInvites()).rejects.toThrow(`NEXT_REDIRECT:${SIGN_IN_PATH}`);
  });

  it("should let any other failure reach the error boundary", async () => {
    answer(404, { title: "not found", status: 404, error_code: "UZ-REQ-404" });
    await expect(loadWaitingInvites()).rejects.toMatchObject({ status: 404 });
    expect(redirect).not.toHaveBeenCalled();
  });
});

describe("invite routes", () => {
  it("should hand an invite link's id to the view and no id on the plain page", async () => {
    const { default: InvitePage } = await import("@/app/(dashboard)/invites/[inviteId]/page");
    const { default: InvitesPage } = await import("@/app/(dashboard)/invites/page");
    answer(200, { items: [WAITING], total: 1, next_cursor: null });
    const linked = (await InvitePage({ params: Promise.resolve({ inviteId: "inv_9" }) })) as { props: unknown };
    expect(linked.props).toEqual({ waiting: [WAITING], linkedId: "inv_9" });
    const plain = (await InvitesPage()) as { props: unknown };
    expect(plain.props).toEqual({ waiting: [WAITING], linkedId: null });
  });
});
