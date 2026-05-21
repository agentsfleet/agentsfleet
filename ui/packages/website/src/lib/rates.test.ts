import { describe, expect, it } from "vitest";
import {
  EVENT_NANOS,
  NANOS_PER_USD,
  RATES_DISPLAY,
  STAGE_PLATFORM_NANOS,
  STAGE_SELF_MANAGED_NANOS,
  STARTER_CREDIT_NANOS,
} from "./rates";

/*
 * Pin tests — catch drift across the three surfaces that hand-type
 * usezombie rates: src/state/tenant_billing.zig (server authority),
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

  it("STAGE_PLATFORM_NANOS = 1_000_000 ($0.001)", () => {
    // pin test: literal is the contract
    expect(STAGE_PLATFORM_NANOS).toBe(1_000_000n);
  });

  it("STAGE_SELF_MANAGED_NANOS = 100_000 ($0.0001)", () => {
    // pin test: literal is the contract
    expect(STAGE_SELF_MANAGED_NANOS).toBe(100_000n);
  });

  it("NANOS_PER_USD = 1_000_000_000 (canonical billing unit)", () => {
    // pin test: literal is the contract
    expect(NANOS_PER_USD).toBe(1_000_000_000n);
  });
});

describe("rate ladder invariants", () => {
  it("starter credit covers thousands of stages on either posture", () => {
    expect(STARTER_CREDIT_NANOS / STAGE_PLATFORM_NANOS).toBeGreaterThanOrEqual(1_000n);
    expect(STARTER_CREDIT_NANOS / STAGE_SELF_MANAGED_NANOS).toBeGreaterThanOrEqual(1_000n);
  });

  it("self-managed stage is 10× cheaper than platform stage (the gradient is the marketing message)", () => {
    expect(STAGE_PLATFORM_NANOS).toBe(STAGE_SELF_MANAGED_NANOS * 10n);
  });

  it("event is free; platform stage is the cheapest non-zero charge surface", () => {
    expect(EVENT_NANOS).toBe(0n);
    expect(STAGE_PLATFORM_NANOS).toBeGreaterThan(EVENT_NANOS);
    expect(STAGE_SELF_MANAGED_NANOS).toBeGreaterThan(EVENT_NANOS);
  });
});

describe("RATES_DISPLAY format contract (shipped to Mintlify snippet, OpenAPI, smoke selectors)", () => {
  it("STARTER_CREDIT renders as $5", () => {
    expect(RATES_DISPLAY.STARTER_CREDIT).toBe("$5");
  });

  it("EVENT_RATE renders as free (the rate is conceptually free, not zero-cents)", () => {
    expect(RATES_DISPLAY.EVENT_RATE).toBe("free");
  });

  it("STAGE_PLATFORM renders as $0.001", () => {
    expect(RATES_DISPLAY.STAGE_PLATFORM).toBe("$0.001");
  });

  it("STAGE_SELF_MANAGED renders as $0.0001", () => {
    expect(RATES_DISPLAY.STAGE_SELF_MANAGED).toBe("$0.0001");
  });
});

describe("free-trial display strings (banner + hero pill share a single date substring)", () => {
  // pin test: literal is the contract
  it("FREE_TRIAL_PILL renders the short pill string with the trial-end date", () => {
    expect(RATES_DISPLAY.FREE_TRIAL_PILL).toBe("Free until July 31, 2026");
  });

  it("FREE_TRIAL_BANNER starts with the same `Free until <date>` prefix", () => {
    expect(RATES_DISPLAY.FREE_TRIAL_BANNER).toMatch(/^Free until July 31, 2026 — /);
  });

  // Drift catcher: the pill and the banner both consume
  // FREE_TRIAL_END_DISPLAY internally. If a future edit hardcodes the date in
  // one without the other, this asserts both still share the same date
  // substring — the substring IS the contract here.
  it("pill and banner share the same trial-end date substring", () => {
    const TRIAL_END_DISPLAY = "July 31, 2026";
    expect(RATES_DISPLAY.FREE_TRIAL_PILL).toContain(TRIAL_END_DISPLAY);
    expect(RATES_DISPLAY.FREE_TRIAL_BANNER).toContain(TRIAL_END_DISPLAY);
  });
});
