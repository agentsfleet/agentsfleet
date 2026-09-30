import React, { type ReactElement, type ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

// The layout's reads each stand in for a network call; what is under test is
// what the shell is handed when the invite read succeeds or fails.
const { listWaitingInvites } = vi.hoisted(() => ({ listWaitingInvites: vi.fn() }));
vi.mock("@/lib/auth/credential", () => ({ credential: async () => "tok_bob" }));
vi.mock("@/lib/workspace", () => ({
  listTenantWorkspacesCached: () => Promise.resolve({ items: [], total: 0 }),
}));
vi.mock("@/lib/auth/platform", () => ({ readSessionScopes: () => Promise.resolve(new Set<string>()) }));
vi.mock("@/lib/api/tenant_billing", () => ({ getTenantBillingCached: () => Promise.resolve(null) }));
vi.mock("@/lib/api/invites", () => ({ listWaitingInvites }));
vi.mock("@/components/layout/ShellFrame", () => ({
  ShellFrame: ({ children }: { children: ReactNode }) => children,
}));

import { ShellFrame } from "@/components/layout/ShellFrame";
import DashboardLayout from "@/app/(dashboard)/layout";

const WAITING = [{ id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: 1 }];

function shellProps(tree: ReactNode): { waitingInvites?: unknown } {
  if (!React.isValidElement(tree)) throw new Error("layout returned no element");
  if (tree.type === ShellFrame) return tree.props as { waitingInvites?: unknown };
  return shellProps((tree.props as { children?: ReactNode }).children);
}

afterEach(() => vi.resetAllMocks());

describe("dashboard layout invites", () => {
  it("should hand the shell the invites waiting for the signed-in person", async () => {
    listWaitingInvites.mockResolvedValue(WAITING);
    const tree = await DashboardLayout({ children: null });
    expect(shellProps(tree as ReactElement).waitingInvites).toEqual(WAITING);
    expect(listWaitingInvites).toHaveBeenCalledExactlyOnceWith("tok_bob");
  });

  it("should render the dashboard with no notice when the invite read fails", async () => {
    listWaitingInvites.mockRejectedValue(new Error("invites endpoint down"));
    const tree = await DashboardLayout({ children: null });
    expect(shellProps(tree as ReactElement).waitingInvites).toEqual([]);
  });
});
