import { describe, expect, it } from "vitest";
import type { MessageState } from "@assistant-ui/react";
import {
  AGENTSFLEET_EVENT_STATUS,
  type FleetEvent,
} from "@/lib/streaming/fleet-stream-row";
import { eventOutcome, messageOutcome } from "./fleetFailureCopy";

// The runner is the only writer of startup_posture, so every startup failure
// reads as its refusal, whatever cause line rides beside it.
const RUNNER_REFUSED = "The runner refused this run before the fleet started.";
const UNFINISHED_REPLY = "This fleet couldn’t complete the reply.";
// Cause lines in the runner's own shape: prose naming what it refused on.
const BUNDLE_REFUSAL = "the fleet bundle could not be fetched and verified";
const SANDBOX_REFUSAL = "this host could not build a sandbox for the run";
// A cause no runner build sends today, e.g. one carried by a pre-Rust row.
const UNLISTED_CAUSE = "no instructions configured";

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
  it.each([BUNDLE_REFUSAL, SANDBOX_REFUSAL, UNLISTED_CAUSE])(
    "reads every startup cause as the runner's refusal: %s",
    (detail) => {
      expect(eventOutcome(failedEvent({ failureDetail: detail }))).toBe(
        `${RUNNER_REFUSED} — ${detail}`,
      );
    },
  );

  it("reads the cause out of the em-dash in the outcome when no detail rides beside it", () => {
    expect(eventOutcome(failedEvent({ outcome: `failed — ${BUNDLE_REFUSAL}` }))).toBe(
      `${RUNNER_REFUSED} — ${BUNDLE_REFUSAL}`,
    );
  });

  it("routes a message's cause through the same sentence", () => {
    expect(messageOutcome(failedMessage(SANDBOX_REFUSAL))).toBe(
      `${RUNNER_REFUSED} — ${SANDBOX_REFUSAL}`,
    );
  });

  it("states the refusal alone when no cause was recorded", () => {
    expect(eventOutcome(failedEvent())).toBe(RUNNER_REFUSED);
    expect(messageOutcome(failedMessage(null))).toBe(RUNNER_REFUSED);
  });

  // Reported live on the dev fleet: the runner could not load its config inside
  // the sandbox, and a raw error identifier was pasted after the em-dash.
  it("test_raw_error_identifier_never_shown: no internal identifier survives into the sentence", () => {
    for (const identifier of [
      "FleetInitFailed",
      "SandboxEstablishFailed",
      "UZ-EXEC-012",
    ]) {
      expect(eventOutcome(failedEvent({ failureDetail: identifier }))).toBe(RUNNER_REFUSED);
      expect(messageOutcome(failedMessage(identifier))).toBe(RUNNER_REFUSED);
      // It also must not leak through the em-dash-embedded path.
      expect(
        eventOutcome(failedEvent({ outcome: `failed — ${identifier}` })),
      ).toBe(RUNNER_REFUSED);
    }
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
