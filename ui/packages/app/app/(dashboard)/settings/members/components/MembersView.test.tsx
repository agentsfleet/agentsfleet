import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
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

import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import { EMAIL_STATUS } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import {
  BOB,
  BOB_NAME,
  CANCEL_LABEL,
  DONE,
  EMAIL_NOT_SENT,
  EMAIL_NOT_SET_UP,
  EMAIL_SENT,
  INVITE,
  INVITED_BADGE,
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
  actionsCellOf,
  confirmIn,
  copyLinkFor,
  hiddenButton,
  personCellOf,
  renderView,
  revokeFor,
  rowOf,
  sendAgain,
  timeCellOf,
} from "@/tests/helpers/members-fixtures";
import { TEAM_CAPTION } from "./TeamTable";

const WARNING = /\btext-warning\b/;
const MUTED = /\btext-muted-foreground\b/;
// Shown on a phone only; the Time column and the actions' status hide there.
const PHONE_ONLY = '[class~="sm:hidden"]';
const PHONE_HIDDEN = '[class~="hidden"]';

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
    expect(stamps(timeCellOf(BOB_NAME))).toEqual([iso(BOB.joined_at)]);
    expect(stamps(timeCellOf(INVITE.email))).toEqual([iso(INVITE.created_at), iso(INVITE.expires_at)]);
    expect(timeCellOf(INVITE.email).textContent).toContain("expires");
  });

  it("should give an invite a copy and a revoke action, a member a remove, and the owner none", () => {
    renderView();
    expect(within(rowOf(INVITE.email)).getByRole("button", { name: copyLinkFor(INVITE) })).toBeTruthy();
    expect(within(rowOf(INVITE.email)).getByRole("button", { name: revokeFor(INVITE) })).toBeTruthy();
    expect(within(rowOf(BOB_NAME)).getByRole("button", { name: REMOVE_BOB })).toBeTruthy();
    expect(within(rowOf(JOHN_NAME)).queryAllByRole("button")).toHaveLength(0);
  });
});

describe("invite email", () => {
  it("should show each invite's email status, and offer send again only when the email failed", () => {
    renderView([JOHN], [INVITE, UNSENT, NO_RELAY]);
    expect(within(actionsCellOf(INVITE.email)).getByText(EMAIL_SENT)).toBeTruthy();
    expect(within(actionsCellOf(UNSENT.email)).getByText(EMAIL_NOT_SENT)).toBeTruthy();
    expect(within(actionsCellOf(NO_RELAY.email)).getByText(EMAIL_NOT_SET_UP)).toBeTruthy();
    expect(within(rowOf(INVITE.email)).queryByRole("button", { name: sendAgain(INVITE) })).toBeNull();
    expect(within(rowOf(UNSENT.email)).getByRole("button", { name: sendAgain(UNSENT) })).toBeTruthy();
    expect(within(rowOf(NO_RELAY.email)).getByRole("button", { name: copyLinkFor(NO_RELAY) })).toBeTruthy();
  });

  it("should send again and show the status the reloaded list carries", async () => {
    actions.sendInviteEmailAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [{ ...UNSENT, email_status: EMAIL_STATUS.sent }] } });
    renderView([JOHN], [UNSENT]);
    await userEvent.setup().click(screen.getByRole("button", { name: sendAgain(UNSENT) }));
    await waitFor(() => expect(within(actionsCellOf(UNSENT.email)).getByText(EMAIL_SENT)).toBeTruthy());
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
    expect(within(actionsCellOf(UNSENT.email)).getByText(EMAIL_NOT_SENT)).toBeTruthy();
  });
});

