import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { resolve } from "node:path";
import { EVENTS, EVENT_PROP_KEYS } from "../lib/analytics/events";

const APP_ROOT = resolve(__dirname, "..");

// Exact prop-key names that would indicate a secret-bearing payload. Event
// props carry IDs, names, and enums only; suffixed forms (api_key_id,
// credential_name, has_reason) are the allowed ID/name/flag shapes.
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

  it("event names follow snake_case object-action composition", () => {
    for (const name of Object.values(EVENTS)) {
      expect(name).toMatch(/^[a-z]+(_[a-z]+)+$/);
    }
  });

  it("no bare catalog event-name literal exists outside the catalog module", () => {
    // Same grep-gate shape as tests/grep-gates: the catalog is the single
    // source; every other reference goes through EVENTS.*. The pattern is
    // built from the catalog itself so this file carries no bare literal.
    const pattern = `"(${Object.values(EVENTS).join("|")})"`;
    let out = "";
    try {
      out = execSync(
        `grep -rnE --include='*.ts' --include='*.tsx' ` +
          `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
          `-- '${pattern}' .`,
        { cwd: APP_ROOT, encoding: "utf8" },
      );
    } catch (err) {
      if ((err as { status?: number }).status === 1) return; // zero matches anywhere
      throw err;
    }
    const offenders = out
      .split("\n")
      .filter((line) => line.trim().length > 0)
      .filter((line) => !line.startsWith("./lib/analytics/events.ts:"));
    expect(offenders, offenders.join("\n")).toEqual([]);
  });
});
