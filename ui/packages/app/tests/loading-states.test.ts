import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Each Next.js segment has its own loading.tsx Suspense fallback. The shared
// RouteLoading paints the page's exact title + description so a route swap shows
// the correct header instantly (no wobble) with one consistent spinner across
// routes. The title assertions below pin that each loader matches its page.

describe("dashboard segment loading states", () => {
  const cases: Array<{ name: string; importer: () => Promise<{ default: React.ComponentType }>; expectsTitle: string | null }> = [
    {
      name: "fleets/[id]",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/fleets/[id]/loading"),
      expectsTitle: null, // skeleton title, no static text
    },
    {
      // /settings is a bare redirect to /settings/api-keys now,
      // no loading.tsx of its own — its former loader was the Workspace tab's.
      name: "settings/api-keys",
      importer: () => import("../app/(dashboard)/settings/api-keys/loading"),
      expectsTitle: "API Keys",
    },
    {
      name: "settings/models",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/settings/models/loading"),
      expectsTitle: "Models",
    },
    {
      name: "settings/billing",
      importer: () => import("../app/(dashboard)/settings/billing/loading"),
      expectsTitle: "Billing",
    },
    {
      // Secrets is its own standalone page — its loader paints the real
      // title, not the stale "Models" it borrowed when /credentials redirected.
      name: "secrets",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/secrets/loading"),
      expectsTitle: "Secrets",
    },
    {
      name: "admin/runners",
      importer: () => import("../app/(dashboard)/admin/runners/loading"),
      expectsTitle: "Runners",
    },
    {
      name: "admin/models",
      importer: () => import("../app/(dashboard)/admin/models/loading"),
      expectsTitle: "Model library",
    },
    {
      name: "admin/fleet-libraries",
      importer: () => import("../app/(dashboard)/admin/fleet-libraries/loading"),
      expectsTitle: "Fleet library",
    },
    {
      name: "integrations",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/integrations/loading"),
      expectsTitle: "Integrations",
    },
    {
      name: "events",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/events/loading"),
      expectsTitle: "Events",
    },
    {
      name: "approvals",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/approvals/loading"),
      expectsTitle: "Approvals",
    },
    {
      name: "approvals/[gateId]",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/loading"),
      expectsTitle: null, // skeleton title
    },
    {
      name: "workspace home",
      importer: () => import("../app/(dashboard)/w/[workspaceId]/loading"),
      expectsTitle: null, // workspace-home fallback — spinner only, no static text
    },
  ];

  for (const { name, importer, expectsTitle } of cases) {
    it(`${name} loading renders loading chrome`, async () => {
      const { default: Loading } = await importer();
      const markup = renderToStaticMarkup(React.createElement(Loading));
      // A loader renders either a RouteLoading title + Spinner or a bespoke
      // Skeleton (the detail/settings routes); both emit a div. Smoke-check the
      // markup contains one — the title assertion below pins the rest.
      expect(markup).toContain("<div");
      if (expectsTitle) {
        expect(markup).toContain(expectsTitle);
      }
    });
  }
});
