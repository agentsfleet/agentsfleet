import { describe, expect, it } from "vitest";
import {
  ACTOR,
  EVENT_STATUS,
  OUTCOME,
  SENDER,
  failurePresentationFor,
  failureSentenceFor,
  GUIDANCE,
  guidanceFor,
  outcomeFor,
  outcomeForStatus,
  roleFor,
  senderInitialsFor,
  senderLabelFor,
} from "./event-summary";
import { ACCOUNT_ID, PLATFORM_IDENTITY, row } from "@/tests/helpers/event-summary-fixtures";

const FLEET_NAME = "github-pr-reviewer";
const UNFINISHED_REPLY = "This fleet couldn’t complete the reply.";

describe("roleFor", () => {
  it("splits operator steers, fleet replies, and everything else", () => {
    expect(roleFor(`${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`)).toBe("user");
    expect(roleFor(ACTOR.FLEET)).toBe("assistant");
    expect(roleFor(PLATFORM_IDENTITY)).toBe("system");
    expect(roleFor("")).toBe("system");
  });

  it("unwraps a continuation to the turn it resumed", () => {
    expect(roleFor(`continuation:${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`)).toBe("user");
    expect(roleFor(`continuation:${ACTOR.FLEET}`)).toBe("assistant");
    expect(roleFor(`continuation:${PLATFORM_IDENTITY}`)).toBe("system");
  });
});

describe("senderLabelFor", () => {
  it("renders a word for an operator steer, never the account identifier", () => {
    const label = senderLabelFor(`${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`);
    expect(label).toBe(SENDER.OPERATOR);
    expect(label).not.toContain(ACCOUNT_ID);
  });

  it("distinguishes a programmatic steer from a human one", () => {
    expect(senderLabelFor(ACTOR.API_STEER)).toBe(SENDER.API);
  });

  it("labels the fleet with its own name, falling back when none is known", () => {
    expect(senderLabelFor(ACTOR.FLEET, FLEET_NAME)).toBe(FLEET_NAME);
    expect(senderLabelFor(ACTOR.FLEET)).toBe(SENDER.FLEET_FALLBACK);
    expect(senderLabelFor(ACTOR.FLEET, "")).toBe(SENDER.FLEET_FALLBACK);
  });

  it("names the source behind a prefixed webhook actor", () => {
    expect(senderLabelFor(`${ACTOR.WEBHOOK_PREFIX}slack`)).toBe("slack");
  });

  it("translates the runtime's own actors", () => {
    expect(senderLabelFor(ACTOR.CRON)).toBe(SENDER.SCHEDULE);
    expect(senderLabelFor(`${ACTOR.CRON}:nightly`)).toBe(SENDER.SCHEDULE);
    expect(senderLabelFor(ACTOR.CONTINUATION)).toBe(SENDER.CONTINUATION);
    expect(senderLabelFor(ACTOR.CONFIG_RELOAD)).toBe(SENDER.CONFIG_RELOAD);
    expect(senderLabelFor(ACTOR.GATE_BLOCKED)).toBe(SENDER.APPROVAL_GATE);
  });

  it("renders the GitHub platform identity readably, and an empty actor as a word", () => {
    expect(senderLabelFor(PLATFORM_IDENTITY)).toBe(SENDER.GITHUB_APP);
    expect(senderLabelFor("")).toBe(SENDER.UNKNOWN);
  });

  it("never leaks a raw account or member identifier for an unrecognised actor", () => {
    // A steer prefix an exact match missed, a continuation chain, a connector
    // actor: none may render the opaque id (Invariant 2).
    expect(senderLabelFor(`${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`)).toBe(SENDER.OPERATOR);
    expect(senderLabelFor(`continuation:${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`)).toBe(SENDER.OPERATOR);
    expect(senderLabelFor(ACCOUNT_ID)).toBe(SENDER.UNKNOWN);
    expect(senderLabelFor("sess_3GkbgXzXU5iLaYCt")).toBe(SENDER.UNKNOWN);
  });

  it("names the source of a connector actor and drops its member id", () => {
    expect(senderLabelFor("slack:U08ABCXYZ")).toBe("slack");
  });
});

describe("senderInitialsFor", () => {
  it("takes one letter per word, or the first two of a single word", () => {
    expect(senderInitialsFor(SENDER.OPERATOR)).toBe("OP");
    expect(senderInitialsFor(PLATFORM_IDENTITY)).toBe("GA");
    expect(senderInitialsFor(SENDER.CONFIG_RELOAD)).toBe("CR");
  });

  it("survives an empty label", () => {
    expect(senderInitialsFor("")).toBe("");
  });
});

