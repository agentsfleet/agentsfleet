import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect } from "vitest";
import FAQ from "./FAQ";

describe("FAQ", () => {
  it("renders the section heading", () => {
    render(<FAQ />);
    expect(screen.getByRole("heading", { level: 2, name: /common questions/i })).toBeInTheDocument();
  });

  it("renders all FAQ questions as buttons", () => {
    render(<FAQ />);
    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBeGreaterThanOrEqual(6);
    expect(screen.getByText("What does BYOK mean?")).toBeInTheDocument();
    expect(screen.getByText("What am I actually paying for?")).toBeInTheDocument();
    expect(screen.getByText("Can I try it before committing?")).toBeInTheDocument();
    expect(screen.getByText("What happens to my code?")).toBeInTheDocument();
    expect(screen.getByText("Which Git providers are supported?")).toBeInTheDocument();
    expect(screen.getByText("Is there an SLA?")).toBeInTheDocument();
  });

  it("answers are hidden by default", () => {
    render(<FAQ />);
    expect(screen.queryByText(/Bring Your Own Keys/)).not.toBeInTheDocument();
  });

  it("shows answer when question is clicked", async () => {
    const user = userEvent.setup();
    render(<FAQ />);

    await user.click(screen.getByText("What does BYOK mean?"));
    expect(screen.getByText(/Bring Your Own Keys/)).toBeInTheDocument();
  });

  it("hides answer when clicked again", async () => {
    const user = userEvent.setup();
    render(<FAQ />);

    await user.click(screen.getByText("What does BYOK mean?"));
    expect(screen.getByText(/Bring Your Own Keys/)).toBeInTheDocument();

    await user.click(screen.getByText("What does BYOK mean?"));
    expect(screen.queryByText(/Bring Your Own Keys/)).not.toBeInTheDocument();
  });

  it("closes previous answer when another is opened", async () => {
    const user = userEvent.setup();
    render(<FAQ />);

    await user.click(screen.getByText("What does BYOK mean?"));
    expect(screen.getByText(/Bring Your Own Keys/)).toBeInTheDocument();

    await user.click(screen.getByText("What am I actually paying for?"));
    expect(screen.queryByText(/Bring Your Own Keys/)).not.toBeInTheDocument();
    expect(screen.getByText(/Compute billing/)).toBeInTheDocument();
  });

  it("sets aria-expanded correctly", async () => {
    const user = userEvent.setup();
    render(<FAQ />);

    const button = screen.getByText("What does BYOK mean?");
    expect(button).toHaveAttribute("aria-expanded", "false");

    await user.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
  });
});
