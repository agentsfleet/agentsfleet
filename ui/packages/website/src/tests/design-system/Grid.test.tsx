import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Grid } from "@usezombie/design-system";

describe("Grid", () => {
  it("renders children", () => {
    render(<Grid columns="two"><span>A</span><span>B</span></Grid>);
    expect(screen.getByText("A")).toBeInTheDocument();
    expect(screen.getByText("B")).toBeInTheDocument();
  });

  it("has z-grid class always", () => {
    const { container } = render(<Grid columns="two">x</Grid>);
    expect(container.firstChild).toHaveClass("z-grid");
  });

  it("applies z-grid--two for two columns", () => {
    const { container } = render(<Grid columns="two">x</Grid>);
    expect(container.firstChild).toHaveClass("z-grid--two");
  });

  it("applies z-grid--three for three columns", () => {
    const { container } = render(<Grid columns="three">x</Grid>);
    expect(container.firstChild).toHaveClass("z-grid--three");
  });

  it("applies z-grid--four for four columns", () => {
    const { container } = render(<Grid columns="four">x</Grid>);
    expect(container.firstChild).toHaveClass("z-grid--four");
  });

  it("forwards extra className", () => {
    const { container } = render(<Grid columns="two" className="custom">x</Grid>);
    expect(container.firstChild).toHaveClass("z-grid", "z-grid--two", "custom");
  });

  it("renders as a <div>", () => {
    const { container } = render(<Grid columns="two">x</Grid>);
    expect(container.firstChild?.nodeName).toBe("DIV");
  });
});
