import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("lucide-react", () => ({
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
}));

import BillingPlanRow from "@/app/(dashboard)/settings/billing/components/BillingPlanRow";

afterEach(() => cleanup());

describe("BillingPlanRow (test_billing_no_seat_grid)", () => {
  it("renders one honest 'Pay as you go' row marked Current — not a seat grid", () => {
    render(React.createElement(BillingPlanRow));
    expect(screen.getByTestId("billing-plan-row")).toBeTruthy();
    expect(screen.getByText("Pay as you go")).toBeTruthy();
    expect(screen.getByText("Current")).toBeTruthy();
    // Exactly one plan row — no per-seat / per-month plan cards.
    expect(screen.getAllByTestId("billing-plan-row").length).toBe(1);
    expect(screen.queryByText(/per seat|\/\s*mo|\/\s*month|\bseats?\b/i)).toBeNull();
  });

  it("offers a volume-pricing contact link", () => {
    render(React.createElement(BillingPlanRow));
    const link = screen.getByRole("link", { name: /volume pricing/i }) as HTMLAnchorElement;
    expect(link.getAttribute("href")).toMatch(/^mailto:/);
  });
});
