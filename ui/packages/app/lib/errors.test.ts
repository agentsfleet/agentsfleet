import { describe, expect, it } from "vitest";
import { presentError, presentErrorString } from "./errors";

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
      action: "install the zombie",
    });
    expect(p.title).toBe("Couldn't install the zombie — trigger source must be set.");
    expect(p.code).toBe("UZ-NEW-CODE");
  });

  it("falls back to the default sentence when only the verb is known", () => {
    const p = presentError({ action: "load more events" });
    expect(p.title).toBe("Couldn't load more events. Try again, or check Events for what blocked it.");
    expect(p.code).toBeUndefined();
  });

  it("ignores a useless 'Failed to ...' server message in the fallback", () => {
    const p = presentError({ message: "Failed to delete zombie", action: "delete this zombie" });
    expect(p.title).toBe("Couldn't delete this zombie. Try again, or check Events for what blocked it.");
  });
});

describe("presentErrorString", () => {
  it("joins title and body with a sentence-final period when the title lacks one", () => {
    const s = presentErrorString({ errorCode: "UZ-INTERNAL-002", action: "store the credential" });
    expect(s).toBe("We're under load and dropped your request. Try again in a few seconds.");
  });

  it("returns just the title when no body is provided", () => {
    const s = presentErrorString({ action: "kill this zombie" });
    expect(s).toContain("Couldn't kill this zombie");
  });
});
