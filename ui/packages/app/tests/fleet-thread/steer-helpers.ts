import { expect } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";
import { steerFleetActionMock } from "./harness";

// What the steer suites share: the composer's names and copy, the action's
// answers, and the moves a person makes in the composer.

import { COMPOSER_NAME, SEND_LABEL, UUID_V7 } from "./steer-copy";

export * from "./steer-copy";
export const OPERATION_ID = expect.stringMatching(UUID_V7);
// A definite refusal: the fleet will not take work, so nothing was admitted.
export const REFUSED = { ok: false, error: "Fleet is paused", status: 409, errorCode: "UZ-AGT-012" } as const;
// An answer that settles nothing: the daemon may hold the message.
export const UNAVAILABLE = { ok: false, error: "Provider unavailable", status: 503, errorCode: "UZ-API-002" } as const;
// The 202 as the daemon answers a fresh admission.
export const ACCEPTED = (eventId: string) => ({ ok: true, data: { status: "accepted", event_id: eventId, replayed: false } });

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
    () => new Promise((resolve) => { held.refuse = () => resolve(REFUSED); }),
  );
  return held;
}
