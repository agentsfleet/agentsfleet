import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
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

// The supervisor's production source: where a lease is ended and with which
// cause line. Test files are left out, since a fixture may spell any line.
const SUPERVISOR_SRC = resolve(
  process.cwd(),
  "../../../rustd/crates/afr_supervisor/src",
);
const DETAIL_DECLARATION = /const (DETAIL_\w+): &str =\s*"([^"]*)"/g;
const DETAIL_NAME = /\b(DETAIL_\w+)\b/g;
const REFUSE_CALL = /\.refuse\([^;]*?,\s*(DETAIL_\w+)\s*\)/g;
const STARTUP_POSTURE_FAILURE =
  /failed\(\s*FailureClass::StartupPosture,\s*(DETAIL_\w+)\s*,?\s*\)/g;
const UNHOSTED_BODY =
  /fn unhosted\([\s\S]*?failed\(FailureClass::StartupPosture, detail\)/;

function supervisorSource(): string {
  return readdirSync(SUPERVISOR_SRC, { recursive: true, encoding: "utf8" })
    .filter((name) => name.endsWith(".rs"))
    .filter((name) => !/(^|\/)(tests?|[\w]+_tests|test_support)(\.rs|\/)/.test(name))
    .map((name) => readFileSync(join(SUPERVISOR_SRC, name), "utf8"))
    .join("\n");
}

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

  // The refusal list is a hand-copy of cause lines the runner emits, in another
  // language, matched by exact string. Nothing but this test connects the two:
  // reword a line on the runner side and every refusal silently reverts to
  // "this fleet needs instructions" — the exact bug the split was written to
  // fix, reappearing with no failing test to announce it.
  //
  // Derived from the runner source rather than from a second copy of the list,
  // so the assertion cannot pass by agreeing with itself.
  it("carries exactly the runner's own startup-posture refusal lines", () => {
    const supervisor = supervisorSource();

    // Every `DETAIL_*` literal the supervisor declares, by name; a long one
    // wraps onto the next line after its `=`.
    const literals = new Map<string, string>();
    for (const match of supervisor.matchAll(DETAIL_DECLARATION)) {
      const [, name, text] = match;
      if (name === undefined || text === undefined) continue;
      literals.set(name, text);
    }
    expect(literals.size).toBeGreaterThan(0);

    // Only the ones a lease ends with under `FailureClass::StartupPosture`. A
    // line ended under another class — `renewal_terminate`, say — is not a
    // refusal to start and must NOT appear in the chat copy's list.
    //
    // Three emission shapes, and the guard must know all three: `refuse`,
    // which hard-codes the class; `failed(FailureClass::StartupPosture, ..)`
    // named directly; and `unhosted`, which picks its line by what the fleet
    // named and then ends the lease under the class.
    const unhosted = supervisor.match(UNHOSTED_BODY)?.[0] ?? "";
    expect(unhosted, "the unhosted refusal moved or was renamed").not.toBe("");
    const refusals = new Set<string>();
    const named = [
      ...supervisor.matchAll(REFUSE_CALL),
      ...supervisor.matchAll(STARTUP_POSTURE_FAILURE),
      ...unhosted.matchAll(DETAIL_NAME),
    ];
    for (const match of named) {
      const name = match[1];
      if (name === undefined) continue;
      const literal = literals.get(name);
      expect(literal, `${name} is emitted but never declared`).toBeDefined();
      if (literal !== undefined) refusals.add(literal);
    }

    expect([...refusals].sort()).toEqual([...RUNNER_REFUSAL_DETAILS].sort());
  });

  // The internal-identifier rule keys on whitespace: operator-facing cause lines
  // are prose, a bare token is a leaked error name. That holds for every cause
  // the runner declares today — but nothing stopped a future one-word cause from
  // being added, at which point it would be silently suppressed AND misread as a
  // runner refusal. Derived from the runner source so the guard cannot drift.
  it("every runner cause line is prose, so the internal-identifier rule cannot misfire", () => {
    const offenders: string[] = [];
    for (const match of supervisorSource().matchAll(DETAIL_DECLARATION)) {
      const [, name, text] = match;
      if (name === undefined || text === undefined) continue;
      if (!/\s/.test(text)) offenders.push(`${name} = "${text}"`);
    }

    expect(
      offenders,
      "a single-word cause line would be treated as an internal identifier: " +
        "hidden from the user and reported as a runner refusal. Reword it as prose.",
    ).toEqual([]);
  });
});
