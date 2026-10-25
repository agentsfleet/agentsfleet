import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { EMPTY_STATE_SURFACE, EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("keeps a plain empty result unboxed and its explanation unconstrained", () => {
    render(<EmptyState title="No settled activity" description="Choose another period."
      surface={EMPTY_STATE_SURFACE.plain} />);
    const root = screen.getByRole("status");
    expect(root).toHaveAttribute("aria-live", "polite");
    expect(root).toHaveClass("items-start", "text-left");
    expect(root).not.toHaveClass("border", "rounded-md", "items-center");
    expect(screen.getByRole("heading", {name: "No settled activity"})).toHaveClass("font-medium");
    expect(screen.getByText("Choose another period.")).not.toHaveClass("max-w-sm");
  });

  it("renders title and sets role=status + aria-live=polite", () => {
    render(<EmptyState title="Nothing here" description="Add a agent to get started." />);
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
    expect(screen.getByText("Add a agent to get started.")).toBeInTheDocument();
    const root = screen.getByTestId("empty-state");
    expect(root).toHaveAttribute("role", "status");
    expect(root).toHaveAttribute("aria-live", "polite");
  });

  it("omits the <p> description node when no description is provided", () => {
    const { container } = render(<EmptyState title="Nada" />);
    expect(container.querySelectorAll("p").length).toBe(0);
  });

  it("renders the icon slot with aria-hidden and the action slot", () => {
    render(
      <EmptyState
        title="X"
        icon={<i data-testid="icon" data-icon="1" />}
        action={<button>Start</button>}
      />,
    );
    const iconWrap = screen.getByTestId("icon").parentElement!;
    expect(iconWrap).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByText("Start")).toBeInTheDocument();
  });

  it("carries data-slot for external styling hooks", () => {
    render(<EmptyState title="X" />);
    expect(screen.getByTestId("empty-state")).toHaveAttribute("data-slot", "empty-state");
  });

  it("merges a custom className", () => {
    render(<EmptyState title="X" className="my-empty" />);
    const cls = screen.getByTestId("empty-state").className;
    expect(cls).toContain("my-empty");
    expect(cls).toContain("rounded-md");
  });

  it("SSR renders with role=status", () => {
    const html = renderToStaticMarkup(<EmptyState title="SSR" />);
    expect(html).toContain('role="status"');
    expect(html).toContain("SSR");
  });
});
