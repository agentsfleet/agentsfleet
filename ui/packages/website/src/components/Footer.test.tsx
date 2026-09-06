import { WAITLIST_URL } from "../config";
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

  it("offers direct Contact without duplicate GitHub or legal links", () => {
    renderFooter();
    const contact = screen.getByRole("link", { name: /^contact$/i });
    expect(contact).toHaveAttribute("href", `mailto:${SUPPORT_EMAIL}`);
    expect(contact).not.toHaveAttribute("target");
    for (const name of [/^github$/i, /^privacy$/i, /^terms$/i]) {
      expect(screen.getAllByRole("link", { name })).toHaveLength(1);
    }
    fireEvent.click(contact);
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({ source: "footer_contact", surface: "footer", target: "contact" });
  });
  it("renders the brand name", () => {
    renderFooter();
    expect(screen.getByText(/^agentsfleet$/)).toBeInTheDocument();
  });

  it("describes incident investigation and user approval", () => {
    renderFooter();
    expect(
      screen.getByText(/AI teammates that investigate incidents and help prepare fixes/i),
    ).toBeInTheDocument();
    // "Self-managed. Open source." was pulled from the footer tagline.
    expect(screen.queryByText(/Self-managed\. Open source\./)).not.toBeInTheDocument();
  });

  it("renders product column with links", () => {
    renderFooter();
    expect(screen.getByText(/^product$/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^Use cases$/i })).toHaveAttribute("href", "/#operational-loop");
    expect(screen.queryByRole("link", { name: /^early access$/i })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^Dashboard$/i })).toHaveAttribute("href", WAITLIST_URL);
  });

  it("offers docs and agent resources", () => {
    renderFooter();
    expect(screen.getByText(/^resources$/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^docs$/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
    expect(screen.getByRole("link", { name: "agents" })).toHaveAttribute("href", "/agents");
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
