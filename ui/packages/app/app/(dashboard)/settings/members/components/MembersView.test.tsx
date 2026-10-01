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
  sendInviteEmailAction: vi.fn(),
}));
vi.mock("../actions", () => actions);
// The Invite trigger ships behind a next/dynamic shim; alias it back to the
// real dialog so the trigger and form mount synchronously.
vi.mock("@/components/domain/island-dynamic/InviteDialogDynamic", async () => ({
  default: (await vi.importActual<{ default: unknown }>("./InviteDialog")).default,
}));

import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import { EMAIL_STATUS, type InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { MembersView } from "./MembersView";

const JOHN_NAME = "John";
const BOB_NAME = "Bob";
const JOHN: MemberSummary = { user_id: "user_john", display_name: JOHN_NAME, email: "john@example.com", role: ACCOUNT_ROLE.owner, joined_at: Date.UTC(2026, 7, 1) };
const BOB: MemberSummary = { user_id: "user_bob", display_name: BOB_NAME, email: "bob@example.com", role: ACCOUNT_ROLE.member, joined_at: Date.UTC(2026, 8, 20) };
const INVITE: InviteSummary = {
  id: "inv_1",
  email: "carol@example.com",
  role: ACCOUNT_ROLE.member,
  expires_at: Date.UTC(2026, 9, 7),
  created_at: Date.UTC(2026, 8, 30),
  link: "https://app.agentsfleet.net/invites/inv_1",
  email_status: EMAIL_STATUS.sent,
  email_sent_at: Date.UTC(2026, 8, 30),
};
const UNSENT: InviteSummary = {
  ...INVITE,
  id: "inv_2",
  email: "dave@example.com",
  link: "https://app.agentsfleet.net/invites/inv_2",
  email_status: EMAIL_STATUS.failed,
  email_sent_at: null,
};
const NO_RELAY: InviteSummary = { ...UNSENT, id: "inv_3", email: "erin@example.com", email_status: EMAIL_STATUS.unconfigured };
const sendAgain = (invite: InviteSummary) => `Send the invite email to ${invite.email} again`;
const revokeFor = (invite: InviteSummary) => `Revoke invite for ${invite.email}`;
const LAST_OWNER = "The account's last owner cannot be removed.";
const LAST_OWNER_REFUSED = { ok: false, status: 409, errorCode: "UZ-INV-004", error: LAST_OWNER };
const SEND_REFUSED = { ok: false, status: 503, errorCode: "UZ-INV-005", error: "We could not send the email." };
const RELOAD_REFUSED = { ok: false, status: 503, error: "Service unavailable" };
const DONE = { ok: true, data: undefined };
const EMAIL_SENT = "Email sent";
const EMAIL_NOT_SENT = "Email not sent";
const CREATE_INVITE = "Create invite";
const EMAIL_FIELD = "Email";
const CANCEL_LABEL = "Cancel";
const INVITE_READY = "invite-ready";
const INVITED_BADGE = "invited";
const DUPLICATE_REFUSED = { ok: false, status: 409, errorCode: "UZ-INV-003", error: "That address already has a pending invite." };
const REVOKE_LABEL = "Revoke";
const REMOVE_LABEL = "Remove";
const REMOVE_BOB = `${REMOVE_LABEL} ${BOB_NAME}`;
const SEND_FAILED = /Couldn't send the invite email/;
const RELOAD_FAILED = /Couldn't reload this page's lists/;

function renderView(members: MemberSummary[] = [JOHN, BOB], invites: InviteSummary[] = [INVITE]) {
  return render(<MembersView initialMembers={members} initialInvites={invites} />, { wrapper: TooltipProvider });
}

const rowOf = (name: string) => screen.getByRole("row", { name: new RegExp(name) });

// An open confirm dialog hides the table from assistive tech, so its buttons
// are reached past that while a request runs behind the dialog.
const hiddenButton = (name: string) => screen.getByRole("button", { name, hidden: true }) as HTMLButtonElement;

async function confirmIn(dialogButton: string) {
  const dialog = await screen.findByRole("alertdialog");
  await userEvent.setup().click(within(dialog).getByRole("button", { name: dialogButton }));
}

async function openInvite() {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Invite" }));
  return { user, dialog: await screen.findByRole("dialog") };
}

beforeEach(() => {
  actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN, BOB], invites: [INVITE] } });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("the table", () => {
  it("should list people and pending invites in one table, each with its role", () => {
    renderView();
    expect(screen.getAllByRole("table")).toHaveLength(1);
    expect(within(rowOf(JOHN_NAME)).getByText(ACCOUNT_ROLE.owner)).toBeTruthy();
    expect(within(rowOf(BOB_NAME)).getByText(ACCOUNT_ROLE.member)).toBeTruthy();
    expect(within(rowOf(INVITE.email)).getByText(INVITED_BADGE)).toBeTruthy();
  });

  it("should say when each person joined and when each invite went out and lapses", () => {
    renderView();
    const stamps = (row: HTMLElement) => [...row.querySelectorAll("time")].map((time) => time.dateTime);
    const iso = (epochMs: number) => new Date(epochMs).toISOString();
    expect(stamps(rowOf(BOB_NAME))).toEqual([iso(BOB.joined_at)]);
    expect(stamps(rowOf(INVITE.email))).toEqual([iso(INVITE.created_at), iso(INVITE.expires_at)]);
    expect(rowOf(INVITE.email).textContent).toContain("expires");
  });

  it("should give an invite a copy and a revoke action, a member a remove, and the owner none", () => {
    renderView();
    expect(within(rowOf(INVITE.email)).getByRole("button", { name: `Copy invite link for ${INVITE.email}` })).toBeTruthy();
    expect(within(rowOf(INVITE.email)).getByRole("button", { name: `Revoke invite for ${INVITE.email}` })).toBeTruthy();
    expect(within(rowOf(BOB_NAME)).getByRole("button", { name: REMOVE_BOB })).toBeTruthy();
    expect(within(rowOf(JOHN_NAME)).queryAllByRole("button")).toHaveLength(0);
  });
});

