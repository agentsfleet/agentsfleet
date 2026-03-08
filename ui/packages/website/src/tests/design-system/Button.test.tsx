import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { Button } from "@usezombie/design-system";

function wrap(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

describe("Button — primary variant", () => {
  it("renders as a router Link for internal paths", () => {
    wrap(<Button to="/pricing">View pricing</Button>);
    const el = screen.getByRole("link", { name: "View pricing" });
    expect(el).toHaveAttribute("href", "/pricing");
    expect(el).not.toHaveAttribute("target");
  });

  it("has primary class by default", () => {
    wrap(<Button to="/pricing">Foo</Button>);
    expect(screen.getByRole("link")).toHaveClass("z-btn");
    expect(screen.getByRole("link")).not.toHaveClass("z-btn--ghost");
    expect(screen.getByRole("link")).not.toHaveClass("z-btn--double");
  });

  it("renders external link with target _blank and rel", () => {
    wrap(<Button to="https://docs.usezombie.com" external>Docs</Button>);
    const el = screen.getByRole("link", { name: "Docs" });
    expect(el).toHaveAttribute("href", "https://docs.usezombie.com");
    expect(el).toHaveAttribute("target", "_blank");
    expect(el).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("renders https:// paths as external <a> without needing external prop", () => {
    wrap(<Button to="https://github.com/usezombie">GitHub</Button>);
    const el = screen.getByRole("link");
    expect(el.tagName).toBe("A");
    expect(el).toHaveAttribute("href", "https://github.com/usezombie");
  });

  it("renders mailto: as plain <a>", () => {
    wrap(<Button to="mailto:team@usezombie.com">Email</Button>);
    const el = screen.getByRole("link");
    expect(el).toHaveAttribute("href", "mailto:team@usezombie.com");
    expect(el).not.toHaveAttribute("target");
  });
});

describe("Button — ghost variant", () => {
  it("applies ghost class", () => {
    wrap(<Button to="/pricing" variant="ghost">Ghost</Button>);
    expect(screen.getByRole("link")).toHaveClass("z-btn--ghost");
  });

  it("still renders as router Link for internal path", () => {
    wrap(<Button to="/agents" variant="ghost">Agents</Button>);
    expect(screen.getByRole("link")).toHaveAttribute("href", "/agents");
  });
});

describe("Button — double-border variant", () => {
  it("applies double-border class", () => {
    wrap(<Button to="/dashboard" variant="double-border">Dashboard</Button>);
    expect(screen.getByRole("link")).toHaveClass("z-btn--double");
  });
});

describe("Button — native button", () => {
  it("renders as <button> when no `to` prop", () => {
    render(<Button onClick={() => {}}>Click me</Button>);
    expect(screen.getByRole("button")).toBeInTheDocument();
    expect(screen.getByRole("button")).toHaveClass("z-btn");
  });

  it("calls onClick when clicked", async () => {
    const handler = vi.fn();
    const user = userEvent.setup();
    render(<Button onClick={handler}>Click</Button>);
    await user.click(screen.getByRole("button"));
    expect(handler).toHaveBeenCalledOnce();
  });

  it("respects disabled attribute", async () => {
    const handler = vi.fn();
    const user = userEvent.setup();
    render(<Button onClick={handler} disabled>Disabled</Button>);
    await user.click(screen.getByRole("button"));
    expect(handler).not.toHaveBeenCalled();
  });
});
