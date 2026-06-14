import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Footer from "./Footer";

function renderFooter() {
  return render(
    <BrowserRouter>
      <Footer />
    </BrowserRouter>
  );
}

describe("Footer", () => {
  it("renders the brand name", () => {
    renderFooter();
    expect(screen.getByText(/^usezombie$/)).toBeInTheDocument();
  });

  it("renders the tagline without the self-managed/open-source tail", () => {
    renderFooter();
    expect(
      screen.getByText(/durable, markdown-defined agents that wake on your events/i),
    ).toBeInTheDocument();
    // "Self-managed. Open source." was pulled from the footer tagline.
    expect(screen.queryByText(/Self-managed\. Open source\./)).not.toBeInTheDocument();
  });

  it("renders product column with links", () => {
    renderFooter();
    expect(screen.getByText(/^product$/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^features$/i })).toHaveAttribute("href", "/");
    expect(screen.getByRole("link", { name: /^pricing$/i })).toHaveAttribute("href", "/#pricing");
    expect(screen.getByRole("link", { name: /^docs$/i })).toHaveAttribute("href", "https://docs.agentsfleet.net");
    expect(screen.getByRole("link", { name: /^agents$/i })).toHaveAttribute("href", "/agents");
  });

  it("renders community column with canonical Discord URL", () => {
    renderFooter();
    expect(screen.getByText(/^community$/i)).toBeInTheDocument();
    const github = screen.getByRole("link", { name: /^github$/i });
    expect(github).toHaveAttribute("href", "https://github.com/agentsfleet/usezombie");
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
