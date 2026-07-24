import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SectionHeader } from "./SectionHeader";

describe("SectionHeader", () => {
  it("renders the standard work-area label and optional action", () => {
    render(
      <SectionHeader className="custom-header" actions={<button type="button">Create fleet</button>}>
        Manage fleets
      </SectionHeader>,
    );

    const header = screen.getByText("Manage fleets").parentElement;
    expect(header).toHaveClass("flex", "justify-between", "custom-header");
    expect(screen.getByRole("button", { name: "Create fleet" })).toBeTruthy();
  });

  it("renders without an empty action wrapper", () => {
    const { container } = render(<SectionHeader>Manage events</SectionHeader>);

    expect(screen.getByText("Manage events")).toBeTruthy();
    expect(container.querySelectorAll("div")).toHaveLength(1);
  });
});
