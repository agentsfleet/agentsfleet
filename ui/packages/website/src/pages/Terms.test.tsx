import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Terms from "./Terms";
import { SUPPORT_EMAIL } from "../lib/contact";

describe("Terms", () => {
  it("renders the heading", () => {
    render(<Terms />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/terms of service/i);
  });

  it("states the approved starting price, included credit, and own-key billing", () => {
    render(<Terms />);
    expect(screen.getByTestId("terms-page")).toHaveTextContent("Fleet runtime starts at $0.");
    expect(screen.getByTestId("terms-page")).toHaveTextContent("Run your first jobs with $25 in included credit.");
    expect(screen.getByTestId("terms-page")).toHaveTextContent("If you bring your own key, usage is billed by your provider directly.");
    expect(screen.getByTestId("terms-page")).not.toHaveTextContent(/early access|model credit|zero markup/i);
    expect(screen.getByTestId("terms-page")).not.toHaveTextContent(/enterprise tier/i);
  });

  it("renders contact email link sourced from SUPPORT_EMAIL", () => {
    render(<Terms />);
    expect(screen.getByRole("link", { name: SUPPORT_EMAIL })).toHaveAttribute(
      "href",
      `mailto:${SUPPORT_EMAIL}`,
    );
  });
});
