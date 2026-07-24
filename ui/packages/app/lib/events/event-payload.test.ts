import { describe, expect, it } from "vitest";

import { eventLinkFrom, eventReferenceFrom } from "./event-payload";

const proposal = (over: Record<string, unknown> = {}) =>
  JSON.stringify({ action: "opened", repo: "o/r", number: 7, ...over });

describe("eventLinkFrom", () => {
  it("returns the provider link when the payload carries an absolute https URL", () => {
    expect(eventLinkFrom(proposal({ url: "https://github.com/o/r/pull/7" }))).toBe(
      "https://github.com/o/r/pull/7",
    );
  });

  it("accepts the completed-run link field too", () => {
    expect(
      eventLinkFrom(JSON.stringify({ run_url: "https://ci.example.test/run/1" })),
    ).toBe("https://ci.example.test/run/1");
  });

  it("refuses a non-http(s) scheme", () => {
    expect(eventLinkFrom(proposal({ url: "javascript:alert(1)" }))).toBeNull();
    expect(eventLinkFrom(proposal({ url: "data:text/html,x" }))).toBeNull();
  });

  it("refuses a value that is not an absolute URL at all", () => {
    // These throw inside `new URL()` (no base) — the parse guard catches and
    // moves on rather than letting a relative path resolve against the
    // console's own origin.
    expect(eventLinkFrom(proposal({ url: "//evil.example" }))).toBeNull();
    expect(eventLinkFrom(proposal({ url: "/pulls/7" }))).toBeNull();
    expect(eventLinkFrom(proposal({ url: "not a url at all" }))).toBeNull();
  });

  it("returns null when no link field is present", () => {
    expect(eventLinkFrom(proposal())).toBeNull();
    expect(eventLinkFrom(null)).toBeNull();
    expect(eventLinkFrom("not json")).toBeNull();
  });
});

describe("eventReferenceFrom", () => {
  it("reads the repository and number from a change proposal", () => {
    expect(eventReferenceFrom(proposal())).toBe("o/r#7");
  });

  it("returns null when the payload has no repository reference", () => {
    expect(eventReferenceFrom(JSON.stringify({ repo: "o/r" }))).toBeNull();
    expect(eventReferenceFrom(null)).toBeNull();
  });
});
