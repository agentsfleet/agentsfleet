import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// The server actions are this view's boundary: each is an RPC to the server,
// so they are what the tests answer for. Everything else renders for real.
const actions = vi.hoisted(() => ({
  loadTeamAction: vi.fn(),
  createInviteAction: vi.fn(),
  revokeInviteAction: vi.fn(),
  removeMemberAction: vi.fn(),
  sendInviteEmailAction: vi.fn(),
}));
vi.mock("../actions", () => actions);
// The Invite trigger ships behind a next/dynamic shim; alias it back to the
// real dialog so the trigger and form mount synchronously.
vi.mock("@/components/domain/island-dynamic/InviteDialogDynamic", async () => ({
  default: (await vi.importActual<{ default: unknown }>("./InviteDialog")).default,
}));

import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import { EMAIL_STATUS } from "@/lib/api/invites-types";
import type { MemberSummary } from "@/lib/api/tenant-members";
import {
  BOB,
  BOB_NAME,
  CANCEL_LABEL,
  DONE,
  INVITE,
  JOHN,
  JOHN_NAME,
  LAST_OWNER,
  LAST_OWNER_REFUSED,
  NO_RELAY,
  RELOAD_FAILED,
  RELOAD_REFUSED,
  REMOVE_BOB,
  REMOVE_LABEL,
  REVOKE_LABEL,
  SEND_FAILED,
  SEND_REFUSED,
  UNSENT,
  confirmIn,
  chooseTeamAction,
  manageLabel,
  copyLinkFor,
  hiddenButton,
  renderView,
  revokeFor,
  rowOf,
  sendAgain,
} from "@/tests/helpers/members-fixtures";
import { TEAM_CAPTION } from "./TeamTable";

beforeEach(() => {
  actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN, BOB], invites: [INVITE] } });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("the approved team list", () => {
  it("keeps an account heading when its owner's name is unavailable", () => {
    renderView([{...JOHN, display_name: null}]);
    expect(screen.getByText("Your account")).toBeTruthy();
    expect(screen.getByText("You’re the owner")).toBeTruthy();
  });
  it("copies an invitation link without dismissing its menu or changing membership", async () => {
    const user = userEvent.setup();
    const copied = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    renderView();
    await user.click(screen.getByRole("button", {name: manageLabel(revokeFor(INVITE))}));
    await user.click(screen.getByRole("menuitem", {name: copyLinkFor(INVITE)}));
    expect(copied).toHaveBeenCalledWith(INVITE.link);
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(actions.revokeInviteAction).not.toHaveBeenCalled();
    copied.mockRestore();
  });
  it("shows names, roles and dates without confusing another owner with you", () => {
    renderView([JOHN, BOB, {...JOHN, user_id: "other_owner", display_name: "Dana", email: "dana@example.com"}]);
    expect(screen.getByRole("region", {name: "Members"})).toBeTruthy();
    expect(screen.getByRole("region", {name: "Pending invitations"})).toBeTruthy();
    expect(within(rowOf(JOHN_NAME)).getByText("You")).toBeTruthy();
    expect(within(rowOf("Dana")).queryByText("You")).toBeNull();
    expect(rowOf(BOB_NAME).textContent).toContain("member");
    expect(rowOf(BOB_NAME).querySelector("time")?.dateTime).toBe(BOB.joined_at);
    expect(rowOf(INVITE.email).querySelector("time")?.dateTime).toBe(INVITE.expires_at);
  });

  it("keeps member and invitation actions inside their own menus", async () => {
    renderView();
    expect(within(rowOf(JOHN_NAME)).queryAllByRole("button")).toHaveLength(0);
    expect(within(rowOf(BOB_NAME)).getAllByRole("button")).toHaveLength(1);
    await userEvent.setup().click(screen.getByRole("button", {name: manageLabel(revokeFor(INVITE))}));
    expect(screen.getByRole("menuitem", {name: "Send invite again"})).toBeTruthy();
    expect(screen.getByRole("menuitem", {name: copyLinkFor(INVITE)})).toBeTruthy();
    expect(screen.getByRole("menuitem", {name: "Revoke invite"})).toBeTruthy();
  });

  it("shows delivery failures and their next step without inventing successful delivery", async () => {
    renderView([JOHN], [INVITE, UNSENT, NO_RELAY]);
    expect(rowOf(INVITE.email).textContent).toContain("Invitation sent");
    expect(rowOf(UNSENT.email).textContent).toContain("Email not sent · Share the invite link");
    expect(rowOf(NO_RELAY.email).textContent).toContain("Email not set up · Share the invite link");
    await userEvent.setup().click(screen.getByRole("button", {name: manageLabel(revokeFor(NO_RELAY))}));
    expect(screen.queryByRole("menuitem", {name: "Send invite again"})).toBeNull();
    expect(screen.getByRole("menuitem", {name: copyLinkFor(NO_RELAY)})).toBeTruthy();
  });

  it("resends and uses the reloaded delivery status", async () => {
    actions.sendInviteEmailAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ok: true, data: {members: [JOHN], invites: [{...UNSENT, email_status: EMAIL_STATUS.sent}]}});
    renderView([JOHN], [UNSENT]);
    await chooseTeamAction(sendAgain(UNSENT));
    await waitFor(() => expect(rowOf(UNSENT.email).textContent).toContain("Invitation sent"));
    expect(actions.sendInviteEmailAction).toHaveBeenCalledExactlyOnceWith(UNSENT.id);
  });

  it("retains a failed resend and keeps its explanation after reloading", async () => {
    actions.sendInviteEmailAction.mockResolvedValue(SEND_REFUSED);
    actions.loadTeamAction.mockResolvedValue({ok: true, data: {members: [JOHN], invites: [UNSENT]}});
    renderView([JOHN], [UNSENT]);
    await chooseTeamAction(sendAgain(UNSENT));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(SEND_FAILED));
    await waitFor(() => expect(screen.getByRole("button", {name: manageLabel(sendAgain(UNSENT))})).toHaveProperty("disabled", false));
    expect(rowOf(UNSENT.email).textContent).toContain("Email not sent");
  });
});

