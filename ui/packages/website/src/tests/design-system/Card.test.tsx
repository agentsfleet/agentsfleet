import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Card } from "@usezombie/design-system";

describe("Card", () => {
  it("renders children", () => {
    render(<Card><p>Hello card</p></Card>);
    expect(screen.getByText("Hello card")).toBeInTheDocument();
  });

  it("has z-card class", () => {
    const { container } = render(<Card>Content</Card>);
    expect(container.firstChild).toHaveClass("z-card");
  });

  it("does not have featured class by default", () => {
    const { container } = render(<Card>Content</Card>);
    expect(container.firstChild).not.toHaveClass("z-card--featured");
  });

  it("adds featured class when featured=true", () => {
    const { container } = render(<Card featured>Featured</Card>);
    expect(container.firstChild).toHaveClass("z-card--featured");
  });

  it("renders as an <article>", () => {
    const { container } = render(<Card>Content</Card>);
    expect(container.firstChild?.nodeName).toBe("ARTICLE");
  });

  it("forwards extra className", () => {
    const { container } = render(<Card className="my-card">Content</Card>);
    expect(container.firstChild).toHaveClass("z-card", "my-card");
  });

  it("forwards arbitrary props (e.g. data-testid)", () => {
    render(<Card data-testid="my-card">Content</Card>);
    expect(screen.getByTestId("my-card")).toBeInTheDocument();
  });
});
