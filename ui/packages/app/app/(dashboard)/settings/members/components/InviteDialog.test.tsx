import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";

// The create action is the dialog's one RPC; everything else renders for real.
const actions = vi.hoisted(() => ({ createInviteAction: vi.fn() }));
vi.mock("../actions", () => actions);

import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import { EMAIL_STATUS, type EmailStatus, type InviteSummary } from "@/lib/api/invites";
import InviteDialog from "./InviteDialog";

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

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

async function createWith(email_status: EmailStatus) {
  const email_sent_at = email_status === EMAIL_STATUS.sent ? INVITE.email_sent_at : null;
  actions.createInviteAction.mockResolvedValue({ ok: true, data: { ...INVITE, email_status, email_sent_at } });
  const onCreated = vi.fn();
  render(<InviteDialog onCreated={onCreated} />, { wrapper: TooltipProvider });
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Invite" }));
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByLabelText("Email"), INVITE.email);
  await user.click(within(dialog).getByRole("button", { name: "Send" }));
  const ready = await screen.findByTestId("invite-ready");
  expect(onCreated).toHaveBeenCalledOnce();
  return ready;
}

describe("the invite it just created", () => {
  it("should say the invitation was sent, and offer the link to share as well", async () => {
    const ready = await createWith(EMAIL_STATUS.sent);
    expect(within(ready).getByRole("heading", { name: "Invitation sent" })).toBeTruthy();
    expect(ready.textContent).toContain(`We emailed ${INVITE.email}. You can also copy the link and share it.`);
    expect((within(ready).getByLabelText("Invite link") as HTMLInputElement).value).toBe(INVITE.link);
  });

  it.each([
    [EMAIL_STATUS.failed, `The email to ${INVITE.email} did not go out.`],
    [EMAIL_STATUS.unconfigured, `This deployment sends no email. Copy the link and share it with ${INVITE.email}.`],
  ])("should not claim the email went out when its status is %s", async (status, lead) => {
    const ready = await createWith(status);
    expect(within(ready).getByRole("heading", { name: "Invite created" })).toBeTruthy();
    expect(ready.textContent).toContain(lead);
    expect(ready.textContent).not.toMatch(/\bsent\b|emailed/i);
    expect((within(ready).getByLabelText("Invite link") as HTMLInputElement).value).toBe(INVITE.link);
  });
});
