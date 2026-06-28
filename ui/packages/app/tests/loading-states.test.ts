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
      importer: () => import("../app/(dashboard)/fleets/[id]/loading"),
      expectsTitle: null, // skeleton title, no static text
    },
    {
      name: "settings",
      importer: () => import("../app/(dashboard)/settings/loading"),
      expectsTitle: "Workspace",
    },
    {
      name: "settings/models",
      importer: () => import("../app/(dashboard)/settings/models/loading"),
      expectsTitle: "Models &amp; Keys", // renderToStaticMarkup escapes the ampersand
    },
    {
      name: "settings/billing",
      importer: () => import("../app/(dashboard)/settings/billing/loading"),
      expectsTitle: "Billing",
    },
    {
      name: "credentials",
      importer: () => import("../app/(dashboard)/credentials/loading"),
      expectsTitle: "Credentials",
    },
    {
      name: "integrations",
      importer: () => import("../app/(dashboard)/integrations/loading"),
      expectsTitle: "Integrations",
    },
    {
      name: "events",
      importer: () => import("../app/(dashboard)/events/loading"),
      expectsTitle: "Events",
    },
    {
      name: "approvals",
      importer: () => import("../app/(dashboard)/approvals/loading"),
      expectsTitle: "Approvals",
    },
    {
      name: "approvals/[gateId]",
      importer: () => import("../app/(dashboard)/approvals/[gateId]/loading"),
      expectsTitle: null, // skeleton title
    },
    {
      name: "(dashboard) root",
      importer: () => import("../app/(dashboard)/loading"),
      expectsTitle: null, // dashboard-wide fallback — skeleton only, no static text
    },
  ];

  for (const { name, importer, expectsTitle } of cases) {
    it(`${name} loading renders loading chrome`, async () => {
      const { default: Loading } = await importer();
      const markup = renderToStaticMarkup(React.createElement(Loading));
      // Every loading state participates in the design-system skeleton
      // primitive — its data signature is the surface-2 token + reduced-
      // motion-aware shimmer. Smoke-check the markup contains a div.
      expect(markup).toContain("<div");
      if (expectsTitle) {
        expect(markup).toContain(expectsTitle);
      }
    });
  }
});
