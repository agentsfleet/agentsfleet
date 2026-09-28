import { expect } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";
import { steerFleetActionMock } from "./harness";

// What the steer suites share: the composer's names and copy, the action's
// answers, and the moves a person makes in the composer.

export const COMPOSER_NAME = "Message this fleet…";
export const SEND_FAILED_TEXT = "Message not sent.";
export const SEND_UNCONFIRMED_TEXT = "Couldn't confirm this message was sent.";
export const RESEND_LABEL = "Resend";
export const SEND_LABEL = "Send";
export const SIGN_IN_LABEL = "Sign in";
export const DISMISS_LABEL = "Dismiss";
export const NOTICES_LABEL = "Unsent messages";
export const UUID_V7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const OPERATION_ID = expect.stringMatching(UUID_V7);
export const UNAVAILABLE = { ok: false, error: "Provider unavailable", status: 503, errorCode: "UZ-AGT-503" } as const;
export const ACCEPTED = (eventId: string) => ({ ok: true, data: { event_id: eventId } });

export function composerInput(): HTMLTextAreaElement {
  return screen.getByRole("textbox", { name: COMPOSER_NAME }) as HTMLTextAreaElement;
}

// Through the composer, not `onNew`: the draft return under test is the
// composer's own.
export async function send(text: string): Promise<void> {
  fireEvent.change(composerInput(), { target: { value: text } });
  await act(async () => {
    // A string name matches whole, so this never picks Resend.
    fireEvent.click(screen.getByRole("button", { name: SEND_LABEL }));
  });
}

/** The operation id the action received on its `index`th call. */
export function operationIdOf(index: number): string {
  return String(steerFleetActionMock.mock.calls[index]?.[3]);
}

// A refusal the test releases when it chooses.
export function heldRefusal(): { refuse: () => void } {
  const held = { refuse: () => {} };
  steerFleetActionMock.mockImplementationOnce(
    () => new Promise((resolve) => { held.refuse = () => resolve(UNAVAILABLE); }),
  );
  return held;
}
