import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import React from "react";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));
vi.mock("lucide-react", () => ({
  CoinsIcon: (props: Record<string, unknown>) =>
    React.createElement("svg", { ...props, "data-icon": "coins" }),
}));

const { BalanceLink, BALANCE_HREF, BALANCE_LABEL, BALANCE_EXHAUSTED_LABEL } = await import(
  "../components/layout/BalanceLink"
);

afterEach(() => cleanup());

const link = () => screen.getByRole("link");

describe("BalanceLink", () => {
  it("states what is left and links to the page that can change it", () => {
    render(<BalanceLink balanceNanos={4_710_000_000} isExhausted={false} />);
    expect(link().textContent).toContain("$4.71");
    expect(link().getAttribute("href")).toBe(BALANCE_HREF);
    expect(link().getAttribute("aria-label")).toBe(BALANCE_LABEL);
    // Tabular mono so the figure does not shuffle as it changes.
    expect(link().className).toContain("font-mono");
    expect(link().className).toContain("tabular-nums");
  });

  it("turns destructive when the balance is exhausted, because that changes what happens next", () => {
    // Exhausted is the one state an operator has to act on: new fleet events
    // gate-block until a top-up.
    render(<BalanceLink balanceNanos={0} isExhausted />);
    expect(link().className).toContain("text-destructive");
    expect(link().dataset.exhausted).toBe("true");
    expect(link().getAttribute("aria-label")).toBe(BALANCE_EXHAUSTED_LABEL);
  });

  it("stays quiet while there is credit", () => {
    render(<BalanceLink balanceNanos={4_710_000_000} isExhausted={false} />);
    expect(link().className).not.toContain("text-destructive");
    expect(link().dataset.exhausted).toBeUndefined();
  });

  it("renders sub-cent balances at the precision the formatter carries", () => {
    render(<BalanceLink balanceNanos={9_400_000} isExhausted={false} />);
    expect(link().textContent).toContain("$0.0094");
  });

  it("stands down below sm, where the header has no room for it", () => {
    render(<BalanceLink balanceNanos={1} isExhausted={false} />);
    expect(link().className).toContain("hidden");
    expect(link().className).toContain("sm:inline-flex");
  });
});
