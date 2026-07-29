import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { RUNNER_VIEW } from "@/lib/runner-routes";
import { RunnerSubnavigation } from "./RunnerSubnavigation";

afterEach(() => cleanup());

describe("RunnerSubnavigation", () => {
  it("renders exactly the two rail items, Leases leading", () => {
    render(<RunnerSubnavigation runnerId="r-1" activeView={RUNNER_VIEW.leases} />);
    const links = screen.getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual(["Leases", "Activity"]);
    expect(links[0]?.getAttribute("href")).toBe("/admin/runners/r-1");
    expect(links[1]?.getAttribute("href")).toBe("/admin/runners/r-1?view=activity");
    expect(links[0]?.getAttribute("aria-current")).toBe("page");
  });

  it("marks Activity current when it is the active view", () => {
    render(<RunnerSubnavigation runnerId="r-1" activeView={RUNNER_VIEW.activity} />);
    const activity = screen.getByRole("link", { name: /activity/i });
    expect(activity.getAttribute("aria-current")).toBe("page");
  });
});
