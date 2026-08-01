import { describe, expect, it } from "vitest";
import {
  EVENT_NANOS,
  NANOS_PER_USD,
  RATES_DISPLAY,
  RUN_NANOS_PER_SEC,
  STARTER_CREDIT_NANOS,
} from "./rates";

/*
 * Pin tests — catch drift across the three surfaces that hand-type
 * agentsfleet rates: src/state/tenant_billing.zig (server authority),
 * this file (marketing mirror), ~/Projects/docs/snippets/rates.mdx
 * (Mintlify display snippet). Identifier names are identical across
 * Zig/TS/JS per the cross-tier parity rule, so a rename in any tier
 * surfaces here as a compile error rather than silent drift.
 *
 * Bug class: a contributor edits one tier without updating the other
 * two. The Zig sibling test ("rates pinned" in tenant_billing_test.zig)
 * locks the server-side numbers; this file locks the TS-side numbers
 * and the display strings against them.
 */

describe("rates pinned (regression — mirror src/state/tenant_billing_test.zig)", () => {
  // Bumping a rate fails this test AND the Zig sibling AND requires a
  // paired ~/Projects/docs/snippets/rates.mdx PR. The literal IS the
  // contract here — divergence between tiers mis-bills users vs. what
  // the site quotes.
  it("STARTER_CREDIT_NANOS = 5_000_000_000 ($5)", () => {
    // pin test: literal is the contract
    expect(STARTER_CREDIT_NANOS).toBe(5_000_000_000n);
  });

  it("EVENT_NANOS = 0 (free, both postures)", () => {
    // pin test: literal is the contract
    expect(EVENT_NANOS).toBe(0n);
  });

  it("RUN_NANOS_PER_SEC = RUN_NANOS_PER_SEC_EXPECTED ($0.0001/sec)", () => {
    // pin test: literal is the contract
    expect(RUN_NANOS_PER_SEC).toBe(100_000n);
  });

  it("NANOS_PER_USD = NANOS_PER_USD_EXPECTED (canonical billing unit)", () => {
    // pin test: literal is the contract
    expect(NANOS_PER_USD).toBe(1_000_000_000n);
  });
});

describe("rate ladder invariants", () => {
  it("starter credit covers thousands of seconds of runtime", () => {
    // $5 / $0.0001/sec = 50_000 seconds (~13.9 hours) of active runtime.
    expect(STARTER_CREDIT_NANOS / RUN_NANOS_PER_SEC).toBeGreaterThanOrEqual(1_000n);
  });

  it("the run rate is one value for both postures (no per-posture gradient)", () => {
    // pin test: literal is the contract — a single run rate, charged the same
    // whether platform or self-managed; only the model-token cost differs.
    expect(RUN_NANOS_PER_SEC).toBe(100_000n);
  });

  it("event is free; the run rate is the cheapest non-zero charge surface", () => {
    expect(EVENT_NANOS).toBe(0n);
    expect(RUN_NANOS_PER_SEC).toBeGreaterThan(EVENT_NANOS);
  });
});

describe("RATES_DISPLAY format contract (shipped to Mintlify snippet, OpenAPI, smoke selectors)", () => {
  it("STARTER_CREDIT renders as $5", () => {
    expect(RATES_DISPLAY.STARTER_CREDIT).toBe("$5");
  });

  it("EVENT_RATE renders as free (the rate is conceptually free, not zero-cents)", () => {
    expect(RATES_DISPLAY.EVENT_RATE).toBe("free");
  });

  it("RUN_RATE_PER_SEC renders as $0.0001/sec (the per-second billing unit)", () => {
    expect(RATES_DISPLAY.RUN_RATE_PER_SEC).toBe("$0.0001/sec");
  });

  it("RUN_RATE_PER_HOUR renders as $0.36/hr (the hourly equivalent)", () => {
    expect(RATES_DISPLAY.RUN_RATE_PER_HOUR).toBe("$0.36/hr");
  });
});

describe("free-trial display strings (open-ended trial — the copy names no date)", () => {
  // pin test: literal is the contract
  it("FREE_TRIAL_PILL renders the short pill string", () => {
    expect(RATES_DISPLAY.FREE_TRIAL_PILL).toBe("Free during early access");
  });

  it("FREE_TRIAL_BANNER opens with the phrase the pill uses", () => {
    expect(RATES_DISPLAY.FREE_TRIAL_BANNER).toMatch(/^Free during early access — /);
  });

  // Drift catcher, replacing the shared-date pin these two strings used to
  // carry. The trial boundary is a per-tenant column now (NULL = open-ended),
  // so this static page has no date it could state truthfully. It shipped
  // "Free until July 31, 2026" and went on saying it after that date passed —
  // any calendar date reappearing in this copy is that bug coming back.
  it("neither string states an end date", () => {
    const A_CALENDAR_DATE =
      /\b(January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4}\b|\b\d{4}-\d{2}-\d{2}\b/;
    expect(RATES_DISPLAY.FREE_TRIAL_PILL).not.toMatch(A_CALENDAR_DATE);
    expect(RATES_DISPLAY.FREE_TRIAL_BANNER).not.toMatch(A_CALENDAR_DATE);
  });
});