describe("an invite's email status", () => {
  it("should offer send again for an email that failed, and never for one with no relay to send it", () => {
    renderView([JOHN], [UNSENT, NO_RELAY]);
    expect(within(actionsCellOf(UNSENT.email)).getByRole("button", { name: sendAgain(UNSENT) })).toBeTruthy();
    expect(within(rowOf(NO_RELAY.email)).queryByRole("button", { name: sendAgain(NO_RELAY) })).toBeNull();
  });

  it("should read as a warning when the email did not go or cannot, and quietly once it went", () => {
    renderView([JOHN], [INVITE, UNSENT, NO_RELAY]);
    expect(within(actionsCellOf(INVITE.email)).getByText(EMAIL_SENT).className).toMatch(MUTED);
    expect(within(actionsCellOf(UNSENT.email)).getByText(EMAIL_NOT_SENT).className).toMatch(WARNING);
    expect(within(actionsCellOf(NO_RELAY.email)).getByText(EMAIL_NOT_SET_UP).className).toMatch(WARNING);
  });

  it("should explain on focus that this deployment sends no email, and to copy the link instead", async () => {
    renderView([JOHN], [NO_RELAY]);
    fireEvent.focus(within(actionsCellOf(NO_RELAY.email)).getByText(EMAIL_NOT_SET_UP));
    // pin test: literal is the contract — what the owner reads on hover or focus.
    const explained = "Email isn't set up for this deployment. Copy the link and share it instead.";
    await waitFor(() => expect(screen.getAllByText(explained).length).toBeGreaterThan(0));
  });
});

describe("a row at phone width", () => {
  it("should carry an invite's times and email status under its address, leaving the actions cell only icons", () => {
    renderView([JOHN], [UNSENT]);
    const phoneOnly = personCellOf(UNSENT.email).querySelector(PHONE_ONLY);
    expect(phoneOnly?.querySelectorAll("time")).toHaveLength(2);
    expect(phoneOnly?.textContent).toContain(EMAIL_NOT_SENT);
    expect(within(actionsCellOf(UNSENT.email)).getByText(EMAIL_NOT_SENT).closest(PHONE_HIDDEN)).not.toBeNull();
    expect(timeCellOf(UNSENT.email).matches(PHONE_HIDDEN)).toBe(true);
  });

  it("should carry a member's joined time under their name", () => {
    renderView();
    const phoneOnly = personCellOf(BOB_NAME).querySelector(PHONE_ONLY);
    expect([...(phoneOnly?.querySelectorAll("time") ?? [])].map((time) => time.dateTime)).toEqual([
      new Date(BOB.joined_at).toISOString(),
    ]);
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
    await userEvent.setup().click(screen.getByRole("button", { name: rowButton }));
    await confirmIn(confirmLabel);
    await waitFor(() => expect(screen.queryByText(gone)).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("region", { name: TEAM_CAPTION })));
  });
});

describe("a member with no display name", () => {
  const DANA: MemberSummary = { user_id: "user_dana", display_name: null, email: "dana@example.com", role: ACCOUNT_ROLE.member, joined_at: Date.UTC(2026, 8, 25) };

  it("should name them by their address and leave them in place when the removal is cancelled", async () => {
    renderView([JOHN, DANA], []);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: `${REMOVE_LABEL} ${DANA.email}` }));
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
    await userEvent.setup().click(screen.getByRole("button", { name: revokeFor(INVITE) }));
    await confirmIn(REVOKE_LABEL);
    await waitFor(() => expect(screen.queryByText(INVITE.email)).toBeNull());
    expect(actions.revokeInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.id);
  });

  it("should keep the dialog open with the backend's reason when a revoke is refused", async () => {
    actions.revokeInviteAction.mockResolvedValue({ ok: false, status: 503, errorCode: "UZ-DB-001", error: "The database is unavailable." });
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: revokeFor(INVITE) }));
    await confirmIn(REVOKE_LABEL);
    const dialog = await screen.findByRole("alertdialog");
    await waitFor(() => expect(dialog.textContent).toContain("Couldn't revoke the invite"));
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });

  it("should say so when the lists cannot be reloaded, rather than leave them silently stale", async () => {
    actions.revokeInviteAction.mockResolvedValue(DONE);
    actions.loadTeamAction.mockResolvedValue(RELOAD_REFUSED);
    renderView();
    await userEvent.setup().click(screen.getByRole("button", { name: revokeFor(INVITE) }));
    await confirmIn(REVOKE_LABEL);
    await waitFor(() => expect(screen.getByText(RELOAD_FAILED)).toBeTruthy());
    expect(screen.getByText(INVITE.email)).toBeTruthy();
  });
});
