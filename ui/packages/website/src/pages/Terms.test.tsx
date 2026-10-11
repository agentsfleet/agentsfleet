import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Terms from "./Terms";
import { SUPPORT_EMAIL } from "../lib/contact";

describe("Terms", () => {
  it("renders the heading", () => {
    render(<Terms />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/terms of service/i);
  });

  it("publishes no price, credit, or enterprise tier in the terms", () => {
    render(<Terms />);
    expect(screen.getByTestId("terms-page")).not.toHaveTextContent(/starter credit|free to try|zero markup|\$\d/i);
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
