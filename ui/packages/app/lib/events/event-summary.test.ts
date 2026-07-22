import { describe, expect, it } from "vitest";
import type { EventRow } from "@/lib/api/events";
import {
  ACTOR,
  EVENT_STATUS,
  HEADLINE,
  OUTCOME,
  SENDER,
  eventHeadlineFrom,
  failureSentenceFor,
  triggerBodyFor,
  replyBodyFor,
  outcomeFor,
  outcomeForStatus,
  parsePayload,
  roleFor,
  senderInitialsFor,
  senderLabelFor,
  steerMessageFrom,
} from "./event-summary";

const ACCOUNT_ID = "user_3gkbgxjnujsxbdxttcwcslpc87k";
const FLEET_NAME = "github-pr-reviewer";
const PLATFORM_IDENTITY = "github-app";

function row(over: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "e1",
    fleet_id: "f1",
    workspace_id: "ws1",
    actor: ACTOR.FLEET,
    event_type: "chat",
    status: EVENT_STATUS.PROCESSED,
    request_json: "{}",
    response_text: null,
    tokens: null,
    wall_ms: null,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: null,
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}

describe("roleFor", () => {
  it("splits operator steers, fleet replies, and everything else", () => {
    expect(roleFor(`${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`)).toBe("user");
    expect(roleFor(ACTOR.FLEET)).toBe("assistant");
    expect(roleFor(PLATFORM_IDENTITY)).toBe("system");
    expect(roleFor("")).toBe("system");
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
    expect(senderLabelFor(ACTOR.CONTINUATION)).toBe(SENDER.CONTINUATION);
    expect(senderLabelFor(ACTOR.CONFIG_RELOAD)).toBe(SENDER.CONFIG_RELOAD);
    expect(senderLabelFor(ACTOR.GATE_BLOCKED)).toBe(SENDER.APPROVAL_GATE);
  });

  it("renders an unknown platform identity as itself, and an empty actor as a word", () => {
    expect(senderLabelFor(PLATFORM_IDENTITY)).toBe(PLATFORM_IDENTITY);
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
  });
});

describe("outcomeFor", () => {
  it("states in-progress, approval-blocked, failed, and reply-less completion", () => {
    expect(outcomeFor(row({ status: EVENT_STATUS.RECEIVED }))).toBe(OUTCOME.WORKING);
    expect(outcomeFor(row({ status: EVENT_STATUS.GATE_BLOCKED }))).toBe(OUTCOME.WAITING_APPROVAL);
    expect(outcomeFor(row({ status: EVENT_STATUS.FLEET_ERROR }))).toBe(OUTCOME.FAILED);
    expect(outcomeFor(row({ status: EVENT_STATUS.PROCESSED }))).toBe(OUTCOME.NO_REPLY);
  });

  it("prefers the failure sentence over the generic failed line", () => {
    expect(
      outcomeFor(row({ status: EVENT_STATUS.FLEET_ERROR, failure_label: "timeout_kill" })),
    ).toBe("Timed out");
  });

  it("never returns an empty string for any status", () => {
    for (const status of ["received", "processed", "fleet_error", "gate_blocked", "invented"]) {
      expect(outcomeForStatus(status).length).toBeGreaterThan(0);
    }
  });
});

describe("parsePayload", () => {
  it("rejects absent, malformed, and non-object payloads", () => {
    expect(parsePayload(null)).toBeNull();
    expect(parsePayload("")).toBeNull();
    expect(parsePayload("{ not json")).toBeNull();
    expect(parsePayload("[1,2,3]")).toBeNull();
    expect(parsePayload("null")).toBeNull();
    expect(parsePayload('"a string"')).toBeNull();
  });

  it("returns the object for a well-formed payload", () => {
    expect(parsePayload('{"a":1}')).toEqual({ a: 1 });
  });
});

