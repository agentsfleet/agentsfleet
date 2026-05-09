import { describe, expect, it } from "vitest";
import { RATES_CENTS, RATES_DISPLAY, WORKED_EXAMPLE } from "./rates";

/*
 * Invariant suite — catches display ↔ cents drift across every surface
 * that hand-types the rate ($/¢): marketing site, OpenAPI description,
 * schema header comments, e2e selectors, internal docs.
 *
 * Bug class this catches: a contributor edits one of those four surfaces
 * (or this file) without updating the matching constant. The previous
 * "$0.001 per event receipt" typo passed unit tests in isolation because
 * Pricing.tsx and the smoke selector both rendered the *same* (wrong)
 * RATES_DISPLAY string — only this invariant binds the displayed string
 * to the underlying integer cents and to the shipped worked-example math.
 */

function formatCents(cents: number): string {
  if (cents === 0) return "$0";
  if (cents % 100 === 0) return `$${cents / 100}`;
  return `$${(cents / 100).toFixed(2)}`;
}

describe("rates pinned (regression — mirror src/state/tenant_billing_test.zig)", () => {
  // Bumping a rate fails both this test and the Zig sibling test
  // ("rates pinned" in tenant_billing_test.zig). Cross-stack update is the
  // intent: the Zig server is the authority, this file is the marketing
  // mirror, and a divergence between them mis-bills users vs. what the
  // site quotes. No silent drift.
  it("event = 1¢, stage = 10¢, starterCredit = 500¢", () => {
    expect(RATES_CENTS.event).toBe(1);
    expect(RATES_CENTS.stage).toBe(10);
    expect(RATES_CENTS.starterCredit).toBe(500);
  });
});

describe("RATES_DISPLAY mirrors RATES_CENTS", () => {
  it("should render RATES_CENTS.event as RATES_DISPLAY.event", () => {
    expect(RATES_DISPLAY.event).toBe(formatCents(RATES_CENTS.event));
  });

  it("should render RATES_CENTS.stage as RATES_DISPLAY.stage", () => {
    expect(RATES_DISPLAY.stage).toBe(formatCents(RATES_CENTS.stage));
  });

  it("should render RATES_CENTS.starterCredit as RATES_DISPLAY.starterCredit", () => {
    expect(RATES_DISPLAY.starterCredit).toBe(formatCents(RATES_CENTS.starterCredit));
  });
});

describe("RATES_CENTS boundary invariants", () => {
  it("should expose every rate as a positive integer number of cents", () => {
    for (const [key, value] of Object.entries(RATES_CENTS)) {
      expect.soft(Number.isInteger(value), `${key} not integer: ${value}`).toBe(true);
      expect.soft(value, `${key} not positive: ${value}`).toBeGreaterThan(0);
      expect.soft(Number.isFinite(value), `${key} not finite: ${value}`).toBe(true);
    }
  });

  it("should keep stage strictly more expensive than event (rate ladder invariant)", () => {
    expect(RATES_CENTS.stage).toBeGreaterThan(RATES_CENTS.event);
  });

  it("should grant a starter credit large enough to cover at least one event+stage cycle", () => {
    expect(RATES_CENTS.starterCredit).toBeGreaterThanOrEqual(
      RATES_CENTS.event + RATES_CENTS.stage,
    );
  });
});

function parseDollarString(str: string): number {
  const match = str.match(/^\$(\d+(?:\.\d{1,2})?)$/);
  if (!match) throw new Error(`unparseable dollar string: ${str}`);
  return Math.round(parseFloat(match[1]) * 100);
}

describe("WORKED_EXAMPLE matches the displayed math", () => {
  it("should report total = events × event + events × stagesPerEvent × stage", () => {
    const expectedCents =
      WORKED_EXAMPLE.events * RATES_CENTS.event +
      WORKED_EXAMPLE.events * WORKED_EXAMPLE.stagesPerEvent * RATES_CENTS.stage;
    // Compare numeric value, not format — RATES_DISPLAY ships `$5` while
    // WORKED_EXAMPLE.total ships `$31.00`, both valid presentation choices.
    expect(parseDollarString(WORKED_EXAMPLE.total)).toBe(expectedCents);
  });

  it("should report starterCoversEvents as floor(starterCredit / per-event cycle cost)", () => {
    const perEventCents =
      RATES_CENTS.event + WORKED_EXAMPLE.stagesPerEvent * RATES_CENTS.stage;
    const expected = Math.floor(RATES_CENTS.starterCredit / perEventCents);
    expect(WORKED_EXAMPLE.starterCoversEvents).toBe(expected);
  });

  it("should keep events and stagesPerEvent as positive integers", () => {
    expect.soft(Number.isInteger(WORKED_EXAMPLE.events)).toBe(true);
    expect.soft(WORKED_EXAMPLE.events).toBeGreaterThan(0);
    expect.soft(Number.isInteger(WORKED_EXAMPLE.stagesPerEvent)).toBe(true);
    expect.soft(WORKED_EXAMPLE.stagesPerEvent).toBeGreaterThan(0);
  });
});

describe("RATES_DISPLAY format contract (shipped to OpenAPI / schema / smoke selectors)", () => {
  it.each([
    ["event", RATES_DISPLAY.event],
    ["stage", RATES_DISPLAY.stage],
    ["starterCredit", RATES_DISPLAY.starterCredit],
  ])("should format %s as a leading-$ amount with no whitespace", (_key, str) => {
    expect(str).toMatch(/^\$\d+(\.\d{2})?$/);
  });
});
