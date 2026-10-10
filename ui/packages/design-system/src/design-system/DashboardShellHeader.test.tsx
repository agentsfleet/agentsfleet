import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { DashboardShellHeader } from "./DashboardShellHeader";

describe("DashboardShellHeader", () => {
  it("renders the dashboard chrome as the page banner", () => {
    const { getByRole } = render(<DashboardShellHeader>agentsfleet</DashboardShellHeader>);
    const header = getByRole("banner");

    expect(header.tagName).toBe("HEADER");
  });
});