describe("steerMessageFrom", () => {
  it("recovers the operator's own submitted text", () => {
    expect(steerMessageFrom('{"message":"are you alive"}')).toBe("are you alive");
  });

  it("returns nothing for a payload with no message, or an unreadable one", () => {
    expect(steerMessageFrom('{"other":"x"}')).toBe("");
    expect(steerMessageFrom('{"message":42}')).toBe("");
    expect(steerMessageFrom("broken")).toBe("");
    expect(steerMessageFrom(null)).toBe("");
  });
});

describe("eventHeadlineFrom", () => {
  it("builds a change-proposal headline from repository, number, action, and title", () => {
    const payload = JSON.stringify({
      action: "opened",
      repo: "agentsfleet/agentsfleet",
      number: 539,
      title: "focus fleet details",
    });
    expect(eventHeadlineFrom(payload, "webhook")).toBe(
      "opened · agentsfleet/agentsfleet#539 — focus fleet details",
    );
  });

  it("omits the parts a change-proposal payload does not carry", () => {
    const payload = JSON.stringify({ repo: "owner/repo", number: 7 });
    expect(eventHeadlineFrom(payload, "webhook")).toBe("owner/repo#7");
  });

  it("builds a completed-run headline from name, conclusion, repository, and branch", () => {
    const payload = JSON.stringify({
      workflow_name: "ci.yml",
      conclusion: "failure",
      repo: "owner/repo",
      head_branch: "main",
    });
    expect(eventHeadlineFrom(payload, "webhook")).toBe("ci.yml failure · owner/repo · main");
  });

  it("drops the location clause when a completed run carries neither repository nor branch", () => {
    const payload = JSON.stringify({ workflow_name: "ci.yml", conclusion: "success" });
    expect(eventHeadlineFrom(payload, "webhook")).toBe("ci.yml success");
  });

  it("names what arrived when the payload shape is unrecognised or unreadable", () => {
    expect(eventHeadlineFrom('{"unknown":"shape"}', "webhook")).toBe("webhook received");
    expect(eventHeadlineFrom("not json", "cron")).toBe("cron received");
    expect(eventHeadlineFrom(null, "")).toBe(HEADLINE.EVENT_FALLBACK);
  });

  it("does not mistake a change proposal without a number for one", () => {
    expect(eventHeadlineFrom('{"repo":"owner/repo","action":"opened"}', "webhook")).toBe(
      "webhook received",
    );
  });
});

describe("triggerBodyFor / replyBodyFor", () => {
  it("the trigger is the operator's own text, never the fleet's reply on the same row", () => {
    const operator = row({
      actor: `${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`,
      request_json: '{"message":"hello"}',
      response_text: "the fleet answered on this same durable row",
    });
    // The two fields of one turn: the operator asked, the fleet answered — and
    // neither overwrites the other, so a reload shows both.
    expect(triggerBodyFor(operator)).toBe("hello");
    expect(replyBodyFor(operator)).toBe("the fleet answered on this same durable row");
  });

  it("a webhook row keeps its headline as the trigger and its reply separately", () => {
    const event = row({
      actor: PLATFORM_IDENTITY,
      event_type: "webhook",
      request_json: JSON.stringify({ repo: "owner/repo", number: 12, action: "closed" }),
      response_text: "I reviewed the change",
    });
    // The reply must NOT clobber the headline — the old code showed the reply
    // and dropped "closed · owner/repo#12" entirely.
    expect(triggerBodyFor(event)).toBe("closed · owner/repo#12");
    expect(replyBodyFor(event)).toBe("I reviewed the change");
  });

  it("an assistant-actor row has no trigger; its text is the reply", () => {
    expect(triggerBodyFor(row({ actor: ACTOR.FLEET }))).toBe("");
    expect(replyBodyFor(row({ actor: ACTOR.FLEET, response_text: " reviewed it " }))).toBe(
      "reviewed it",
    );
  });

  it("a reply-less operator turn leaves an empty reply and a non-empty outcome floor", () => {
    const operator = row({ actor: `${ACTOR.STEER_PREFIX}${ACCOUNT_ID}`, request_json: "{}" });
    expect(replyBodyFor(operator)).toBe("");
    expect(outcomeFor(operator).length).toBeGreaterThan(0);
  });
});
