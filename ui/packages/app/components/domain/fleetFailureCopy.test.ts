import { describe, expect, it } from "vitest";
import type { MessageState } from "@assistant-ui/react";
import {
  AGENTSFLEET_EVENT_STATUS,
  type FleetEvent,
} from "@/lib/streaming/fleet-stream-frames";
import {
  eventOutcome,
  messageOutcome,
  RUNNER_REFUSAL_DETAILS,
} from "./fleetFailureCopy";

// The two startup_posture sentences this module must keep apart: the fleet
// that never got instructions, and the runner that refused before the fleet
// ever ran. Both spelled here verbatim so a rewording breaks a test.
const NEEDS_INSTRUCTIONS =
  "This fleet needs instructions before it can respond.";
const RUNNER_REFUSED = "The runner refused this run before the fleet started.";

function failedEvent(overrides: Partial<FleetEvent> = {}): FleetEvent {
  return {
    id: "evt-1",
    role: "system",
    actor: "system",
    text: "",
    reply: "",
    outcome: "failed",
    failureLabel: "startup_posture",
    failureDetail: null,
    createdAt: new Date(0),
    status: AGENTSFLEET_EVENT_STATUS.FAILED,
    ...overrides,
  };
}

function failedMessage(
  failureDetail: string | null,
  outcome = "failed",
): MessageState {
  return {
    content: [],
    metadata: {
      custom: { outcome, failureLabel: "startup_posture", failureDetail },
    },
  } as unknown as MessageState;
}

describe("fleetFailureCopy — startup_posture sentences", () => {
  it.each([...RUNNER_REFUSAL_DETAILS])(
    "reads a runner refusal from the detail: %s",
    (detail) => {
      expect(eventOutcome(failedEvent({ failureDetail: detail }))).toBe(
        `${RUNNER_REFUSED} — ${detail}`,
      );
    },
  );

  it("reads a runner refusal out of the em-dash in the outcome when no detail rides beside it", () => {
    const [detail] = RUNNER_REFUSAL_DETAILS;
    expect(eventOutcome(failedEvent({ outcome: `failed — ${detail}` }))).toBe(
      `${RUNNER_REFUSED} — ${detail}`,
    );
  });

  it("routes a message's refusal detail through the same sentence", () => {
    const [detail] = RUNNER_REFUSAL_DETAILS;
    expect(messageOutcome(failedMessage(detail))).toBe(
      `${RUNNER_REFUSED} — ${detail}`,
    );
  });

  it("keeps the needs-instructions sentence verbatim for a non-refusal detail", () => {
    expect(
      eventOutcome(failedEvent({ failureDetail: "no instructions configured" })),
    ).toBe(`${NEEDS_INSTRUCTIONS} — no instructions configured`);
  });

  it("keeps the needs-instructions sentence verbatim when no detail was recorded", () => {
    expect(eventOutcome(failedEvent())).toBe(NEEDS_INSTRUCTIONS);
    expect(messageOutcome(failedMessage(null))).toBe(NEEDS_INSTRUCTIONS);
  });

  it("leaves every other failure tag on the shared event-summary sentence", () => {
    expect(
      eventOutcome(
        failedEvent({ failureLabel: "oom_kill", failureDetail: "killed at 2 GiB" }),
      ),
    ).toBe("Ran out of memory — killed at 2 GiB");
  });
});
