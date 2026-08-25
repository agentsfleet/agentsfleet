import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Nav } from "./Nav";

describe("Nav", () => {
  it("renders a <nav> landmark", () => {
    const { container } = render(<Nav aria-label="Primary">Home</Nav>);
    expect(container.firstChild?.nodeName).toBe("NAV");
    expect(screen.getByText("Home")).toBeInTheDocument();
  });

  // Bug this catches: a page with two unnamed <nav> landmarks gives screen
  // reader users two entries both announced "navigation", with nothing to tell
  // them apart. The required aria-label is the whole reason this primitive
  // exists rather than callers writing <nav> directly.
  it("exposes its accessible name, so two landmarks are distinguishable", () => {
    render(
      <>
        <Nav aria-label="Primary">P</Nav>
        <Nav aria-label="Breadcrumbs">B</Nav>
      </>,
    );
    expect(screen.getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Breadcrumbs" })).toBeInTheDocument();
  });

  it("merges consumer className", () => {
    const { container } = render(
      <Nav aria-label="Primary" className="flex gap-2" />,
    );
    expect((container.firstChild as HTMLElement).className).toContain("flex");
  });

  it("forwards a ref to the underlying <nav>", () => {
    let captured: HTMLElement | null = null;
    render(<Nav aria-label="Primary" ref={(node) => { captured = node; }} />);
    expect(captured).not.toBeNull();
    expect(captured!.nodeName).toBe("NAV");
  });

  it("passes arbitrary nav attributes through", () => {
    render(<Nav aria-label="Primary" id="site-nav" data-testid="nav" />);
    expect(screen.getByTestId("nav")).toHaveAttribute("id", "site-nav");
  });
});
