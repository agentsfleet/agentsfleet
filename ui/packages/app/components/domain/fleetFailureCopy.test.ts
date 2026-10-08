import { describe, expect, it } from "vitest";
import type { MessageState } from "@assistant-ui/react";
import {
  AGENTSFLEET_EVENT_STATUS,
  type FleetEvent,
} from "@/lib/streaming/fleet-stream-row";
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
const UNFINISHED_REPLY = "This fleet couldn’t complete the reply.";

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
    status: AGENTSFLEET_EVENT_STATUS.AGENT_ERROR,
    ...overrides,
  };
}

function failedMessage(
  failureDetail: string | null,
  outcome = "failed",
  failureLabel = "startup_posture",
): MessageState {
  return {
    content: [],
    metadata: {
      custom: { outcome, failureLabel, failureDetail },
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

  // Reported live on the dev fleet: the runner could not load its config inside
  // the sandbox, and the user was told their fleet lacked instructions — with a
  // raw error identifier pasted after the em-dash. The fleet was configured
  // correctly; the fault was entirely runner-side.
  it("test_unrecognised_cause_is_not_blamed_on_the_fleet: an internal identifier reads as a runner failure", () => {
    expect(eventOutcome(failedEvent({ failureDetail: "FleetInitFailed" }))).toBe(
      RUNNER_REFUSED,
    );
    expect(messageOutcome(failedMessage("FleetInitFailed"))).toBe(RUNNER_REFUSED);
  });

  it("test_raw_error_identifier_never_shown: no internal identifier survives into the sentence", () => {
    for (const identifier of [
      "FleetInitFailed",
      "SandboxEstablishFailed",
      "UZ-EXEC-012",
    ]) {
      const rendered = eventOutcome(failedEvent({ failureDetail: identifier }));
      expect(rendered).not.toContain(identifier);
      // It also must not leak through the em-dash-embedded path.
      expect(
        eventOutcome(failedEvent({ outcome: `failed — ${identifier}` })),
      ).not.toContain(identifier);
    }
  });

  it("test_missing_instructions_keeps_its_sentence: a genuine fleet-config cause still names the fleet", () => {
    // The inverse guard: prose causes are unaffected by the identifier rule, so
    // a fleet that really has no instructions still says so.
    expect(
      eventOutcome(failedEvent({ failureDetail: "no instructions configured" })),
    ).toBe(`${NEEDS_INSTRUCTIONS} — no instructions configured`);
    expect(eventOutcome(failedEvent())).toBe(NEEDS_INSTRUCTIONS);
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

  it("should keep runner details out of the chat outcome", () => {
    expect(messageOutcome(failedMessage("NoResponseContent", "failed", "runner_crash"))).toBe(UNFINISHED_REPLY);
    expect(eventOutcome(failedEvent({
      failureLabel: "runner_crash",
      failureDetail: "NoResponseContent",
    }))).toBe(UNFINISHED_REPLY);
    expect(eventOutcome(failedEvent({
      failureLabel: "runner_crash",
      failureDetail: null,
      outcome: "The runner crashed — NoResponseContent",
    }))).toBe(UNFINISHED_REPLY);
    expect(eventOutcome(failedEvent({
      failureLabel: "runner_crash",
      failureDetail: "NoResponseContent: empty after retry",
    }))).toBe(UNFINISHED_REPLY);
    expect(eventOutcome(failedEvent({
      failureLabel: "runner_crash",
      failureDetail: "UnexpectedFault",
    }))).toBe(UNFINISHED_REPLY);
  });
});
