import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";

// The accept action is the view's server boundary and the router is Next's;
// both are answered here, everything else renders for real.
const { acceptInviteAction, push } = vi.hoisted(() => ({ acceptInviteAction: vi.fn(), push: vi.fn() }));
vi.mock("../actions", () => ({ acceptInviteAction }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ push }) }));

import type { WaitingInvite } from "@/lib/api/invites";
import { accountLabel } from "@/components/layout/workspace-groups";
import { DASHBOARD_ROOT_PATH, DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import { InvitesView } from "./InvitesView";

const FROM_JOHN: WaitingInvite = { id: "inv_1", account: { tenant_id: "t_john", owner_name: "John" }, expires_at: Date.UTC(2026, 9, 7) };
const ACCEPT_JOHN = `Accept invite into ${accountLabel(FROM_JOHN.account.owner_name)}`;
const MISMATCH = "This invite was sent to a different email address. Sign in with that address to accept it.";
const MISMATCH_REFUSED = { ok: false, status: 403, errorCode: "UZ-INV-002", error: MISMATCH };
const LINKED_ID = "inv_from_link";
const FIRST_WORKSPACE = "ws_1";
const ACCEPT_LINKED = "Accept";
const LINKED_CARD = /You opened an invite link/;
const NOTHING_WAITS = "No invites waiting";

function renderView(waiting: WaitingInvite[], linkedId: string | null = null) {
  return render(<InvitesView waiting={waiting} linkedId={linkedId} />, { wrapper: TooltipProvider });
}

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("InvitesView", () => {
  it("should land the person in the joined account's first workspace after accepting", async () => {
    acceptInviteAction.mockResolvedValue({ ok: true, data: { tenant_id: FROM_JOHN.account.tenant_id, workspace_ids: [FIRST_WORKSPACE, "ws_2"] } });
    renderView([FROM_JOHN]);
    await userEvent.setup().click(screen.getByRole("button", { name: ACCEPT_JOHN }));
    await waitFor(() => expect(push).toHaveBeenCalledExactlyOnceWith(workspacePath(FIRST_WORKSPACE, DEFAULT_WORKSPACE_SUBPATH)));
    expect(acceptInviteAction).toHaveBeenCalledExactlyOnceWith(FROM_JOHN.id);
  });

  it("should go to the dashboard root when the joined account has no workspace yet", async () => {
    acceptInviteAction.mockResolvedValue({ ok: true, data: { tenant_id: FROM_JOHN.account.tenant_id, workspace_ids: [] } });
    renderView([FROM_JOHN]);
    await userEvent.setup().click(screen.getByRole("button", { name: ACCEPT_JOHN }));
    await waitFor(() => expect(push).toHaveBeenCalledExactlyOnceWith(DASHBOARD_ROOT_PATH));
  });

  it("should show the refusal and stay put when the invite is for another address", async () => {
    acceptInviteAction.mockResolvedValue(MISMATCH_REFUSED);
    renderView([FROM_JOHN]);
    await userEvent.setup().click(screen.getByRole("button", { name: ACCEPT_JOHN }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Sign in with that address");
    expect(push).not.toHaveBeenCalled();
  });

  it("should offer a linked invite the list does not hold, and let the accept call decide", async () => {
    acceptInviteAction.mockResolvedValue({ ok: false, status: 404, errorCode: "UZ-INV-001", error: "This invite has expired or was revoked." });
    renderView([], LINKED_ID);
    expect(screen.queryByText(NOTHING_WAITS)).toBeNull();
    await userEvent.setup().click(screen.getByRole("button", { name: ACCEPT_LINKED }));
    expect(acceptInviteAction).toHaveBeenCalledExactlyOnceWith(LINKED_ID);
    expect((await screen.findByRole("alert")).textContent).toContain("expired or was revoked");
  });

  it("should not add a separate linked card when the linked invite is already listed", () => {
    renderView([FROM_JOHN], FROM_JOHN.id);
    expect(screen.queryByText(LINKED_CARD)).toBeNull();
    expect(screen.getByRole("button", { name: ACCEPT_JOHN })).toBeTruthy();
  });

  it("should show the linked card above the list when other invites also wait", () => {
    renderView([FROM_JOHN], LINKED_ID);
    const card = screen.getByText(LINKED_CARD);
    const listed = screen.getByRole("button", { name: ACCEPT_JOHN });
    expect(screen.getByRole("button", { name: ACCEPT_LINKED })).toBeTruthy();
    expect(card.compareDocumentPosition(screen.getByRole("table")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(card.compareDocumentPosition(listed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText(NOTHING_WAITS)).toBeNull();
  });

  it("should hold every accept disabled and accept once when one is clicked twice while the request runs", async () => {
    const answer = Promise.withResolvers<unknown>();
    acceptInviteAction.mockReturnValue(answer.promise);
    renderView([FROM_JOHN], LINKED_ID);
    const user = userEvent.setup();
    // Queried fresh each time: a disabled row action re-mounts inside a wrapper.
    const listed = () => screen.getByRole("button", { name: ACCEPT_JOHN }) as HTMLButtonElement;
    const linked = () => screen.getByRole("button", { name: ACCEPT_LINKED }) as HTMLButtonElement;
    await user.click(listed());
    await waitFor(() => expect(listed().disabled).toBe(true));
    expect(linked().disabled).toBe(true);
    await user.click(listed());
    await user.click(linked());
    expect(acceptInviteAction).toHaveBeenCalledExactlyOnceWith(FROM_JOHN.id);

    answer.resolve(MISMATCH_REFUSED);
    await screen.findByRole("alert");
    // The refusal paints before the transition ends, so the buttons come back a beat later.
    await waitFor(() => expect([listed().disabled, linked().disabled]).toEqual([false, false]));
    expect(acceptInviteAction).toHaveBeenCalledOnce();
  });

  it("should say nothing waits when there is no invite and no link", () => {
    renderView([]);
    expect(screen.getByText(NOTHING_WAITS)).toBeTruthy();
  });
});
