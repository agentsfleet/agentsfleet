import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { PageTitle } from "./PageTitle";

describe("PageTitle", () => {
  it("renders as an <h1> with children", () => {
    const { container } = render(<PageTitle>Workspaces</PageTitle>);
    expect(container.firstChild?.nodeName).toBe("H1");
    expect(screen.getByRole("heading", { level: 1, name: "Workspaces" })).toBeInTheDocument();
  });

  it("applies base typography utilities", () => {
    const { container } = render(<PageTitle>Title</PageTitle>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("text-xl");
    expect(cls).toContain("font-semibold");
    expect(cls).toContain("tracking-tight");
  });

  it("merges consumer className without losing base utilities", () => {
    const { container } = render(<PageTitle className="text-2xl">T</PageTitle>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("text-2xl");
    expect(cls).toContain("font-semibold");
  });
});
