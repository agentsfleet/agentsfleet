import { describe, expect, it } from "vitest";
import { CURATED_ERROR_CODES, presentError, presentErrorString } from "./errors";

describe("presentError", () => {
  it("maps a known errorCode to the curated title + body", () => {
    const p = presentError({ errorCode: "UZ-AUTH-401", action: "load the dashboard" });
    expect(p.title).toBe("Your session expired");
    expect(p.body).toBe("Sign in again to keep going.");
    expect(p.code).toBe("UZ-AUTH-401");
  });

  it("falls back to verb + server message when the code is unknown but the message is usable", () => {
    const p = presentError({
      errorCode: "UZ-NEW-CODE",
      message: "trigger source must be set",
      action: "install the Fleet",
    });
    expect(p.title).toBe("Couldn't install the Fleet — trigger source must be set.");
    expect(p.code).toBe("UZ-NEW-CODE");
  });

  it("falls back to the default sentence when only the verb is known", () => {
    const p = presentError({ action: "load more events" });
    expect(p.title).toBe("Couldn't load more events. Try again, or check Events for what blocked it.");
    expect(p.code).toBeUndefined();
  });

  it("ignores a useless 'Failed to ...' server message in the fallback", () => {
    const p = presentError({ message: "Failed to delete Fleet", action: "delete this Fleet" });
    expect(p.title).toBe("Couldn't delete this Fleet. Try again, or check Events for what blocked it.");
  });
});

describe("presentErrorString", () => {
  it("joins title and body with a sentence-final period when the title lacks one", () => {
    const s = presentErrorString({ errorCode: "UZ-INTERNAL-002", action: "store the credential" });
    expect(s).toBe("We're under load and dropped your request. Try again in a few seconds.");
  });

  it("returns just the title when no body is provided", () => {
    const s = presentErrorString({ action: "kill this Fleet" });
    expect(s).toContain("Couldn't kill this Fleet");
  });

  // Invariant guard: every curated map title must NOT end in terminal
  // punctuation. presentErrorString unconditionally inserts `. ` between
  // title and body, so a title ending in `.`/`!`/`?` would double-period
  // the rendered sentence. This test fails loud the day a new map entry
  // breaks the invariant — iterating CURATED_ERROR_CODES (exported from
  // errors.ts) means new codes are auto-covered without touching the test.
  it("invariant: no curated map title ends in terminal punctuation", () => {
    for (const code of CURATED_ERROR_CODES) {
      const title = presentErrorString({ errorCode: code, action: "x" }).split(". ")[0];
      expect(title, `code=${code} title=${title}`).not.toMatch(/[.!?]$/);
    }
  });
});