describe("people", () => {
  it("should remove a member after confirmation and show the reloaded list", async () => {
    actions.removeMemberAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [INVITE] } });
    renderView();
    await chooseTeamAction(REMOVE_BOB);
    await confirmIn(REMOVE_LABEL);
    await waitFor(() => expect(screen.queryByText(BOB_NAME)).toBeNull());
    expect(actions.removeMemberAction).toHaveBeenCalledExactlyOnceWith(BOB.user_id);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("should keep the dialog open with the backend's reason when removal is refused", async () => {
    actions.removeMemberAction.mockResolvedValue(LAST_OWNER_REFUSED);
    renderView();
    await chooseTeamAction(REMOVE_BOB);
    await confirmIn(REMOVE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain(LAST_OWNER.replace(/\.$/, "")));
    expect(screen.getByText(BOB_NAME)).toBeTruthy();
  });

  it("should keep the refusal reason when the reload after it also fails", async () => {
    actions.removeMemberAction.mockResolvedValue(LAST_OWNER_REFUSED);
    actions.loadTeamAction.mockResolvedValue(RELOAD_REFUSED);
    renderView();
    await chooseTeamAction(REMOVE_BOB);
    await confirmIn(REMOVE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    // The row's own button stays disabled until the reload settles, so once it
    // re-enables both the refusal and the failed reload have landed.
    await waitFor(() => expect(hiddenButton(manageLabel(REMOVE_BOB)).disabled).toBe(false));
    expect(actions.loadTeamAction).toHaveBeenCalledOnce();
    expect(within(dialog).getByRole("alert").textContent).toContain(LAST_OWNER.replace(/\.$/, ""));
    expect(dialog.textContent).not.toMatch(RELOAD_FAILED);
  });
});

