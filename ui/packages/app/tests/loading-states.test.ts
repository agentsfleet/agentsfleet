import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { within } from "@testing-library/react";

describe("shared page loading", () => {
  const routes = [
    { name: "cli-auth/loading.tsx", load: () => import("../app/cli-auth/loading") },
    { name: "connectors/loading.tsx", load: () => import("../app/connectors/loading") },
    { name: "(dashboard)/loading.tsx", load: () => import("../app/(dashboard)/loading") },
    { name: "(dashboard)/invites/loading.tsx", load: () => import("../app/(dashboard)/invites/loading") },
    { name: "(dashboard)/settings/members/loading.tsx", load: () => import("../app/(dashboard)/settings/members/loading") },
    { name: "(dashboard)/w/[workspaceId]/turns/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/turns/loading") },
    { name: "(dashboard)/w/[workspaceId]/fleets/[id]/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/loading") },
    { name: "(dashboard)/w/[workspaceId]/fleets/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/fleets/loading") },
    { name: "(dashboard)/w/[workspaceId]/fleets/new/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/fleets/new/loading") },
    { name: "(dashboard)/w/[workspaceId]/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/loading") },
    { name: "(dashboard)/w/[workspaceId]/secrets/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/secrets/loading") },
    { name: "(dashboard)/w/[workspaceId]/settings/models/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/settings/models/loading") },
  ];
  for (const { name, load } of routes) {
    it(`${name} shows the common accessible indicator`, async () => {
      const { default: Loading } = await load();
      const host = document.createElement("div");
      host.innerHTML = renderToStaticMarkup(React.createElement(Loading));
      const status = within(host).getByRole("status", { name: "Loading page" });
      expect(within(status).getByText("Loading…")).toBeTruthy();
      expect(status.querySelectorAll(".page-loading-stroke")).toHaveLength(8);
      expect(Boolean(within(status).queryByText("agentsfleet"))).toBe(!name.startsWith("(dashboard)"));
      expect(host.querySelector(".animate-pulse")).toBeNull();
      expect(host.querySelector("h1")).toBeNull();
    });
  }

  it("renders the connected apps header and one loading region on first navigation", async () => {
    const [{ default: Layout }, { default: Loading }] = await Promise.all([
      import("../app/(dashboard)/w/[workspaceId]/integrations/layout"),
      import("../app/(dashboard)/w/[workspaceId]/integrations/loading"),
    ]);
    const host = document.createElement("div");
    host.innerHTML = renderToStaticMarkup(React.createElement(Layout, null, React.createElement(Loading)));
    const page = within(host);
    expect(page.getByRole("heading", { name: "Connected apps" })).toBeTruthy();
    expect(page.getAllByRole("status")).toHaveLength(1);
    expect(page.getByRole("status", { name: "Loading connected apps" }).getAttribute("aria-busy")).toBe("true");
    expect(host.querySelector(".page-loading-stroke")).toBeNull();
  });
});
