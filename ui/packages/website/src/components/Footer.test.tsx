import { fireEvent, render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, it, expect, vi } from "vitest";
import Footer from "./Footer";
import { SUPPORT_EMAIL } from "../lib/contact";

const analytics = vi.hoisted(() => ({ trackNavigationClicked: vi.fn() }));
vi.mock("../analytics/posthog", () => analytics);

function renderFooter() {
  return render(
    <BrowserRouter>
      <Footer />
    </BrowserRouter>
  );
}

describe("Footer", () => {
  beforeEach(() => analytics.trackNavigationClicked.mockReset());

  it("adds About and direct Contact without duplicate GitHub or legal links", () => {
    renderFooter();
    const about = screen.getByRole("link", { name: /^about$/i });
    const contact = screen.getByRole("link", { name: /^contact$/i });
    expect(about).toHaveAttribute("href", "/about");
    expect(contact).toHaveAttribute("href", `mailto:${SUPPORT_EMAIL}`);
    expect(contact).not.toHaveAttribute("target");
    for (const name of [/^github$/i, /^privacy$/i, /^terms$/i]) {
      expect(screen.getAllByRole("link", { name })).toHaveLength(1);
    }
    fireEvent.click(about);
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({ source: "footer_about", surface: "footer", target: "about" });
    fireEvent.click(contact);
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({ source: "footer_contact", surface: "footer", target: "contact" });
  });
  it("renders the brand name", () => {
    renderFooter();
    expect(screen.getByText(/^agentsfleet$/)).toBeInTheDocument();
  });

  it("renders the warm teammates tagline without the self-managed/open-source tail", () => {
    renderFooter();
    expect(
      screen.getByText(/prebuilt ai teammates that take the recurring engineering work/i),
    ).toBeInTheDocument();
    // "Self-managed. Open source." was pulled from the footer tagline.
    expect(screen.queryByText(/Self-managed\. Open source\./)).not.toBeInTheDocument();
  });

  it("renders product column with links", () => {
    renderFooter();
    expect(screen.getByText(/^product$/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^fleet$/i })).toHaveAttribute("href", "/#operational-loop");
    expect(screen.getByRole("link", { name: /^early access$/i })).toHaveAttribute("href", "/#pricing");
    expect(screen.getByRole("link", { name: /^fleets$/i })).toHaveAttribute("href", "/fleets");
  });

  it("renders resources column with docs and machine-readable surfaces", () => {
    renderFooter();
    expect(screen.getByText(/^resources$/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^docs$/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
    expect(screen.getByRole("link", { name: /^llms\.txt$/i })).toHaveAttribute(
      "href",
      "/llms.txt",
    );
    expect(screen.getByRole("link", { name: /^llms-full\.txt$/i })).toHaveAttribute(
      "href",
      "/llms-full.txt",
    );
    expect(screen.getByRole("link", { name: /^OpenAPI$/ })).toHaveAttribute(
      "href",
      "/openapi.json",
    );
  });

  it("renders community column with canonical Discord URL", () => {
    renderFooter();
    expect(screen.getByText(/^community$/i)).toBeInTheDocument();
    const github = screen.getByRole("link", { name: /^github$/i });
    expect(github).toHaveAttribute("href", "https://github.com/agentsfleet/agentsfleet");
    expect(github).toHaveAttribute("target", "_blank");
    expect(github).toHaveAttribute("rel", "noopener noreferrer");

    const discord = screen.getByRole("link", { name: /^discord$/i });
    expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
    expect(discord).toHaveAttribute("target", "_blank");
    expect(discord).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("renders legal column with router links", () => {
    renderFooter();
    expect(screen.getByText(/^legal$/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^privacy$/i })).toHaveAttribute("href", "/privacy");
    expect(screen.getByRole("link", { name: /^terms$/i })).toHaveAttribute("href", "/terms");
  });

  it("renders copyright with current year", () => {
    renderFooter();
    const year = new Date().getFullYear().toString();
    expect(screen.getByText(new RegExp(year))).toBeInTheDocument();
  });
});
