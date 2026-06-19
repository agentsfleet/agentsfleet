import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Each Next.js segment has its own loading.tsx Suspense fallback. The
// skeleton shapes must match the eventual page chrome (PageHeader +
// SectionLabel placement) so a route swap does not cause layout shift.
// Rendering each one drives the inner Array.from arrow callbacks too,
// which v8 counts as separate functions.

describe("dashboard segment loading states", () => {
  const cases: Array<{ name: string; importer: () => Promise<{ default: React.ComponentType }>; expectsTitle: string | null }> = [
    {
      name: "agents/[id]",
      importer: () => import("../app/(dashboard)/agents/[id]/loading"),
      expectsTitle: null, // skeleton title, no static text
    },
    {
      name: "settings",
      importer: () => import("../app/(dashboard)/settings/loading"),
      expectsTitle: "Workspace",
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
    it(`${name} loading renders skeleton chrome`, async () => {
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
