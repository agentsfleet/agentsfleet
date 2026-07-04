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

// Every code named in the error-copy spec's reachable-surface audit maps to
// friendly copy that is NOT the raw backend detail string it was drafted from.
describe("presentErrorString — reachable-surface codes", () => {
  const RAW_BACKEND_DETAIL: Record<string, string> = {
    "UZ-PROVIDER-001": "PUT body must include `secret_ref` naming a vault credential when `mode` is self_managed.",
    "UZ-PROVIDER-002": "The named secret_ref has no vault row in the tenant's primary workspace.",
    "UZ-PROVIDER-003": "Stored credential JSON must include `provider` and `model`.",
    "UZ-PROVIDER-004": "The effective model is not present in core.model_caps.",
    "UZ-VAULT-001": "body must include a 'data' field that is a JSON object with at least one key.",
    "UZ-VAULT-002": "Stringified credential data exceeds 4KB.",
    "UZ-VAULT-003": "No credential matches this name in the workspace.",
    "UZ-BUNDLE-001": "The supplied Fleet Bundle is missing SKILL.md or contains unsafe, oversized, or malformed files.",
    "UZ-BUNDLE-002": "No installable template or stored snapshot matches the request in this workspace.",
    "UZ-APPROVAL-001": "Gate policy in TRIGGER.md config_json has invalid syntax.",
    "UZ-APPROVAL-002": "Approval action not found or already resolved.",
    "UZ-APPROVAL-003": "The approval callback signature is invalid.",
    "UZ-APPROVAL-004": "Gate service unavailable — default-deny applied.",
    "UZ-APPROVAL-005": "Gate condition expression is invalid.",
    "UZ-APPROVAL-006": "Resolved earlier by Slack, dashboard, or auto-timeout.",
  };

  it.each(Object.entries(RAW_BACKEND_DETAIL))("%s renders friendly copy, not the raw backend detail", (code, raw) => {
    const s = presentErrorString({ errorCode: code, message: raw, action: "x" });
    expect(s).not.toBe(raw);
    expect(s.length).toBeGreaterThan(0);
  });
});
