import { describe, expect, it } from "vitest";
import {
  fallbackPersonLabel,
  isPersonId,
  shortenPersonId,
  systemLabel,
} from "@/lib/identity/person";

const SUBJECT = "user_3HizL5hdEfQ9Gy4e6Qsuq9nkKCu";
const SWEEPER = "system:approval_gate_sweeper";

describe("isPersonId", () => {
  it("accepts a Clerk subject", () => {
    expect(isPersonId(SUBJECT)).toBe(true);
  });

  // The daemon's own sentinels share the column but not the namespace, and
  // sending one to Clerk buys a round-trip for a 404.
  it("rejects the daemon's sentinels, the bare prefix and the empty string", () => {
    expect(isPersonId(SWEEPER)).toBe(false);
    expect(isPersonId("user_")).toBe(false);
    expect(isPersonId("")).toBe(false);
  });
});

describe("systemLabel", () => {
  it("names the sweeper as the closure nobody chose", () => {
    expect(systemLabel(SWEEPER)).toBe("Auto-swept");
  });

  // A sentinel this build has no arm for still reads as the daemon rather than
  // as a person, because that much is true from the prefix alone.
  it("falls back to System for an unknown sentinel", () => {
    expect(systemLabel("system:something_later")).toBe("System");
  });

  it("returns null for a person, who has a real name to find", () => {
    expect(systemLabel(SUBJECT)).toBeNull();
  });
});

describe("shortenPersonId", () => {
  it("keeps the head and tail so two subjects stay distinguishable", () => {
    const short = shortenPersonId(SUBJECT);
    expect(short).toBe("user_3HizL…kKCu");
    expect(short.length).toBeLessThan(SUBJECT.length);
  });

  it("leaves a string already short enough alone", () => {
    expect(shortenPersonId("user_abc")).toBe("user_abc");
  });
});

describe("fallbackPersonLabel", () => {
  // Dropping the only record of who decided is worse than printing it ugly.
  it("shows a subject the directory cannot resolve, shortened", () => {
    expect(fallbackPersonLabel(SUBJECT)).toBe("user_3HizL…kKCu");
  });

  it("prefers the sentinel's own words", () => {
    expect(fallbackPersonLabel(SWEEPER)).toBe("Auto-swept");
  });
});
