import React from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import { EMAIL_STATUS, type InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";
import { MembersView } from "@/app/(dashboard)/settings/members/components/MembersView";

// The Members page's people, invites, answers and words, shared by its suites.
// Each suite stands in for the server actions itself: `vi.mock` binds per file.

export const JOHN_NAME = "John";
export const BOB_NAME = "Bob";
export const JOHN: MemberSummary = { user_id: "user_john", display_name: JOHN_NAME, email: "john@example.com", role: ACCOUNT_ROLE.owner, joined_at: Date.UTC(2026, 7, 1) };
export const BOB: MemberSummary = { user_id: "user_bob", display_name: BOB_NAME, email: "bob@example.com", role: ACCOUNT_ROLE.member, joined_at: Date.UTC(2026, 8, 20) };
export const INVITE: InviteSummary = {
  id: "inv_1",
  email: "carol@example.com",
  role: ACCOUNT_ROLE.member,
  expires_at: Date.UTC(2026, 9, 7),
  created_at: Date.UTC(2026, 8, 30),
  link: "https://app.agentsfleet.net/invites/inv_1",
  email_status: EMAIL_STATUS.sent,
  email_sent_at: Date.UTC(2026, 8, 30),
};
export const UNSENT: InviteSummary = {
  ...INVITE,
  id: "inv_2",
  email: "dave@example.com",
  link: "https://app.agentsfleet.net/invites/inv_2",
  email_status: EMAIL_STATUS.failed,
  email_sent_at: null,
};
export const NO_RELAY: InviteSummary = { ...UNSENT, id: "inv_3", email: "erin@example.com", email_status: EMAIL_STATUS.unconfigured };

export const sendAgain = (invite: InviteSummary) => `Send the invite email to ${invite.email} again`;
export const revokeFor = (invite: InviteSummary) => `Revoke invite for ${invite.email}`;
export const copyLinkFor = (invite: InviteSummary) => `Copy invite link for ${invite.email}`;

export const LAST_OWNER = "The account's last owner cannot be removed.";
export const LAST_OWNER_REFUSED = { ok: false, status: 409, errorCode: "UZ-INV-004", error: LAST_OWNER };
export const SEND_REFUSED = { ok: false, status: 503, errorCode: "UZ-INV-005", error: "We could not send the email." };
export const RELOAD_REFUSED = { ok: false, status: 503, error: "Service unavailable" };
export const DUPLICATE_REFUSED = { ok: false, status: 409, errorCode: "UZ-INV-003", error: "That address already has a pending invite." };
export const DONE = { ok: true, data: undefined };

export const EMAIL_SENT = "Email sent";
export const EMAIL_NOT_SENT = "Email not sent";
export const EMAIL_NOT_SET_UP = "Email not set up";
export const INVITE_BUTTON = "Invite";
export const SEND_INVITE = "Send";
export const EMAIL_FIELD = "Email";
export const CANCEL_LABEL = "Cancel";
export const INVITE_READY = "invite-ready";
export const INVITED_BADGE = "invited";
export const REVOKE_LABEL = "Revoke";
export const REMOVE_LABEL = "Remove";
export const REMOVE_BOB = `${REMOVE_LABEL} ${BOB_NAME}`;
export const SEND_FAILED = /Couldn't send the invite email/;
export const RELOAD_FAILED = /Couldn't reload this page's lists/;

export function renderView(members: MemberSummary[] = [JOHN, BOB], invites: InviteSummary[] = [INVITE]) {
  return render(<MembersView initialMembers={members} initialInvites={invites} />, { wrapper: TooltipProvider });
}

export const rowOf = (name: string) => screen.getByRole("row", { name: new RegExp(name) });

// A row's cells, in the table's column order: person, role, time, actions.
const PERSON_COLUMN = 0;
const TIME_COLUMN = 2;
const ACTIONS_COLUMN = 3;
const cellOf = (name: string, column: number) => {
  const cell = within(rowOf(name)).getAllByRole("cell")[column];
  if (!cell) throw new Error(`the row for ${name} has no column ${column}`);
  return cell;
};
export const personCellOf = (name: string) => cellOf(name, PERSON_COLUMN);
export const timeCellOf = (name: string) => cellOf(name, TIME_COLUMN);
export const actionsCellOf = (name: string) => cellOf(name, ACTIONS_COLUMN);

// An open confirm dialog hides the table from assistive tech, so its buttons
// are reached past that while a request runs behind the dialog.
export const hiddenButton = (name: string) => screen.getByRole("button", { name, hidden: true }) as HTMLButtonElement;

export async function confirmIn(dialogButton: string) {
  const dialog = await screen.findByRole("alertdialog");
  await userEvent.setup().click(within(dialog).getByRole("button", { name: dialogButton }));
}

export async function openInvite() {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: INVITE_BUTTON }));
  return { user, dialog: await screen.findByRole("dialog") };
}