describe("invite email", () => {
  it("should show each invite's email status, and offer send again only when it did not go", () => {
    renderView([JOHN], [INVITE, UNSENT, NO_RELAY]);
    expect(within(rowOf(INVITE.email)).getByText(EMAIL_SENT)).toBeTruthy();
    expect(within(rowOf(UNSENT.email)).getByText(EMAIL_NOT_SENT)).toBeTruthy();
    expect(within(rowOf(NO_RELAY.email)).getByText("Email not set up")).toBeTruthy();
    expect(within(rowOf(INVITE.email)).queryByRole("button", { name: sendAgain(INVITE) })).toBeNull();
    expect(within(rowOf(UNSENT.email)).getByRole("button", { name: sendAgain(UNSENT) })).toBeTruthy();
    expect(within(rowOf(NO_RELAY.email)).getByRole("button", { name: `Copy invite link for ${NO_RELAY.email}` })).toBeTruthy();
  });

  it("should send again and show the status the reloaded list carries", async () => {
    actions.sendInviteEmailAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [{ ...UNSENT, email_status: EMAIL_STATUS.sent }] } });
    renderView([JOHN], [UNSENT]);
    await userEvent.setup().click(screen.getByRole("button", { name: sendAgain(UNSENT) }));
    await waitFor(() => expect(within(rowOf(UNSENT.email)).getByText(EMAIL_SENT)).toBeTruthy());
    expect(actions.sendInviteEmailAction).toHaveBeenCalledExactlyOnceWith(UNSENT.id);
  });

  it("should announce a failed send-again as an alert when the email cannot be sent", async () => {
    actions.sendInviteEmailAction.mockResolvedValue(SEND_REFUSED);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [UNSENT] } });
    renderView([JOHN], [UNSENT]);
    await userEvent.setup().click(screen.getByRole("button", { name: sendAgain(UNSENT) }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(SEND_FAILED);
  });

  it("should say the email could not be sent, and still show it as not sent once the lists reload", async () => {
    actions.sendInviteEmailAction.mockResolvedValue(SEND_REFUSED);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [UNSENT] } });
    renderView([JOHN], [UNSENT]);
    const button = () => within(rowOf(UNSENT.email)).getByRole("button", { name: sendAgain(UNSENT) }) as HTMLButtonElement;
    await userEvent.setup().click(button());
    await waitFor(() => expect(screen.getByText(SEND_FAILED)).toBeTruthy());
    // Send again holds until the reload behind the refusal has landed.
    await waitFor(() => expect(button().disabled).toBe(false));
    expect(actions.loadTeamAction).toHaveBeenCalledOnce();
    expect(within(rowOf(UNSENT.email)).getByText(EMAIL_NOT_SENT)).toBeTruthy();
  });
});

