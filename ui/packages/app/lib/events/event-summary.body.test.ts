import { describe, expect, it } from "vitest";
import {
  ACTOR,
  EVENT_STATUS,
  HEADLINE,
  eventHeadlineFrom,
  outcomeFor,
  outcomeForCompletion,
  parsePayload,
  replyBodyFor,
  steerMessageFrom,
  triggerBodyFor,
} from "./event-summary";
import { ACCOUNT_ID, PLATFORM_IDENTITY, row } from "@/tests/helpers/event-summary-fixtures";

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

describe("outcomeForCompletion", () => {
  it("a completion carrying no cause reads as the status alone", () => {
    expect(outcomeForCompletion(EVENT_STATUS.PROCESSED, undefined, undefined)).toBe(
      outcomeForCompletion(EVENT_STATUS.PROCESSED, "", ""),
    );
    expect(outcomeForCompletion(EVENT_STATUS.FLEET_ERROR, undefined, undefined).length).toBeGreaterThan(0);
  });
});
