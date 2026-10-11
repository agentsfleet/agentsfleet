import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect } from "vitest";
import HowItWorks from "./HowItWorks";

describe("Incident workflow illustration", () => {
  it("labels the illustration as an example without fake live activity or controls", () => {
    render(<HowItWorks />);
    const example = screen.getByRole("figure", { name: /Illustrated example/ });
    expect(example.querySelector('[data-live="true"]')).toBeNull();
    expect(within(example).queryByRole("button")).toBeNull();
  });

  it("places approval before repair", () => {
    render(<HowItWorks />);
    const approval = screen.getByText("Repository write access");
    const repair = screen.getByText("Reread evidence. Bound the fix.");
    expect(approval.compareDocumentPosition(repair) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

it("switches from incident response to a mention-only Slack example", async () => {
  const user = userEvent.setup();
  render(<HowItWorks />);
  expect(within(screen.getByTestId("how-it-works")).getAllByRole("button").map(tab => tab.textContent)).toEqual(["Incident Response", "Slack Teammate"]);
  await user.click(screen.getByRole("button", { name: "Slack Teammate" }));
  expect(screen.getByRole("figure", { name: "Illustrated example: Slack teammate" })).toHaveTextContent("You mention @agentsfleet");
  expect(screen.getByRole("figure", { name: "Illustrated example: Slack teammate" })).toHaveTextContent("stays read-only");
  expect(screen.getByRole("figure", { name: "Illustrated example: Slack teammate" })).toHaveTextContent("does not read other channels’ memory");
});
