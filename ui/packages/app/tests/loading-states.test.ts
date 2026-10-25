import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { within } from "@testing-library/react";

describe("shared page loading", () => {
  const routes = [
    { name: "loading.tsx", load: () => import("../app/loading") },
    { name: "(dashboard)/loading.tsx", load: () => import("../app/(dashboard)/loading") },
    { name: "(dashboard)/admin/fleet-libraries/loading.tsx", load: () => import("../app/(dashboard)/admin/fleet-libraries/loading") },
    { name: "(dashboard)/admin/models/loading.tsx", load: () => import("../app/(dashboard)/admin/models/loading") },
    { name: "(dashboard)/admin/runners/[runnerId]/loading.tsx", load: () => import("../app/(dashboard)/admin/runners/[runnerId]/loading") },
    { name: "(dashboard)/admin/runners/loading.tsx", load: () => import("../app/(dashboard)/admin/runners/loading") },
    { name: "(dashboard)/invites/loading.tsx", load: () => import("../app/(dashboard)/invites/loading") },
    { name: "(dashboard)/settings/account/loading.tsx", load: () => import("../app/(dashboard)/settings/account/loading") },
    { name: "(dashboard)/settings/api-keys/loading.tsx", load: () => import("../app/(dashboard)/settings/api-keys/loading") },
    { name: "(dashboard)/settings/billing/loading.tsx", load: () => import("../app/(dashboard)/settings/billing/loading") },
    { name: "(dashboard)/settings/members/loading.tsx", load: () => import("../app/(dashboard)/settings/members/loading") },
    { name: "(dashboard)/w/[workspaceId]/approvals/[gateId]/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/loading") },
    { name: "(dashboard)/w/[workspaceId]/approvals/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/approvals/loading") },
    { name: "(dashboard)/w/[workspaceId]/events/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/events/loading") },
    { name: "(dashboard)/w/[workspaceId]/fleets/[id]/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/loading") },
    { name: "(dashboard)/w/[workspaceId]/fleets/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/fleets/loading") },
    { name: "(dashboard)/w/[workspaceId]/fleets/new/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/fleets/new/loading") },
    { name: "(dashboard)/w/[workspaceId]/integrations/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/integrations/loading") },
    { name: "(dashboard)/w/[workspaceId]/library/loading.tsx", load: () => import("../app/(dashboard)/w/[workspaceId]/library/loading") },
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
      expect(status.textContent).toBe("Loading…");
      expect(host.querySelector(".animate-pulse")).toBeNull();
      expect(host.querySelector("h1")).toBeNull();
    });
  }
});
