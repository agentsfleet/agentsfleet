import { describe, expect, it } from "vitest";
import { EVENTS } from "@/lib/analytics/events";

import { turnHeadlineFrom, turnLinkFrom, turnReferenceFrom } from "./turn-payload";

const proposal = (over: Record<string, unknown> = {}) =>
  JSON.stringify({ action: "opened", repo: "o/r", number: 7, ...over });

it("names a saved model change without exposing opaque model references", () => {
  expect(turnHeadlineFrom(JSON.stringify({model_ref: "owner-private-ref"}), EVENTS.model_changed)).toBe("Model changed");
  expect(turnHeadlineFrom(null, EVENTS.model_changed)).toBe("Model changed");
});

describe("turnLinkFrom", () => {
  it("returns the provider link when the payload carries an absolute https URL", () => {
    expect(turnLinkFrom(proposal({ url: "https://github.com/o/r/pull/7" }))).toBe(
      "https://github.com/o/r/pull/7",
    );
  });

  it("accepts the completed-run link field too", () => {
    expect(
      turnLinkFrom(JSON.stringify({ run_url: "https://ci.example.test/run/1" })),
    ).toBe("https://ci.example.test/run/1");
  });

  it("refuses a non-http(s) scheme", () => {
    expect(turnLinkFrom(proposal({ url: "javascript:alert(1)" }))).toBeNull();
    expect(turnLinkFrom(proposal({ url: "data:text/html,x" }))).toBeNull();
  });

  it("refuses a value that is not an absolute URL at all", () => {
    // These throw inside `new URL()` (no base) — the parse guard catches and
    // moves on rather than letting a relative path resolve against the
    // console's own origin.
    expect(turnLinkFrom(proposal({ url: "//evil.example" }))).toBeNull();
    expect(turnLinkFrom(proposal({ url: "/pulls/7" }))).toBeNull();
    expect(turnLinkFrom(proposal({ url: "not a url at all" }))).toBeNull();
  });

  it("returns null when no link field is present", () => {
    expect(turnLinkFrom(proposal())).toBeNull();
    expect(turnLinkFrom(null)).toBeNull();
    expect(turnLinkFrom("not json")).toBeNull();
  });
});

describe("turnReferenceFrom", () => {
  it("reads the repository and number from a change proposal", () => {
    expect(turnReferenceFrom(proposal())).toBe("o/r#7");
  });

  it("returns null when the payload has no repository reference", () => {
    expect(turnReferenceFrom(JSON.stringify({ repo: "o/r" }))).toBeNull();
    expect(turnReferenceFrom(null)).toBeNull();
  });
});