describe("failureSentenceFor", () => {
  it("renders a sentence for a known runner failure", () => {
    expect(failureSentenceFor("startup_posture")).toBe("Failed a startup safety check");
  });

  it("renders the raw tag for a failure class the vocabulary has not caught up to", () => {
    expect(failureSentenceFor("brand_new_class")).toBe("brand_new_class");
    expect(failurePresentationFor("brand_new_class")).toEqual({
      label: "brand_new_class",
      guidance: null,
    });
  });

  it("only attaches startup guidance to the startup safety failure", () => {
    expect(failurePresentationFor("startup_posture").guidance).toBe("startup");
    expect(failurePresentationFor("budget_breach").guidance).toBeNull();
  });
});

describe("guidanceFor", () => {
  it("turns the startup guidance token into the line the operator can act on", () => {
    expect(guidanceFor("startup_posture")).toBe(GUIDANCE.STARTUP);
  });

  it("stays silent for classes with nothing actionable, and for no failure at all", () => {
    // A guidance line the operator cannot follow is noise, so these render none.
    expect(guidanceFor("oom_kill")).toBeNull();
    expect(guidanceFor("brand_new_class")).toBeNull();
    expect(guidanceFor(null)).toBeNull();
    expect(guidanceFor(undefined)).toBeNull();
    expect(guidanceFor("")).toBeNull();
  });
});

describe("outcomeFor", () => {
  it("states in-progress, approval-blocked, failed, and reply-less completion", () => {
    expect(outcomeFor(row({ status: EVENT_STATUS.RECEIVED }))).toBe(OUTCOME.WORKING);
    expect(outcomeFor(row({ status: EVENT_STATUS.GATE_BLOCKED }))).toBe(OUTCOME.WAITING_APPROVAL);
    expect(outcomeFor(row({ status: EVENT_STATUS.FLEET_ERROR }))).toBe(OUTCOME.FAILED);
    expect(outcomeFor(row({ status: EVENT_STATUS.PROCESSED }))).toBe(OUTCOME.COMPLETED);
  });

  it("prefers the failure sentence over the generic failed line", () => {
    expect(
      outcomeFor(row({ status: EVENT_STATUS.FLEET_ERROR, failure_label: "timeout_kill" })),
    ).toBe("Timed out");
  });

  it("keeps runner details out of the user-facing outcome", () => {
    expect(outcomeFor(row({
      status: EVENT_STATUS.FLEET_ERROR,
      failure_label: "runner_crash",
      failure_detail: "NoResponseContent",
    }))).toBe(UNFINISHED_REPLY);
    expect(outcomeFor(row({
      status: EVENT_STATUS.FLEET_ERROR,
      failure_label: "runner_crash",
      failure_detail: "NoResponseContent: empty after retry",
    }))).toBe(UNFINISHED_REPLY);
    expect(outcomeFor(row({
      status: EVENT_STATUS.FLEET_ERROR,
      failure_label: "runner_crash",
      failure_detail: "SegmentationFault",
    }))).toBe(UNFINISHED_REPLY);
  });

  it("shows a terminal gate refusal and its recovery instruction", () => {
    expect(
      outcomeFor(
        row({
          status: EVENT_STATUS.GATE_BLOCKED,
          failure_label: "repository_base_required",
          failure_detail: "Add x-agentsfleet.repository_base to TRIGGER.md, save the fleet, then retry the event.",
        }),
      ),
    ).toBe(
      "Fleet repository base is missing — Add x-agentsfleet.repository_base to TRIGGER.md, save the fleet, then retry the event.",
    );
  });

  it("appends the recorded cause line after the failure sentence", () => {
    expect(
      outcomeFor(
        row({
          status: EVENT_STATUS.FLEET_ERROR,
          failure_label: "startup_posture",
          failure_detail: "fleet has no instructions configured",
        }),
      ),
    ).toBe("Failed a startup safety check — fleet has no instructions configured");
  });

  it("does not repeat a cause that merely restates the sentence", () => {
    expect(
      outcomeFor(
        row({
          status: EVENT_STATUS.FLEET_ERROR,
          failure_label: "startup_posture",
          failure_detail: "Failed a startup safety check",
        }),
      ),
    ).toBe("Failed a startup safety check");
  });

  it("never returns an empty string for any status", () => {
    for (const status of ["received", "processed", "fleet_error", "gate_blocked", "invented"]) {
      expect(outcomeForStatus(status).length).toBeGreaterThan(0);
    }
  });
});
