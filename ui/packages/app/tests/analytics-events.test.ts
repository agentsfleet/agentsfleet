import { describe, expect, it } from "vitest";
import { EVENTS, EVENT_PROP_KEYS } from "../lib/analytics/events";

// Exact prop-key names that would indicate a secret-bearing payload. Event
// props carry IDs, names, and enums only; suffixed forms (api_key_id,
// secret_name, has_reason) are the allowed ID/name/flag shapes.
const FORBIDDEN_PROP_KEYS = [
  "token",
  "key",
  "secret",
  "password",
  "api_key",
  "runner_token",
  "credential",
  "data_json",
  "email",
  "reason",
];

describe("analytics event catalog", () => {
  it("EVENT_PROP_KEYS mirrors the catalog exactly (every event, no extras)", () => {
    expect(Object.keys(EVENT_PROP_KEYS).sort()).toEqual(Object.values(EVENTS).sort());
  });

  it("no event prop key is a secret-bearing name", () => {
    for (const [event, keys] of Object.entries(EVENT_PROP_KEYS)) {
      for (const key of keys) {
        expect(FORBIDDEN_PROP_KEYS, `${event}.${key}`).not.toContain(key);
      }
    }
  });

  it("secret-hinting prop keys are only allowed in id/name/flag shapes", () => {
    // Exact-match alone misses composites (access_token, secret_value, …):
    // any key that hints at secret material must be one of the allowed
    // reference shapes — an id, a name, or a has_* flag.
    const SECRET_HINT = /(token|key|secret|password|credential|reason|email|json)/;
    const ALLOWED_SHAPE = /(_id|_name)$|^has_/;
    for (const [event, keys] of Object.entries(EVENT_PROP_KEYS)) {
      for (const key of keys) {
        if (SECRET_HINT.test(key)) {
          expect(ALLOWED_SHAPE.test(key), `${event}.${key}`).toBe(true);
        }
      }
    }
  });

  it("event names follow snake_case object-action composition", () => {
    for (const name of Object.values(EVENTS)) {
      expect(name).toMatch(/^[a-z]+(_[a-z]+)+$/);
    }
  });
});
