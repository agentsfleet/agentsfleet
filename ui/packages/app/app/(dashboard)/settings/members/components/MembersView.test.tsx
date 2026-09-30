import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";

// The server actions are this view's boundary: each is an RPC to the server,
// so they are what the tests answer for. Everything else renders for real.
const actions = vi.hoisted(() => ({
  loadTeamAction: vi.fn(),
  createInviteAction: vi.fn(),
  revokeInviteAction: vi.fn(),
  removeMemberAction: vi.fn(),
}));
vi.mock("../actions", () => actions);

import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import type { InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { MembersView } from "./MembersView";

const JOHN: MemberSummary = { user_id: "user_john", display_name: "John", email: "john@example.com", role: ACCOUNT_ROLE.owner };
const BOB: MemberSummary = { user_id: "user_bob", display_name: "Bob", email: "bob@example.com", role: ACCOUNT_ROLE.member };
const INVITE: InviteSummary = {
  id: "inv_1",
  email: "carol@example.com",
  role: ACCOUNT_ROLE.member,
  expires_at: Date.UTC(2026, 9, 7),
  created_at: Date.UTC(2026, 8, 30),
  link: "https://app.agentsfleet.net/invites/inv_1",
};
const LAST_OWNER = "The account's last owner cannot be removed.";

function renderView(members: MemberSummary[] = [JOHN, BOB], invites: InviteSummary[] = [INVITE]) {
  return render(<MembersView initialMembers={members} initialInvites={invites} />, { wrapper: TooltipProvider });
}

async function confirmIn(dialogButton: string) {
  const dialog = await screen.findByRole("alertdialog");
  await userEvent.setup().click(within(dialog).getByRole("button", { name: dialogButton }));
}

beforeEach(() => {
  actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN, BOB], invites: [INVITE] } });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("people", () => {
  it("should offer Remove for a member and never for the owner", () => {
    renderView();
    expect(screen.getByRole("button", { name: "Remove Bob" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Remove John" })).toBeNull();
  });

  it("should remove a member after confirmation and show the reloaded list", async () => {
    actions.removeMemberAction.mockResolvedValue({ ok: true, data: undefined });
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [INVITE] } });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: "Remove Bob" }));
    await confirmIn("Remove");
    await waitFor(() => expect(screen.queryByText("Bob")).toBeNull());
    expect(actions.removeMemberAction).toHaveBeenCalledExactlyOnceWith(BOB.user_id);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("should keep the dialog open with the backend's reason when removal is refused", async () => {
    actions.removeMemberAction.mockResolvedValue({ ok: false, status: 409, errorCode: "UZ-INV-004", error: LAST_OWNER });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: "Remove Bob" }));
    await confirmIn("Remove");
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain(LAST_OWNER.replace(/\.$/, "")));
    expect(screen.getByText("Bob")).toBeTruthy();
  });
});

describe("a member with no display name", () => {
  const DANA: MemberSummary = { user_id: "user_dana", display_name: null, email: "dana@example.com", role: ACCOUNT_ROLE.member };

  it("should name them by their address and leave them in place when the removal is cancelled", async () => {
    renderView([JOHN, DANA], []);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: `Remove ${DANA.email}` }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain(`Remove ${DANA.email}?`);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(actions.removeMemberAction).not.toHaveBeenCalled();
    expect(screen.getByText(DANA.email)).toBeTruthy();
  });
});

describe("pending invites", () => {
  it("should revoke an invite after confirmation and reload the lists", async () => {
    actions.revokeInviteAction.mockResolvedValue({ ok: true, data: undefined });
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN, BOB], invites: [] } });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: `Revoke invite for ${INVITE.email}` }));
    await confirmIn("Revoke");
    await waitFor(() => expect(screen.getByText("No pending invites")).toBeTruthy());
    expect(actions.revokeInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.id);
  });

  it("should keep the dialog open with the backend's reason when a revoke is refused", async () => {
    actions.revokeInviteAction.mockResolvedValue({ ok: false, status: 503, errorCode: "UZ-DB-001", error: "The database is unavailable." });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: `Revoke invite for ${INVITE.email}` }));
    await confirmIn("Revoke");
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain("Couldn't revoke the invite"));
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });

  it("should say so when the lists cannot be reloaded, rather than leave them silently stale", async () => {
    actions.revokeInviteAction.mockResolvedValue({ ok: true, data: undefined });
    actions.loadTeamAction.mockResolvedValue({ ok: false, status: 503, error: "Service unavailable" });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: `Revoke invite for ${INVITE.email}` }));
    await confirmIn("Revoke");
    await waitFor(() => expect(screen.getByText(/Couldn't reload this page's lists/)).toBeTruthy());
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });
});

describe("inviting", () => {
  it("should refuse something that is plainly not an address without calling the backend", async () => {
    renderView();
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Email"), "not-an-address");
    await user.click(screen.getByRole("button", { name: "Invite" }));
    await waitFor(() => expect(screen.getByText("Enter an email address")).toBeTruthy());
    expect(actions.createInviteAction).not.toHaveBeenCalled();
  });

  it("should send the trimmed address, then show the link to copy and reload the lists", async () => {
    actions.createInviteAction.mockResolvedValue({ ok: true, data: INVITE });
    renderView([JOHN], []);
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Email"), `  ${INVITE.email}  `);
    await user.click(screen.getByRole("button", { name: "Invite" }));
    const ready = await screen.findByTestId("invite-ready");
    expect(actions.createInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.email);
    expect(ready.textContent).toContain(INVITE.link);
    expect(within(ready).getByRole("button", { name: /Copy invite link/ })).toBeTruthy();
    expect(actions.loadTeamAction).toHaveBeenCalled();
  });

  it("should show the backend's refusal and no link when the invite is not created", async () => {
    actions.createInviteAction.mockResolvedValue({ ok: false, status: 409, errorCode: "UZ-INV-003", error: "That address already has a pending invite." });
    renderView();
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Email"), INVITE.email);
    await user.click(screen.getByRole("button", { name: "Invite" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("already has a pending invite");
    expect(screen.queryByTestId("invite-ready")).toBeNull();
  });
});
