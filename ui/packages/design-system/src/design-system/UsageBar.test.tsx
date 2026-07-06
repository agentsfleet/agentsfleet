import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { UsageBar } from "./UsageBar";

const USAGE_BAR_SRC_PATH = path.join(__dirname, "UsageBar.tsx");

describe("UsageBar", () => {
  it("renders label, tabular-nums percentage, track+fill, and a sub-caption when a label is given", () => {
    render(<UsageBar label="Monthly run budget" pct={62} sublabel="$310 of $500" />);
    expect(screen.getByText("Monthly run budget")).toBeInTheDocument();
    expect(screen.getByText("62%")).toBeInTheDocument();
    expect(screen.getByText("$310 of $500")).toBeInTheDocument();
    const el = screen.getByTestId("usage-bar");
    const fill = el.querySelector(".usage-bar-fill") as HTMLElement;
    expect(fill.style.width).toBe("62%");
  });

  it("renders track+fill only when no label is given (BillingBalanceCard's unlabeled case)", () => {
    render(<UsageBar pct={30} />);
    const el = screen.getByTestId("usage-bar");
    expect(el.querySelector(".usage-bar-fill")).toBeInTheDocument();
    expect(screen.queryByText("30%")).toBeNull();
  });

  it("clamps pct to [0, 100]", () => {
    const { rerender } = render(<UsageBar pct={140} />);
    let fill = screen.getByTestId("usage-bar").querySelector(".usage-bar-fill") as HTMLElement;
    expect(fill.style.width).toBe("100%");

    rerender(<UsageBar pct={-10} />);
    fill = screen.getByTestId("usage-bar").querySelector(".usage-bar-fill") as HTMLElement;
    expect(fill.style.width).toBe("0%");
  });

  it("track is aria-hidden (decorative) while a supplied sublabel stays in the accessibility tree", () => {
    render(<UsageBar pct={50} sublabel="spent $12" />);
    const el = screen.getByTestId("usage-bar");
    const track = el.querySelector(".usage-bar-track") as HTMLElement;
    expect(track.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByText("spent $12")).not.toHaveAttribute("aria-hidden");
  });

  it("uses only mapped design-system tokens — no arbitrary hex or bracket utility in its class strings", () => {
    const src = readFileSync(USAGE_BAR_SRC_PATH, "utf8");
    expect(src).not.toMatch(/#[0-9a-fA-F]{3,6}/);
    expect(src).not.toMatch(/\[[^\]]+\]/);
  });

  it("a caller can override data-testid (BillingBalanceCard pins balance-meter)", () => {
    render(<UsageBar pct={6} data-testid="balance-meter" />);
    expect(screen.getByTestId("balance-meter")).toBeInTheDocument();
    expect(screen.queryByTestId("usage-bar")).toBeNull();
  });
});