describe("people", () => {
  it("should remove a member after confirmation and show the reloaded list", async () => {
    actions.removeMemberAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [INVITE] } });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: REMOVE_BOB }));
    await confirmIn(REMOVE_LABEL);
    await waitFor(() => expect(screen.queryByText(BOB_NAME)).toBeNull());
    expect(actions.removeMemberAction).toHaveBeenCalledExactlyOnceWith(BOB.user_id);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("should keep the dialog open with the backend's reason when removal is refused", async () => {
    actions.removeMemberAction.mockResolvedValue(LAST_OWNER_REFUSED);
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: REMOVE_BOB }));
    await confirmIn(REMOVE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain(LAST_OWNER.replace(/\.$/, "")));
    expect(screen.getByText(BOB_NAME)).toBeTruthy();
  });

  it("should keep the refusal reason when the reload after it also fails", async () => {
    actions.removeMemberAction.mockResolvedValue(LAST_OWNER_REFUSED);
    actions.loadTeamAction.mockResolvedValue(RELOAD_REFUSED);
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: REMOVE_BOB }));
    await confirmIn(REMOVE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    // The row's own button stays disabled until the reload settles, so once it
    // re-enables both the refusal and the failed reload have landed.
    await waitFor(() => expect(hiddenButton(REMOVE_BOB).disabled).toBe(false));
    expect(actions.loadTeamAction).toHaveBeenCalledOnce();
    expect(within(dialog).getByRole("alert").textContent).toContain(LAST_OWNER.replace(/\.$/, ""));
    expect(dialog.textContent).not.toMatch(RELOAD_FAILED);
  });
});

describe("a member with no display name", () => {
  const DANA: MemberSummary = { user_id: "user_dana", display_name: null, email: "dana@example.com", role: ACCOUNT_ROLE.member, joined_at: Date.UTC(2026, 8, 25) };

  it("should name them by their address and leave them in place when the removal is cancelled", async () => {
    renderView([JOHN, DANA], []);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: `Remove ${DANA.email}` }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain(`Remove ${DANA.email}?`);
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
    await userEvent.setup().click(screen.getByRole("button", { name: `Revoke invite for ${INVITE.email}` }));
    await confirmIn(REVOKE_LABEL);
    await waitFor(() => expect(screen.queryByText(INVITE.email)).toBeNull());
    expect(actions.revokeInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.id);
  });

  it("should keep the dialog open with the backend's reason when a revoke is refused", async () => {
    actions.revokeInviteAction.mockResolvedValue({ ok: false, status: 503, errorCode: "UZ-DB-001", error: "The database is unavailable." });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: `Revoke invite for ${INVITE.email}` }));
    await confirmIn(REVOKE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain("Couldn't revoke the invite"));
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });

  it("should say so when the lists cannot be reloaded, rather than leave them silently stale", async () => {
    actions.revokeInviteAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue(RELOAD_REFUSED);
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: `Revoke invite for ${INVITE.email}` }));
    await confirmIn(REVOKE_LABEL);
    await waitFor(() => expect(screen.getByText(RELOAD_FAILED)).toBeTruthy());
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });
});

