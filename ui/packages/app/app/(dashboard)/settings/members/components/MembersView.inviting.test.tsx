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

import {
  BOB,
  CANCEL_LABEL,
  DONE,
  DUPLICATE_REFUSED,
  EMAIL_FIELD,
  INVITE,
  INVITE_READY,
  INVITED_BADGE,
  JOHN,
  REMOVE_BOB,
  REMOVE_LABEL,
  REVOKE_LABEL,
  SEND_INVITE,
  UNSENT,
  hiddenButton,
  openInvite,
  renderView,
  revokeFor,
  rowOf,
  sendAgain,
} from "@/tests/helpers/members-fixtures";

beforeEach(() => {
  actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN, BOB], invites: [INVITE] } });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
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

  it("should hold Send disabled and create once when it is submitted twice while the request runs", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.createInviteAction.mockReturnValue(answer.promise);
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), INVITE.email);
    const submit = within(dialog).getByRole("button", { name: SEND_INVITE }) as HTMLButtonElement;
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
    await user.click(within(dialog).getByRole("button", { name: SEND_INVITE }));
    await waitFor(() => expect(within(dialog).getByText("Enter an email address")).toBeTruthy());
    expect(actions.createInviteAction).not.toHaveBeenCalled();
  });

  it("should send the trimmed address, show the link to copy, list the reloaded invite, and close on Done", async () => {
    actions.createInviteAction.mockResolvedValue({ ok: true, data: INVITE });
    actions.loadTeamAction.mockResolvedValue({ ok: true, data: { members: [JOHN], invites: [INVITE] } });
    renderView([JOHN], []);
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), `  ${INVITE.email}  `);
    await user.click(within(dialog).getByRole("button", { name: SEND_INVITE }));
    const ready = await screen.findByTestId(INVITE_READY);
    expect(actions.createInviteAction).toHaveBeenCalledExactlyOnceWith(INVITE.email);
    const field = within(ready).getByLabelText("Invite link") as HTMLInputElement;
    expect(field.value).toBe(INVITE.link);
    await user.click(field);
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, INVITE.link.length]);
    expect(within(ready).getByRole("button", { name: /Copy invite link/ })).toBeTruthy();

    // The dialog holds while the list reload its create started is running, and
    // the table opened with no invites, so this row can only come from the reload.
    await waitFor(() => expect(hiddenButton(revokeFor(INVITE)).disabled).toBe(false));
    await user.click(within(ready).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(within(rowOf(INVITE.email)).getByText(INVITED_BADGE)).toBeTruthy();
    const reopened = (await openInvite()).dialog;
    expect((within(reopened).getByLabelText(EMAIL_FIELD) as HTMLInputElement).value).toBe("");
  });

  it("should show a spinner while the invite is being created", async () => {
    const answer = Promise.withResolvers<unknown>();
    actions.createInviteAction.mockReturnValue(answer.promise);
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), INVITE.email);
    await user.click(within(dialog).getByRole("button", { name: SEND_INVITE }));
    await waitFor(() => expect(within(dialog).getByText("Sending")).toBeTruthy());
    answer.resolve({ ok: true, data: INVITE });
    await screen.findByTestId(INVITE_READY);
  });

  it("should show the backend's refusal and no link, and clear it when cancelled", async () => {
    actions.createInviteAction.mockResolvedValue(DUPLICATE_REFUSED);
    renderView();
    const { user, dialog } = await openInvite();
    await user.type(within(dialog).getByLabelText(EMAIL_FIELD), INVITE.email);
    await user.click(within(dialog).getByRole("button", { name: SEND_INVITE }));
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("already has a pending invite");
    expect(screen.queryByTestId(INVITE_READY)).toBeNull();

    // Cancel holds until the list reload behind the refusal has landed.
    const cancel = within(dialog).getByRole("button", { name: CANCEL_LABEL }) as HTMLButtonElement;
    await waitFor(() => expect(cancel.disabled).toBe(false));
    await user.click(cancel);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const reopened = (await openInvite()).dialog;
    expect(within(reopened).queryByRole("alert")).toBeNull();
  });
});
