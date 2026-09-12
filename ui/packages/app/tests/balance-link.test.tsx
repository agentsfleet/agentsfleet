import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import React from "react";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));

const {
  BalanceLink,
  formatHeaderBalance,
  BALANCE_HREF,
  BALANCE_LABEL,
  BALANCE_ARIA_LABEL,
  BALANCE_EXHAUSTED_ARIA_LABEL,
} = await import("../components/layout/BalanceLink");

afterEach(() => cleanup());

const link = () => screen.getByRole("link");
const figure = () => screen.getByText(/^\$/);

describe("BalanceLink", () => {
  it("names the figure and links to the page that can change it", () => {
    render(<BalanceLink balanceNanos={4_806_600_000} isExhausted={false} />);
    // Labelled, not a bare number beside a glyph: an operator scanning the
    // header for "how much is left" has to be told which figure this is.
    expect(screen.getByText(BALANCE_LABEL)).toBeTruthy();
    expect(figure().textContent).toBe("$4.81");
    expect(link().getAttribute("href")).toBe(BALANCE_HREF);
    expect(link().getAttribute("aria-label")).toBe(BALANCE_ARIA_LABEL);
  });

  it("bounds itself as a chip, so the eye lands on it without the type growing", () => {
    render(<BalanceLink balanceNanos={4_806_600_000} isExhausted={false} />);
    // The bound is what makes it findable: growing the type made the figure
    // loud without making it findable, since nothing around it is a chip.
    expect(link().className).toContain("border-pulse/40");
    expect(link().className).toContain("bg-pulse/10");
    expect(figure().className).toContain("text-pulse");
    // Header-sized type, not display type.
    expect(figure().className).toContain("text-body-sm");
    expect(figure().className).not.toContain("text-body-lg");
    // The workspace switcher's own height: two bounded controls side by side
    // at different heights read as a mistake before they read as a hierarchy.
    expect(link().className).toContain("h-8");
    // Tabular mono so the figure does not shuffle as it changes.
    expect(figure().className).toContain("font-mono");
    expect(figure().className).toContain("tabular-nums");
    // The label stays quiet; only the figure carries the colour.
    expect(screen.getByText(BALANCE_LABEL).className).toContain("text-muted-foreground");
  });

  it("turns destructive when exhausted, because that changes what happens next", () => {
    // Exhausted is the one state an operator has to act on: new fleet events
    // gate-block until a top-up.
    render(<BalanceLink balanceNanos={0} isExhausted />);
    expect(figure().className).toContain("text-destructive");
    expect(figure().className).not.toContain("text-pulse");
    // The whole chip turns, not just the figure inside it.
    expect(link().className).toContain("border-destructive/40");
    expect(link().className).toContain("bg-destructive/10");
    expect(link().className).not.toContain("border-pulse/40");
    expect(link().dataset.exhausted).toBe("true");
    expect(link().getAttribute("aria-label")).toBe(BALANCE_EXHAUSTED_ARIA_LABEL);
  });

  it("stays in the accent while there is credit", () => {
    render(<BalanceLink balanceNanos={4_806_600_000} isExhausted={false} />);
    expect(link().dataset.exhausted).toBeUndefined();
  });

  it("stands down below sm, where the header has no room for it", () => {
    // pin test: literal is the contract
    render(<BalanceLink balanceNanos={1_000_000_000} isExhausted={false} />);
    expect(link().className).toContain("hidden");
    expect(link().className).toContain("sm:inline-flex");
  });
});

describe("formatHeaderBalance", () => {
  it("reads in cents, because $4.8066 is a figure to parse and $4.81 is one to read", () => {
    expect(formatHeaderBalance(4_806_600_000)).toBe("$4.81");
    expect(formatHeaderBalance(17_080_000_000)).toBe("$17.08");
  });

  it("keeps the billing page's precision under a cent, rather than saying $0.00", () => {
    // Rounding a live sub-cent balance to zero would say "spent" where the
    // truth is "nearly" — and the exhausted treatment is what says spent.
    expect(formatHeaderBalance(9_400_000)).toBe("$0.0094");
  });

  it("says so when the balance is smaller than four decimals can show", () => {
    // The same floor the charges table gives a sub-visible debit: below this
    // even the four-decimal formatter returns $0.00, which is the one figure
    // that reads as spent.
    expect(formatHeaderBalance(1)).toBe("<$0.0001");
    expect(formatHeaderBalance(49_999)).toBe("<$0.0001");
    expect(formatHeaderBalance(50_000)).toBe("$0.0001");
  });

  it("states a true zero as zero", () => {
    expect(formatHeaderBalance(0)).toBe("$0.00");
  });
});