// Each action's request is held open so the test can act while it runs, then
// answered to show the controls come back.
describe("a request in flight", () => {
  it("should hold send again disabled and send once when it is clicked twice while the email goes", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.sendInviteEmailAction.mockReturnValue(answer.promise);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [UNSENT] } });
    renderView([JOHN], [UNSENT]);
    const user = userEvent.setup();
    const button = () => screen.getByRole("button", { name: sendAgain(UNSENT) }) as HTMLButtonElement;
    await user.click(button());
    await waitFor(() => expect(button().disabled).toBe(true));
    await user.click(button());
    expect(actions.sendInviteEmailAction).toHaveBeenCalledExactlyOnceWith(UNSENT.id);

    answer.resolve(DONE);
    await waitFor(() => expect(button().disabled).toBe(false));
    expect(actions.sendInviteEmailAction).toHaveBeenCalledOnce();
  });

  // A confirm dialog's own button is the one a second click lands on, and the
  // row behind it holds too; both come back once the request answers.
  it.each([
    { what: "revoke", rowButton: revokeFor(INVITE), confirmLabel: REVOKE_LABEL, action: actions.revokeInviteAction, arg: INVITE.id },
    { what: "remove", rowButton: REMOVE_BOB, confirmLabel: REMOVE_LABEL, action: actions.removeMemberAction, arg: BOB.user_id },
  ])("should hold $what disabled and send it once when it is confirmed twice while the request runs", async ({ rowButton, confirmLabel, action, arg }) => {
    const answer = Promise.withResolvers<unknown>();
    action.mockReturnValue(answer.promise);
    renderView();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: rowButton }));
    const dialog = await screen.findByRole("alertdialog");
    const confirmButton = within(dialog).getByRole("button", { name: confirmLabel }) as HTMLButtonElement;
    await user.click(confirmButton);
    await waitFor(() => expect(confirmButton.disabled).toBe(true));
    await waitFor(() => expect(hiddenButton(rowButton).disabled).toBe(true));
    await user.click(confirmButton);
    expect(action).toHaveBeenCalledExactlyOnceWith(arg);

    answer.resolve(DONE);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await waitFor(() => expect((screen.getByRole("button", { name: rowButton }) as HTMLButtonElement).disabled).toBe(false));
    expect(action).toHaveBeenCalledOnce();
  });

  it("should hold create invite disabled and create once when it is submitted twice while the request runs", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.createInviteAction.mockReturnValue(answer.promise);
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), INVITE.email);
    const submit = within(dialog).getByRole("button", { name: CREATE_INVITE }) as HTMLButtonElement;
    await user.click(submit);
    await waitFor(() => expect(submit.disabled).toBe(true));
    await user.click(submit);
    expect(actions.createInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.email);

    answer.resolve(DUPLICATE_REFUSED);
    await within(dialog).findByRole("alert");
    // The refusal paints before the transition ends, so the button comes back a beat later.
    await waitFor(() => expect(submit.disabled).toBe(false));
    expect(actions.createInviteAction).toHaveBeenCalledOnce();
  });
});

describe("inviting", () => {
  it("should refuse something that is plainly not an address without calling the backend", async () => {
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), "not-an-address");
    await user.click(within(dialog).getByRole("button", { name: CREATE_INVITE }));
    await waitFor(() => expect(within(dialog).getByText("Enter an email address")).toBeTruthy());
    expect(actions.createInviteAction).not.toHaveBeenCalled();
  });

  it("should send the trimmed address, show the link to copy, list the reloaded invite, and close on Done", async () => {
    actions.createInviteAction.mockResolvedValue({ ok: true, data: INVITE });
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [INVITE] } });
    renderView([JOHN], []);
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), `  ${INVITE.email}  `);
    await user.click(within(dialog).getByRole("button", { name: CREATE_INVITE }));
    const ready = await screen.findByTestId(INVITE_READY);
    expect(actions.createInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.email);
    const field = within(ready).getByLabelText("Invite link") as HTMLInputElement;
    expect(field.value).toBe(INVITE.link);
    await user.click(field);
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, INVITE.link.length]);
    expect(within(ready).getByRole("button", { name: /Copy invite link/ })).toBeTruthy();

    await user.click(within(ready).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // The table opened with no invites, so this row can only come from the reload.
    await waitFor(() => expect(within(rowOf(INVITE.email)).getByText(INVITED_BADGE)).toBeTruthy());
    const reopened = (await openInvite()).dialog;
    expect((within(reopened).getByLabelText(EMAIL_FIELD) as HTMLInputElement).value).toBe("");
  });

  it("should show a spinner while the invite is being created", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.createInviteAction.mockReturnValue(answer.promise);
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), INVITE.email);
    await user.click(within(dialog).getByRole("button", { name: CREATE_INVITE }));
    await waitFor(() => expect(within(dialog).getByText("Creating")).toBeTruthy());
    answer.resolve({ ok: true, data: INVITE });
    await screen.findByTestId(INVITE_READY);
  });

  it("should show the backend's refusal and no link, and clear it when cancelled", async () => {
    actions.createInviteAction.mockResolvedValue(DUPLICATE_REFUSED);
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), INVITE.email);
    await user.click(within(dialog).getByRole("button", { name: CREATE_INVITE }));
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("already has a pending invite");
    expect(screen.queryByTestId(INVITE_READY)).toBeNull();

    await user.click(within(dialog).getByRole("button", { name: CANCEL_LABEL }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const reopened = (await openInvite()).dialog;
    expect(within(reopened).queryByRole("alert")).toBeNull();
  });
});
