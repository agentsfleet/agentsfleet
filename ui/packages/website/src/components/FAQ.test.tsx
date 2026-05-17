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
    expect(screen.getByText("What is usezombie?")).toBeInTheDocument();
    expect(screen.getByText("What does self-managed mean?")).toBeInTheDocument();
    expect(screen.getByText("What am I actually paying for?")).toBeInTheDocument();
    expect(screen.getByText("Can I self-host?")).toBeInTheDocument();
    expect(screen.getByText("Which agent hosts work for the install skill?")).toBeInTheDocument();
    expect(screen.getByText("What if my agent hits the model's context window?")).toBeInTheDocument();
  });

  it("answers are hidden by default", () => {
    render(<FAQ />);
    expect(screen.queryByText(/self-managed provider key\./)).not.toBeInTheDocument();
  });

  it("shows answer when question is clicked", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.getByText(/self-managed provider key\./)).toBeInTheDocument();
  });

  it("hides answer when clicked again", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.getByText(/self-managed provider key\./)).toBeInTheDocument();
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.queryByText(/self-managed provider key\./)).not.toBeInTheDocument();
  });

  it("closes previous answer when another is opened", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.getByText(/self-managed provider key\./)).toBeInTheDocument();
    await user.click(screen.getByText("What am I actually paying for?"));
    expect(screen.queryByText(/self-managed provider key\./)).not.toBeInTheDocument();
    expect(screen.getByText(/Hosted execution is metered/i)).toBeInTheDocument();
  });

  it("sets aria-expanded correctly", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    const button = screen.getByText("What does self-managed mean?");
    expect(button).toHaveAttribute("aria-expanded", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
  });
});
