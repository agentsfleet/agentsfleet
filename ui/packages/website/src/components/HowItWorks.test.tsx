import { render, screen, within } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import HowItWorks from "./HowItWorks";
import { HOW_IT_WORKS_HEADING } from "../lib/marketing-copy";

describe("Incident workflow illustration", () => {
  it("labels the illustration as an example without fake live activity or controls", () => {
    render(<HowItWorks />);
    const example = screen.getByRole("figure", { name: /Illustrated example/ });
    expect(example).toHaveTextContent("Illustrative example");
    expect(example.querySelector('[data-live="true"]')).toBeNull();
    expect(within(example).queryByRole("button")).toBeNull();
    expect(example).toHaveTextContent(/not a live incident/i);
  });

  it("does not imply diagnosis automatically starts repair", () => {
    render(<HowItWorks />);
    const example = screen.getByRole("figure");
    expect(example).toHaveTextContent(/a diagnosis alone never starts it/i);
    expect(example).toHaveTextContent(/human request or failed GitHub workflow/i);
  });

  it("does not promise every incident produces a fix or autonomous deployment", () => {
    render(<HowItWorks />);
    expect(screen.getByRole("figure")).toHaveTextContent(/end with diagnosis only/i);
    expect(screen.getByRole("figure")).toHaveTextContent(/do not merge or deploy/i);
  });

  it("places approval before repair and keeps the merge human-owned", () => {
    render(<HowItWorks />);
    const approval = screen.getByText("Repository write access");
    const repair = screen.getByText("Reread evidence. Bound the fix.");
    expect(approval.compareDocumentPosition(repair) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByRole("figure")).toHaveTextContent("You review and merge.");
  });

  it("names each evidence and delivery tool with accessible text", () => {
    render(<HowItWorks />);
    for (const name of ["Grafana", "Elasticsearch", "Slack", "GitHub"]) {
      expect(screen.getByRole("figure")).toHaveTextContent(name);
    }
    expect(screen.getByRole("heading", { level: 2, name: HOW_IT_WORKS_HEADING })).toBeInTheDocument();
  });
});
