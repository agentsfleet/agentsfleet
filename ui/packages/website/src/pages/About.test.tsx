import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SUPPORT_EMAIL } from "../lib/contact";
import About from "./About";

const analytics = vi.hoisted(() => ({ trackNavigationClicked: vi.fn() }));
vi.mock("../analytics/posthog", () => analytics);

describe("About", () => {
  beforeEach(() => analytics.trackNavigationClicked.mockReset());

  it("explains the product and early-access purpose without invented company credentials", () => {
    render(<About />);
    const page = screen.getByTestId("about-page");
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(page).toHaveTextContent(/solo founders and infrastructure teams/i);
    expect(page).toHaveTextContent(/you choose access.*decide what ships/i);
    expect(page).not.toHaveTextContent(/SOC.?2|backed by|trusted by|\$\d|starter credit/i);
  });

  it("offers direct contact without collecting personal data in a form", () => {
    const { container } = render(<About />);
    const link = screen.getByRole("link", { name: SUPPORT_EMAIL });
    expect(link).toHaveAttribute("href", `mailto:${SUPPORT_EMAIL}`);
    expect(link).not.toHaveAttribute("target");
    expect(container.querySelector("form")).toBeNull();
    expect(screen.queryByRole("textbox")).toBeNull();
    fireEvent.click(link);
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "about_contact", surface: "about", target: "contact",
    });
  });
});
