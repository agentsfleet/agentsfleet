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
    const s = presentErrorString({ errorCode: "UZ-AUTH-401", action: "load the dashboard" });
    expect(s).toBe("Your session expired. Sign in again to keep going.");
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

// The codes this session's audit curated (UZ-PROVIDER-*, UZ-VAULT-*,
// UZ-BUNDLE-*, UZ-APPROVAL-*, plus most of the pre-existing entries) moved
// to the backend's `user_message` (error_entries.zig's eu()) — see
// client.test.ts's "prefers user_message over detail" for the mechanism.
// CODE_MAP now holds only what can never be backend-authored.
describe("CODE_MAP — shrunk to client-minted + dead entries", () => {
  it("contains exactly the codes that cannot be backend-authored", () => {
    // UZ-AUTH-401/UZ-AUTH-022: client-minted (with-token.ts / require-scope.ts),
    // never round-trip to a real backend response for that code path.
    // UZ-VALIDATION-001/UZ-CRED-003: dead — no backend code, never client-minted.
    expect([...CURATED_ERROR_CODES].sort()).toEqual(
      ["UZ-AUTH-401", "UZ-AUTH-022", "UZ-CRED-003", "UZ-VALIDATION-001"].sort(),
    );
  });

  it("no longer maps any of the 26 codes migrated to the backend registry", () => {
    const migrated = [
      "UZ-INTERNAL-001", "UZ-INTERNAL-002", "UZ-AGT-009", "UZ-AUTH-001", "UZ-REQ-001",
      "UZ-APIKEY-003", "UZ-APIKEY-005", "UZ-APIKEY-006", "UZ-APIKEY-007", "UZ-APIKEY-008",
      "UZ-CRED-001", "UZ-PROVIDER-001", "UZ-PROVIDER-002", "UZ-PROVIDER-003", "UZ-PROVIDER-004",
      "UZ-VAULT-001", "UZ-VAULT-002", "UZ-VAULT-003", "UZ-BUNDLE-001", "UZ-BUNDLE-002",
      "UZ-APPROVAL-001", "UZ-APPROVAL-002", "UZ-APPROVAL-003", "UZ-APPROVAL-004",
      "UZ-APPROVAL-005", "UZ-APPROVAL-006",
    ];
    expect(migrated.length).toBe(26);
    for (const code of migrated) {
      expect((CURATED_ERROR_CODES as readonly string[]).includes(code), code).toBe(false);
    }
  });
});
