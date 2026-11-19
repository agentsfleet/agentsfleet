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
  isSteerBy,
  outcomeFor,
  outcomeForStatus,
  roleFor,
  senderLabelFor,
  THREAD_STATUS,
} from "./event-summary";
import { ACCOUNT_ID, PLATFORM_IDENTITY, row } from "@/tests/helpers/event-summary-fixtures";
import { isNamedTeammate, nameSender, senderNamesFrom } from "./sender-names";

const FLEET_NAME = "github-pr-reviewer";
const UNFINISHED_REPLY = "This fleet couldn’t complete the reply.";
const SENDER_FLEET_ID = "0199a801-1000-7000-8000-0000000000c0";

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

describe("isSteerBy", () => {
  const own = `${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`;

  it("claims the viewer's own steer and a send not yet named", () => {
    expect(isSteerBy(own, ACCOUNT_ID)).toBe(true);
    expect(isSteerBy(ACTOR.PENDING_STEER, ACCOUNT_ID)).toBe(true);
    expect(isSteerBy(ACTOR.PENDING_STEER, null)).toBe(true);
  });

  it("disowns another operator, the API, a webhook, and any steer while signed out", () => {
    expect(isSteerBy(`${ACTOR.STEER_PREFIX}user_teammate`, ACCOUNT_ID)).toBe(false);
    // A subject that is a prefix of another's must not claim theirs.
    expect(isSteerBy(`${own}x`, ACCOUNT_ID)).toBe(false);
    expect(isSteerBy(ACTOR.API_STEER, ACCOUNT_ID)).toBe(false);
    expect(isSteerBy(PLATFORM_IDENTITY, ACCOUNT_ID)).toBe(false);
    expect(isSteerBy(own, null)).toBe(false);
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
    expect(senderLabelFor(`${ACTOR.WEBHOOK_PREFIX}slack`)).toBe("Slack");
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
    expect(senderLabelFor("slack:U08ABCXYZ")).toBe("Slack");
  });
});

describe("another fleet's steer", () => {
  it("event_summary_labels_a_fleet_steer_as_another_fleet", () => {
    const steer = `${ACTOR.FLEET_STEER_PREFIX}${SENDER_FLEET_ID}`;
    const names = senderNamesFrom(ACCOUNT_ID, []);
    expect(roleFor(steer)).toBe("user");
    expect(senderLabelFor(steer, FLEET_NAME)).toBe("Another fleet");
    expect(nameSender(steer, FLEET_NAME, names)).toBe("Another fleet");
    expect(isNamedTeammate(steer, names)).toBe(true);
    // A run that resumed the steer is still that fleet's turn.
    expect(roleFor(`continuation:${steer}`)).toBe("user");
    expect(senderLabelFor(`continuation:${steer}`)).toBe("Another fleet");
    // A person's steer and this fleet's own reply read as they did.
    const person = `${ACTOR.STEER_PREFIX}user_teammate`;
    expect(roleFor(person)).toBe("user");
    expect(senderLabelFor(person, FLEET_NAME)).toBe(SENDER.OPERATOR);
    expect(isNamedTeammate(person, names)).toBe(false);
    expect(roleFor(ACTOR.FLEET)).toBe("assistant");
    expect(senderLabelFor(ACTOR.FLEET, FLEET_NAME)).toBe(FLEET_NAME);
    expect(isNamedTeammate(ACTOR.FLEET, names)).toBe(false);
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

  it("should read a queued row as working when the message waits for a runner", () => {
    expect(outcomeFor(row({ status: THREAD_STATUS.QUEUED }))).toBe(OUTCOME.WORKING);
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
