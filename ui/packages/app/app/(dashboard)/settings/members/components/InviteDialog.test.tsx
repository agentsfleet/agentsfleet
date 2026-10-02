import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
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
const INVITE_BUTTON = "Invite";
const EMAIL_FIELD = "Email";
const SEND = "Send";
const CANCEL = "Cancel";
const CLOSE = "Close";
const INVITE_READY = "invite-ready";
const INVITE_LINK = "Invite link";
const ENTER_AN_ADDRESS = "Enter an email address";
const DUPLICATE_REFUSED = { ok: false, status: 409, errorCode: "UZ-INV-003", error: "That address already has a pending invite." };
// pin test: 64 is the RFC 5321 limit the backend's mail parser enforces.
const LOCAL_PART_MAX = 64;
const addressWithLocalPart = (length: number) => `${"a".repeat(length)}@example.com`;

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

async function openDialog(onSettled = vi.fn()) {
  render(<InviteDialog onSettled={onSettled} />, { wrapper: TooltipProvider });
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: INVITE_BUTTON }));
  const dialog = await screen.findByRole("dialog");
  const send = async (address: string) => {
    const field = within(dialog).getByLabelText(EMAIL_FIELD);
    await user.clear(field);
    await user.type(field, address);
    await user.click(within(dialog).getByRole("button", { name: SEND }));
  };
  return { dialog, send, user };
}

async function createWith(email_status: EmailStatus) {
  const email_sent_at = email_status === EMAIL_STATUS.sent ? INVITE.email_sent_at : null;
  actions.createInviteAction.mockResolvedValue({ ok: true, data: { ...INVITE, email_status, email_sent_at } });
  const onSettled = vi.fn();
  const { send } = await openDialog(onSettled);
  await send(INVITE.email);
  const ready = await screen.findByTestId(INVITE_READY);
  expect(onSettled).toHaveBeenCalledOnce();
  return ready;
}

describe("the address it accepts", () => {
  it("should refuse a part before the @ longer than 64 characters without calling the backend", async () => {
    const { dialog, send } = await openDialog();
    await send(addressWithLocalPart(LOCAL_PART_MAX + 1));
    expect(await within(dialog).findByText(ENTER_AN_ADDRESS)).toBeTruthy();
    expect(actions.createInviteAction).not.toHaveBeenCalled();

    actions.createInviteAction.mockResolvedValue({ ok: true, data: INVITE });
    await send(addressWithLocalPart(LOCAL_PART_MAX));
    await screen.findByTestId(INVITE_READY);
    expect(actions.createInviteAction).toHaveBeenCalledExactlyOnceWith(addressWithLocalPart(LOCAL_PART_MAX));
  });
});

describe("the invite it just created", () => {
  it("should say the invite was sent, and offer the link to share as well", async () => {
    const ready = await createWith(EMAIL_STATUS.sent);
    expect(within(ready).getByRole("heading", { name: "Invite sent" })).toBeTruthy();
    expect(ready.textContent).toContain(`We emailed ${INVITE.email}. You can also copy the link and share it.`);
    expect((within(ready).getByLabelText(INVITE_LINK) as HTMLInputElement).value).toBe(INVITE.link);
  });

  it.each([
    [EMAIL_STATUS.failed, `The email to ${INVITE.email} did not go out.`],
    [EMAIL_STATUS.unconfigured, `This deployment sends no email. Copy the link and share it with ${INVITE.email}.`],
  ])("should not claim the email went out when its status is %s", async (status, lead) => {
    const ready = await createWith(status);
    expect(within(ready).getByRole("heading", { name: "Invite created" })).toBeTruthy();
    expect(ready.textContent).toContain(lead);
    expect(ready.textContent).not.toMatch(/\bsent\b|emailed/i);
    expect((within(ready).getByLabelText(INVITE_LINK) as HTMLInputElement).value).toBe(INVITE.link);
  });
});

describe("a create that settles", () => {
  it("should re-read the list after a refused create, since a refusal can follow a saved invite", async () => {
    actions.createInviteAction.mockResolvedValue(DUPLICATE_REFUSED);
    const onSettled = vi.fn();
    const { dialog, send } = await openDialog(onSettled);
    await send(INVITE.email);
    expect(await within(dialog).findByRole("alert")).toBeTruthy();
    expect(onSettled).toHaveBeenCalledOnce();
  });

  it("should stay open while a create is in flight, so its answer cannot land in the next invite", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.createInviteAction.mockReturnValue(answer.promise);
    const { dialog, send, user } = await openDialog();
    await send(INVITE.email);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBe(dialog);

    await act(async () => {
      answer.resolve({ ok: true, data: INVITE });
    });
    await screen.findByTestId(INVITE_READY);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("should disable the close X alongside Cancel while a create is in flight, and free both once it answers", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.createInviteAction.mockReturnValue(answer.promise);
    const { dialog, send, user } = await openDialog();
    await send(INVITE.email);
    const close = within(dialog).getByRole("button", { name: CLOSE }) as HTMLButtonElement;
    const cancel = within(dialog).getByRole("button", { name: CANCEL }) as HTMLButtonElement;
    expect(close.disabled).toBe(true);
    expect(cancel.disabled).toBe(true);

    // A refusal keeps the form up, so both buttons are still there to check.
    await act(async () => {
      answer.resolve(DUPLICATE_REFUSED);
    });
    await within(dialog).findByRole("alert");
    expect(close.disabled).toBe(false);
    expect(cancel.disabled).toBe(false);
    await user.click(close);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
