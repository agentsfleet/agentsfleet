import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
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

  // The refusal list is a hand-copy of cause lines the runner emits, in another
  // language, matched by exact string. Nothing but this test connects the two:
  // reword a line on the runner side and every refusal silently reverts to
  // "this fleet needs instructions" — the exact bug the split was written to
  // fix, reappearing with no failing test to announce it.
  //
  // Derived from the runner source rather than from a second copy of the list,
  // so the assertion cannot pass by agreeing with itself.
  it("carries exactly the runner's own startup-posture refusal lines", () => {
    const runnerRoot = resolve(process.cwd(), "../../../src/runner");
    const sources = readdirSync(runnerRoot, { recursive: true, encoding: "utf8" })
      .filter((name) => name.endsWith(".zig"))
      .map((name) => readFileSync(join(runnerRoot, name), "utf8"));
    const all = sources.join("\n");

    // Every `DETAIL_*` literal the runner declares, by name.
    const literals = new Map<string, string>();
    for (const match of all.matchAll(/const (DETAIL_\w+) = "([^"]*)"/g)) {
      const name = match[1];
      const text = match[2];
      if (name === undefined || text === undefined) continue;
      literals.set(name, text);
    }
    expect(literals.size).toBeGreaterThan(0);

    // Only the ones emitted under `.startup_posture`. A cause line raised under
    // another class — `landlock_deny`, say — is not a refusal to start and must
    // NOT appear in the chat copy's list.
    const refusals = new Set<string>();
    for (const match of all.matchAll(
      /failedDetailed\([^,]+,\s*\.startup_posture,\s*(?:\w+\.)?(DETAIL_\w+)\s*\)/g,
    )) {
      const name = match[1];
      if (name === undefined) continue;
      const literal = literals.get(name);
      expect(literal, `${name} is emitted but never declared`).toBeDefined();
      if (literal !== undefined) refusals.add(literal);
    }

    expect([...refusals].sort()).toEqual([...RUNNER_REFUSAL_DETAILS].sort());
  });
});
