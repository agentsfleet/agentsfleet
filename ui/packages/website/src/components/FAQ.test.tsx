import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect } from "vitest";
import FAQ from "./FAQ";
import { FAQ_WEDGE_ITEM, PRICING_COPY } from "../lib/marketing-copy";

const FAQ_TEST_TIMEOUT_MS = 20_000;

function escapedPattern(value: string) {
  return new RegExp(value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
}

describe("FAQ", { timeout: FAQ_TEST_TIMEOUT_MS }, () => {
  it("renders the section heading", () => {
    render(<FAQ />);
    expect(screen.getByRole("heading", { level: 2, name: /common questions/i })).toBeInTheDocument();
  });

  it("renders all FAQ questions as buttons", () => {
    render(<FAQ />);
    expect(screen.getByText("What is agentsfleet?")).toBeInTheDocument();
    expect(screen.getByText(FAQ_WEDGE_ITEM.q)).toBeInTheDocument();
    expect(screen.getByText("What does self-managed mean?")).toBeInTheDocument();
    expect(screen.getByText("What am I actually paying for?")).toBeInTheDocument();
    expect(screen.getByText("Can I self-host?")).toBeInTheDocument();
    expect(screen.getByText("Which coding agents work for the install skill?")).toBeInTheDocument();
    expect(screen.getByText("What if my Fleet hits the model's context window?")).toBeInTheDocument();
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

  it("places the heading on the page left rail, not inside the reading-measure column", () => {
    const { container } = render(<FAQ />);
    const heading = screen.getByRole("heading", { level: 2, name: /common questions/i });
    // The heading aligns with the rest of the page; only the answers keep the
    // narrower reading measure.
    expect(heading.closest(".max-w-measure")).toBeNull();
    const measure = container.querySelector(".max-w-measure");
    expect(measure).not.toBeNull();
    expect(measure!.querySelector('[data-testid="faq-item-0"]')).not.toBeNull();
  });

  it("defines the Fleet at first touch: 'What is agentsfleet?' opens to the explicit definition", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText("What is agentsfleet?"));
    expect(
      screen.getByText(/A Fleet is a long-lived runtime you install once/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/not a one-shot prompt/i)).toBeInTheDocument();
  });

  it("renders the wedge FAQ answer with source and approval posture", async () => {
    const user = userEvent.setup();
    render(<FAQ />);
    await user.click(screen.getByText(FAQ_WEDGE_ITEM.q));
    expect(screen.getByText(/Signals, telemetry, code/i)).toBeInTheDocument();
    expect(screen.getByText(/human approval before merge or deploy/i)).toBeInTheDocument();
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

  it("does not render the operational-extras FAQ entry (extras were removed from pricing)", () => {
    render(<FAQ />);
    expect(screen.queryByText(/extras provisioned per workspace/i)).not.toBeInTheDocument();
  });
});
