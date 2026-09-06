import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AudienceSection } from "./AdoptionSections";

describe("Audience guidance", () => {
  it("gives founders outcomes and infrastructure leads inspectable controls", () => {
    render(<AudienceSection />);
    expect(screen.getByTestId("audience-founders")).toHaveTextContent(/pull requests/i);
    expect(screen.getByTestId("audience-infra")).toHaveTextContent(/permissions/i);
    expect(screen.getByTestId("audience-infra")).toHaveTextContent(/run history.*evidence.*spending/i);
  });

  it("does not promise staffing savings, instant setup, or autonomous releases", () => {
    render(<AudienceSection />);
    expect(screen.getByTestId("audience-section")).not.toHaveTextContent(/replace your team|no setup|autonomous|save \d/i);
    expect(screen.getByTestId("audience-founders")).toHaveTextContent(/keep the final call/i);
  });

});
