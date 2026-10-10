import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { within } from "@testing-library/react";
import { FLEET_LIBRARY_TITLE } from "../app/(dashboard)/admin/fleet-libraries/library-copy";
import { INVITES_TITLE } from "../app/(dashboard)/invites/copy";
import { MEMBERS_TITLE } from "../app/(dashboard)/settings/members/copy";
import { SECRETS_PAGE_TITLE } from "../app/(dashboard)/w/[workspaceId]/secrets/copy";
import { MODELS_PAGE_TITLE } from "../app/(dashboard)/w/[workspaceId]/settings/models/copy";

// Each Next.js segment has its own loading.tsx Suspense fallback, and every one
// must render. Where the page renders its title from a shared copy constant,
// the loader must paint that same constant, so a route swap shows the page's
// own header instead of a neighbour's. A null title is a skeleton loader, or a
// page that spells its title inline, so there is no shared constant to compare.

describe("dashboard segment loading states", () => {
  const cases: Array<{ name: string; importer: () => Promise<{ default: React.ComponentType }>; pageTitle: string | null }> = [
    {
      name: "fleets/[id]",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/loading"),
      pageTitle: null,
    },
    {
      // /settings is a bare redirect to /settings/api-keys now,
      // no loading.tsx of its own — its former loader was the Workspace tab's.
      name: "settings/api-keys",
      importer: () => import("../app/(dashboard)/settings/api-keys/loading"),
      pageTitle: null,
    },
    {
      name: "settings/models",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/settings/models/loading"),
      pageTitle: MODELS_PAGE_TITLE,
    },
    {
      name: "settings/billing",
      importer: () => import("../app/(dashboard)/settings/billing/loading"),
      pageTitle: null,
    },
    {
      name: "settings/members",
      importer: () => import("../app/(dashboard)/settings/members/loading"),
      pageTitle: MEMBERS_TITLE,
    },
    {
      // One loader covers both Invites routes; the linked one nests under it.
      name: "invites",
      importer: () => import("../app/(dashboard)/invites/loading"),
      pageTitle: INVITES_TITLE,
    },
    {
      // Secrets is its own standalone page — its loader paints the real
      // title, not the stale "Models" it borrowed when /credentials redirected.
      name: "secrets",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/secrets/loading"),
      pageTitle: SECRETS_PAGE_TITLE,
    },
    {
      name: "admin/runners",
      importer: () => import("../app/(dashboard)/admin/runners/loading"),
      pageTitle: null,
    },
    {
      name: "admin/models",
      importer: () => import("../app/(dashboard)/admin/models/loading"),
      pageTitle: null,
    },
    {
      name: "admin/fleet-libraries",
      importer: () => import("../app/(dashboard)/admin/fleet-libraries/loading"),
      pageTitle: FLEET_LIBRARY_TITLE,
    },
    {
      name: "integrations",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/integrations/loading"),
      pageTitle: null,
    },
    {
      name: "events",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/events/loading"),
      pageTitle: null,
    },
    {
      name: "approvals",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/approvals/loading"),
      pageTitle: null,
    },
    {
      name: "approvals/[gateId]",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/loading"),
      pageTitle: null,
    },
    {
      name: "workspace home",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/loading"),
      pageTitle: null,
    },
  ];

  for (const { name, importer, pageTitle } of cases) {
    it(`${name} loading renders${pageTitle ? " the page's own title" : ""}`, async () => {
      const { default: Loading } = await importer();
      const markup = renderToStaticMarkup(React.createElement(Loading));
      expect(markup).not.toBe("");
      if (!pageTitle) return;
      const host = document.createElement("div");
      host.innerHTML = markup;
      expect(within(host).getByRole("heading", { level: 1 }).textContent).toBe(pageTitle);
    });
  }
});
