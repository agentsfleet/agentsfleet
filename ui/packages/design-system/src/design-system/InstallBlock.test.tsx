import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import InstallBlock from "./InstallBlock";

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
  return render(<InstallBlock {...props} />);
}

describe("InstallBlock", () => {
  it("renders the title as an h2", () => {
    renderBlock();
    expect(
      screen.getByRole("heading", { level: 2, name: "Install Zombiectl" }),
    ).toBeInTheDocument();
  });

  it("renders the command in a terminal block", () => {
    renderBlock();
    const terminal = screen.getByLabelText("Install Zombiectl command");
    expect(terminal).toBeInTheDocument();
    expect(terminal).toHaveTextContent(/curl -sSL https:\/\/usezombie\.sh\/install \| bash/);
  });

  it("renders all action links", () => {
    renderBlock();
    expect(screen.getByRole("link", { name: "Install now" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Read the docs" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Setup dashboard" })).toBeInTheDocument();
  });

  it("applies per-variant utility classes to action buttons", () => {
    renderBlock();
    const ghost = screen.getByRole("link", { name: "Read the docs" });
    expect(ghost.className).toContain("bg-transparent");
    expect(ghost.className).toContain("border-border");

    const double = screen.getByRole("link", { name: "Setup dashboard" });
    expect(double.className).toMatch(/border-2\s.*border-primary/);
  });

  it("primary action uses the default primary variant", () => {
    renderBlock();
    const primary = screen.getByRole("link", { name: "Install now" });
    expect(primary.className).toContain("text-primary-foreground");
    expect(primary.className).not.toContain("bg-transparent");
  });

  it("renders a bordered card-style container", () => {
    const { container } = renderBlock();
    const root = container.firstElementChild as HTMLElement | null;
    expect(root?.className).toContain("border");
    expect(root?.className).toContain("rounded-lg");
  });

  it("renders actions in a flex-wrap container with one anchor per action", () => {
    const { container } = renderBlock();
    const actionsRow = container.querySelector("[class*='flex-wrap']");
    expect(actionsRow).toBeInTheDocument();
    expect(actionsRow?.querySelectorAll("a")).toHaveLength(3);
  });

  it("shows a copy button on the terminal block", () => {
    renderBlock();
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy command/i })).toBeInTheDocument();
  });

  it("exposes data-command attribute for machine readability", () => {
    const { container } = renderBlock();
    const pre = container.querySelector("pre");
    expect(pre).toHaveAttribute("data-command", defaultProps.command);
  });

  it("external=true adds target=_blank + rel=noopener noreferrer", () => {
    render(
      <InstallBlock
        title="X"
        command="echo x"
        actions={[{ label: "Docs", to: "https://example.com", external: true }]}
      />,
    );
    const link = screen.getByRole("link", { name: "Docs" });
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });
});