// The row whose button opened the dialog is gone once the lists reload, and
// focus must not fall to the page with it.
describe("focus after a removal", () => {
  it.each([
    { what: "a member is removed", rowButton: REMOVE_BOB, confirmLabel: REMOVE_LABEL, action: actions.removeMemberAction, gone: BOB_NAME, left: { members: [JOHN], invites: [INVITE] } },
    { what: "an invite is revoked", rowButton: revokeFor(INVITE), confirmLabel: REVOKE_LABEL, action: actions.revokeInviteAction, gone: INVITE.email, left: { members: [JOHN, BOB], invites: [] } },
  ])("should land on the team's table once $what", async ({ rowButton, confirmLabel, action, gone, left }) => {
    action.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: left });
    renderView();
    await chooseTeamAction(rowButton);
    await confirmIn(confirmLabel);
    await waitFor(() => expect(screen.queryByText(gone)).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("region", { name: TEAM_CAPTION })));
  });
});

describe("a member with no display name", () => {
  const DANA: MemberSummary = { user_id: "user_dana", display_name: null, email: "dana@example.com", role: ACCOUNT_ROLE.member, joined_at: new Date(Date.UTC(2026, 8, 25)).toISOString() };

  it("should name them by their address and leave them in place when the removal is cancelled", async () => {
    renderView([JOHN, DANA], []);
    const user = userEvent.setup();
    await chooseTeamAction(`${REMOVE_LABEL} ${DANA.email}`);
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain(`${REMOVE_LABEL} ${DANA.email}?`);
    await user.click(within(dialog).getByRole("button", { name: CANCEL_LABEL }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(actions.removeMemberAction).not.toHaveBeenCalled();
    expect(screen.getByText(DANA.email)).toBeTruthy();
  });
});

describe("pending invites", () => {
  it("should revoke an invite after confirmation and drop its row on reload", async () => {
    actions.revokeInviteAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN, BOB], invites: [] } });
    renderView();
    await chooseTeamAction(revokeFor(INVITE));
    await confirmIn(REVOKE_LABEL);
    await waitFor(() => expect(screen.queryByText(INVITE.email)).toBeNull());
    expect(actions.revokeInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.id);
  });

  it("should keep the dialog open with the backend's reason when a revoke is refused", async () => {
    actions.revokeInviteAction.mockResolvedValue({ ok: false, status: 503, errorCode: "UZ-DB-001", error: "The database is unavailable." });
    renderView();
    await chooseTeamAction(revokeFor(INVITE));
    await confirmIn(REVOKE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain("Couldn't revoke the invite"));
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });

  it("should say so when the lists cannot be reloaded, rather than leave them silently stale", async () => {
    actions.revokeInviteAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue(RELOAD_REFUSED);
    renderView();
    await chooseTeamAction(revokeFor(INVITE));
    await confirmIn(REVOKE_LABEL);
    await waitFor(() => expect(screen.getByText(RELOAD_FAILED)).toBeTruthy());
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });
});


it("searches members and pending invitations without changing account access or sending writes", async () => {
  renderView();
  const user = userEvent.setup();
  const search = screen.getByRole("searchbox", {name: "Search name or email"});
  await user.type(search, BOB_NAME);
  await waitFor(() => expect(screen.queryByRole("listitem", {name: JOHN_NAME})).toBeNull());
  expect(rowOf(BOB_NAME)).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Pending invitations"})).toBeNull();
  await user.click(screen.getByRole("button", {name: "Clear search"}));
  await user.type(search, INVITE.email.toUpperCase());
  await waitFor(() => expect(screen.queryByRole("listitem", {name: BOB_NAME})).toBeNull());
  expect(rowOf(INVITE.email)).toBeTruthy();
  await user.clear(search);
  await user.type(search, "no-person-matches");
  await screen.findByText("No matching people");
  expect(actions.removeMemberAction).not.toHaveBeenCalled();
  expect(actions.revokeInviteAction).not.toHaveBeenCalled();
  expect(actions.loadTeamAction).not.toHaveBeenCalled();
});
