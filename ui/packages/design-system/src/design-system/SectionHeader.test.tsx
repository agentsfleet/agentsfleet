import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SectionHeader } from "./SectionHeader";

describe("SectionHeader", () => {
  it("centers the label on its action's middle rather than its baseline", () => {
    const { container } = render(
      <SectionHeader actions={<button type="button">Create fleet</button>}>Manage fleets</SectionHeader>,
    );
    const header = container.firstElementChild;
    // Baseline alignment left 13px of slack under a one-line label sharing a
    // row with a 40px action; the gap to the section body measured 29px.
    expect(header).toHaveClass("items-center");
    expect(header).not.toHaveClass("items-baseline");
  });

  it("renders the standard work-area label and optional action", () => {
    const { container } = render(
      <SectionHeader className="custom-header" actions={<button type="button">Create fleet</button>}>
        Manage fleets
      </SectionHeader>,
    );

    const header = container.firstElementChild;
    expect(header).toHaveClass("flex", "justify-between", "custom-header");
    expect(screen.getByRole("heading", { level: 2, name: "Manage fleets" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create fleet" })).toBeTruthy();
  });

  it("renders without an empty action wrapper", () => {
    const { container } = render(<SectionHeader>Manage events</SectionHeader>);

    expect(screen.getByRole("heading", { level: 2, name: "Manage events" })).toBeInTheDocument();
    expect(container.firstElementChild?.children).toHaveLength(1);
  });

  it("keeps the description with its label when a caller requests paragraph semantics", () => {
    const ref = {current: null as HTMLDivElement | null};
    render(<SectionHeader ref={ref} as="p" description="Devices currently signed in to your account.">
      Active sessions
    </SectionHeader>);
    const label = screen.getByText("Active sessions");
    expect(label.tagName).toBe("P");
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    expect(label.parentElement).toContainElement(screen.getByText("Devices currently signed in to your account."));
    expect(ref.current).toContainElement(label);
  });
});
