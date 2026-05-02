import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("lucide-react", () => ({
  ReceiptIcon: () => React.createElement("svg", { "data-icon": "ReceiptIcon" }),
  CreditCardIcon: () => React.createElement("svg", { "data-icon": "CreditCardIcon" }),
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
}));

import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  EmptyState,
} from "@usezombie/design-system";

afterEach(() => cleanup());

/**
 * The billing page renders Radix Tabs with three triggers (Usage / Invoices
 * / Payment Method). Radix only renders the active panel's children to the
 * DOM, so the dashboard-coverage SSR snapshot can't assert that the
 * Invoices / Payment empty-state copy is reachable. This test mounts the
 * same Tabs structure interactively and clicks each trigger.
 */
describe("Billing tabs interaction", () => {
  function ThreeTabs() {
    return React.createElement(
      Tabs,
      { defaultValue: "usage" },
      React.createElement(
        TabsList,
        null,
        React.createElement(TabsTrigger, { value: "usage" }, "Usage"),
        React.createElement(TabsTrigger, { value: "invoices" }, "Invoices"),
        React.createElement(TabsTrigger, { value: "payment" }, "Payment Method"),
      ),
      React.createElement(
        TabsContent,
        { value: "usage" },
        "USAGE_PANEL",
      ),
      React.createElement(
        TabsContent,
        { value: "invoices" },
        React.createElement(EmptyState, {
          title: "No invoices yet",
          description: "Invoicing arrives with Purchase Credits in v2.1.",
        }),
      ),
      React.createElement(
        TabsContent,
        { value: "payment" },
        React.createElement(EmptyState, {
          title: "No payment method on file",
          description: "Payment methods arrive with Purchase Credits in v2.1.",
        }),
      ),
    );
  }

  it("Usage tab is active by default", () => {
    render(React.createElement(ThreeTabs));
    expect(screen.getByText("USAGE_PANEL")).toBeTruthy();
    expect(screen.queryByText("No invoices yet")).toBeNull();
    expect(screen.queryByText("No payment method on file")).toBeNull();
  });

  it("clicking Invoices trigger swaps the panel to the Invoices empty-state", async () => {
    const user = userEvent.setup();
    render(React.createElement(ThreeTabs));
    await user.click(screen.getByRole("tab", { name: /invoices/i }));
    await waitFor(() => expect(screen.getByText("No invoices yet")).toBeTruthy());
    expect(screen.queryByText("USAGE_PANEL")).toBeNull();
  });

  it("clicking Payment Method trigger swaps to the Payment empty-state", async () => {
    const user = userEvent.setup();
    render(React.createElement(ThreeTabs));
    await user.click(screen.getByRole("tab", { name: /payment method/i }));
    await waitFor(() => expect(screen.getByText("No payment method on file")).toBeTruthy());
  });
});
