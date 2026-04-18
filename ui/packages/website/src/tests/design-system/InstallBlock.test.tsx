import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import { InstallBlock } from "@usezombie/design-system";

const defaultProps = {
  title: "Install Zombiectl",
  command: "curl -sSL https://usezombie.sh/install | bash",
  actions: [
    { label: "Install now", to: "https://docs.usezombie.com/quickstart" },
    { label: "Read the docs", to: "https://docs.usezombie.com", variant: "ghost" as const },
    { label: "Setup dashboard", to: "https://app.usezombie.com", variant: "double-border" as const },
  ],
};

function renderBlock(props = defaultProps) {
  return render(
    <MemoryRouter>
      <InstallBlock {...props} />
    </MemoryRouter>
  );
}

describe("InstallBlock", () => {
  it("renders the title as an h2", () => {
    renderBlock();
    expect(screen.getByRole("heading", { level: 2, name: "Install Zombiectl" })).toBeInTheDocument();
  });

  it("renders the command in a terminal block", () => {
    renderBlock();
    const terminal = screen.getByLabelText("Install Zombiectl command");
    expect(terminal).toBeInTheDocument();
    expect(terminal).toHaveTextContent(/curl -sSL https:\/\/usezombie\.sh\/install \| bash/);
  });

  it("renders all action buttons", () => {
    renderBlock();
    expect(screen.getByRole("link", { name: "Install now" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Read the docs" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Setup dashboard" })).toBeInTheDocument();
  });

  it("applies correct variant classes to buttons", () => {
    renderBlock();
    const ghost = screen.getByRole("link", { name: "Read the docs" });
    expect(ghost.className).toContain("bg-transparent");
    expect(ghost.className).toContain("border-border");

    const double = screen.getByRole("link", { name: "Setup dashboard" });
    expect(double.className).toMatch(/border-2\s.*border-primary/);
  });

  it("primary button uses the default primary variant", () => {
    renderBlock();
    const primary = screen.getByRole("link", { name: "Install now" });
    expect(primary.className).toContain("text-primary-foreground");
    expect(primary.className).not.toContain("bg-transparent");
  });

  it("renders inside z-install-block container", () => {
    const { container } = renderBlock();
    expect(container.querySelector(".z-install-block")).toBeInTheDocument();
  });

  it("renders z-btn-row around actions", () => {
    const { container } = renderBlock();
    expect(container.querySelector(".z-btn-row")).toBeInTheDocument();
  });

  it("shows a copy button on the terminal block for humans", () => {
    renderBlock();
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy command/i })).toBeInTheDocument();
  });

  it("exposes data-command attribute for machine readability", () => {
    const { container } = renderBlock();
    const pre = container.querySelector("pre");
    expect(pre).toHaveAttribute("data-command", defaultProps.command);
  });
});
