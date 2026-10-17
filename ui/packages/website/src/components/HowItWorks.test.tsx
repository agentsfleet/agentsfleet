import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import HowItWorks from "./HowItWorks";
import { PUBLIC_ACCESS_URL } from "../config";
import { DEFAULT_JOB } from "../lib/example-jobs";

afterEach(() => vi.restoreAllMocks());

describe("Conversation example", () => {
  it("shows a complete, inspectable journey with reduced motion", () => {
    render(<HowItWorks job={DEFAULT_JOB} />);
    expect(screen.getByRole("figure", { name: "Illustrated example investigation" })).toHaveTextContent("/schedule every 15 minutes");
    expect(screen.getByText(DEFAULT_JOB.finding)).toBeInTheDocument();
    expect(screen.getByText(DEFAULT_JOB.report)).toBeInTheDocument();
    expect(screen.getByText(`${DEFAULT_JOB.name} created. Waiting for its next scheduled check.`)).toBeInTheDocument();
    expect(screen.queryByText(/Saved to your fleet library/)).toBeNull();
    expect(screen.getByText(/Report ready · repository unchanged/i)).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Example stages" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Replay" })).toBeNull();
    expect(screen.getByRole("link", { name: "Join waitlist" })).toHaveAttribute("href", PUBLIC_ACCESS_URL);
    expect(screen.getByText(/sample run, no changes applied/)).toBeInTheDocument();
  });
  it("opens the exact evidence without navigating or claiming a live connection", () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return new DOMRect(0, this.getAttribute("role") === "log" ? 20 : 200, 600, 200);
    });
    render(<HowItWorks job={DEFAULT_JOB} />);
    const conversation = screen.getByRole("log", { name: "Example conversation" });
    const citation = screen.getByRole("button", { name: "E2 · GitHub" });
    fireEvent.click(citation);
    const evidence = screen.getByRole("region", { name: "Sample evidence" });
    expect(evidence).toHaveTextContent("Number(process.env.BROWSER_TIMEOUT) || 5");
    expect(evidence).toHaveTextContent("Run output is illustrative; linked repository code is real.");
    expect(within(evidence).getByRole("link", { name: "View repository source" })).toHaveAttribute("href", DEFAULT_JOB.evidence[1].url);
    expect(conversation.scrollTop).toBe(180);
    const close = within(evidence).getByRole("button", { name: "Close evidence" });
    close.focus(); fireEvent.click(close);
    expect(screen.queryByRole("region", { name: "Sample evidence" })).toBeNull();
    expect(conversation.scrollTop).toBe(0);
    expect(citation).toHaveFocus();
  });
  it("still explains the result when there are no evidence fixtures", () => {
    render(<HowItWorks job={{ ...DEFAULT_JOB, evidence: [] }} />);
    expect(screen.getByText(DEFAULT_JOB.report)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /E1/ })).toBeNull();
  });
  it("keeps sample log evidence readable without inventing a source URL", () => {
    render(<HowItWorks job={DEFAULT_JOB} />);
    fireEvent.click(screen.getByRole("button", { name: "E1 · Grafana Loki" }));
    const evidence = screen.getByRole("region", { name: "Sample evidence" });
    expect(evidence).toHaveTextContent(DEFAULT_JOB.evidence[0].excerpt.replaceAll("\n", " "));
    expect(within(evidence).queryByRole("link")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "E2 · GitHub" }));
    expect(screen.getByRole("region", { name: "Sample evidence" })).toHaveTextContent(DEFAULT_JOB.evidence[1].detail);
  });
});
