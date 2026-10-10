import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect } from "vitest";
import FAQ from "./FAQ";
import { PRICING_COPY } from "../lib/marketing-copy";

const FAQ_TEST_TIMEOUT_MS = 20_000;

function escapedPattern(value: string) {
  return new RegExp(value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
}

describe("FAQ", { timeout: FAQ_TEST_TIMEOUT_MS }, () => {
  it("renders the section heading", () => {
    render(<FAQ />);
    expect(screen.getByRole("heading", { level: 2, name: /common questions/i })).toBeInTheDocument();
  });

  it("answers are hidden by default", () => {
    render(<FAQ />);
    expect(screen.queryByText(/self-managed provider key is/)).not.toBeInTheDocument();
  });

  it("shows answer when question is clicked", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.getByText(/self-managed provider key is/)).toBeInTheDocument();
  });

  it("hides answer when clicked again", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.getByText(/self-managed provider key is/)).toBeInTheDocument();
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.queryByText(/self-managed provider key is/)).not.toBeInTheDocument();
  });

  it("closes previous answer when another is opened", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What does self-managed mean?"));
    expect(screen.getByText(/self-managed provider key is/)).toBeInTheDocument();
    await user.click(screen.getByText("What am I actually paying for?"));
    expect(screen.queryByText(/self-managed provider key is/)).not.toBeInTheDocument();
    expect(screen.getByText(escapedPattern(PRICING_COPY.note))).toBeInTheDocument();
  });

  it("sets aria-expanded correctly", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    const button = screen.getByText("What does self-managed mean?");
    expect(button).toHaveAttribute("aria-expanded", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps early-access terms consistent without speculative rates", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What am I actually paying for?"));
    const answer = screen.getByText(escapedPattern(PRICING_COPY.note)).textContent ?? "";
    expect(answer).toContain(PRICING_COPY.note);
    expect(answer).not.toMatch(/\$\d|starter credit|zero markup/i);
    expect(answer).toContain("does not remove fleet runtime costs");
  });
});
