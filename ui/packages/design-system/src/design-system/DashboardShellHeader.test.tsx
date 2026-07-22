import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { DashboardShellHeader } from "./DashboardShellHeader";

describe("DashboardShellHeader", () => {
  it("renders semantic dashboard chrome with an overlaid one-pixel divider", () => {
    const { getByRole } = render(<DashboardShellHeader>agentsfleet</DashboardShellHeader>);
    const header = getByRole("banner");

    expect(header.tagName).toBe("HEADER");
    expect(header.className).toContain("after:h-px");
    expect(header.className).not.toContain("border-b");
  });
});
