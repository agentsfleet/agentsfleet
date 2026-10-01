import React, { type ReactElement, type ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

// The layout's reads each stand in for a network call; what is under test is
// what the shell is handed when the invite read succeeds or fails, and that
// the layout and the Invites page share one read per request.
const { TOKEN, listWaitingInvitesCached } = vi.hoisted(() => ({
  TOKEN: "tok_bob",
  listWaitingInvitesCached: vi.fn(),
}));
vi.mock("@/lib/auth/credential", () => ({
  credential: async () => TOKEN,
  requireCredential: async () => TOKEN,
}));
vi.mock("next/navigation", () => ({ redirect: vi.fn() }));
vi.mock("@/lib/workspace", () => ({
  listTenantWorkspacesCached: () => Promise.resolve({ items: [], total: 0 }),
}));
vi.mock("@/lib/auth/platform", () => ({ readSessionScopes: () => Promise.resolve(new Set<string>()) }));
vi.mock("@/lib/api/tenant_billing", () => ({ getTenantBillingCached: () => Promise.resolve(null) }));
vi.mock("@/lib/invites", () => ({ listWaitingInvitesCached }));
vi.mock("@/components/layout/ShellFrame", () => ({
  ShellFrame: ({ children }: { children: ReactNode }) => children,
}));

import { ShellFrame } from "@/components/layout/ShellFrame";
import DashboardLayout from "@/app/(dashboard)/layout";
import { loadWaitingInvites } from "@/app/(dashboard)/invites/load";

const WAITING = [{ id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: 1 }];

function shellProps(tree: ReactNode): { waitingInvites?: unknown } {
  if (!React.isValidElement(tree)) throw new Error("layout returned no element");
  if (tree.type === ShellFrame) return tree.props as { waitingInvites?: unknown };
  return shellProps((tree.props as { children?: ReactNode }).children);
}

afterEach(() => vi.resetAllMocks());

describe("dashboard layout invites", () => {
  it("should hand the shell the invites waiting for the signed-in person", async () => {
    listWaitingInvitesCached.mockResolvedValue(WAITING);
    const tree = await DashboardLayout({ children: null });
    expect(shellProps(tree as ReactElement).waitingInvites).toEqual(WAITING);
    expect(listWaitingInvitesCached).toHaveBeenCalledExactlyOnceWith(TOKEN);
  });

  it("should render the dashboard with no notice when the invite read fails", async () => {
    listWaitingInvitesCached.mockRejectedValue(new Error("invites endpoint down"));
    const tree = await DashboardLayout({ children: null });
    expect(shellProps(tree as ReactElement).waitingInvites).toEqual([]);
  });

  // React's `cache()` answers the second read of one request from the first,
  // so both must go through the same cached function.
  it("should read the waiting invites through the one per-request cache the Invites page reads too", async () => {
    listWaitingInvitesCached.mockResolvedValue(WAITING);
    await DashboardLayout({ children: null });
    await expect(loadWaitingInvites()).resolves.toEqual(WAITING);
    expect(listWaitingInvitesCached.mock.calls).toEqual([[TOKEN], [TOKEN]]);
  });
});
