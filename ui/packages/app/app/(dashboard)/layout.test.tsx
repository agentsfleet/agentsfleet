import React, { type ReactElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { Time, TooltipProvider } from "@agentsfleet/design-system";

// The dashboard layout is a Server Component that authenticates, lists
// workspaces, and reads operator scopes. None of that is what these tests are
// about, so each is stubbed to the point where the returned element TREE is
// inspectable — the tree is the assertion.
vi.mock("@clerk/nextjs/server", () => ({
  auth: () => Promise.resolve({ getToken: () => Promise.resolve(null) }),
}));
vi.mock("@/lib/workspace", () => ({
  listTenantWorkspacesCached: () => Promise.resolve({ items: [], total: 0 }),
}));
vi.mock("@/lib/auth/platform", () => ({
  readSessionScopes: () => Promise.resolve(new Set<string>()),
}));
vi.mock("@/components/layout/ShellFrame", () => ({
  ShellFrame: ({ children }: { children: ReactNode }) => children,
}));

import DashboardLayout from "./layout";

const CHILD_MARKER = "child-marker";

// Walk the returned tree and collect every node of a given component type.
// Rendering is not an option: the layout is async and its shell reaches for
// browser and Clerk context this test deliberately does not stand up.
function findAll(node: ReactNode, type: unknown): ReactElement[] {
  if (!React.isValidElement(node)) return [];
  const self = node.type === type ? [node] : [];
  const kids = (node.props as { children?: ReactNode }).children;
  const descendants = React.Children.toArray(kids).flatMap((child) =>
    findAll(child, type),
  );
  return [...self, ...descendants];
}

describe("dashboard layout — the segment's single tooltip provider", () => {
  it("mounts exactly one TooltipProvider", async () => {
    const tree = await DashboardLayout({
      children: React.createElement("div", null, CHILD_MARKER),
    });

    // Exactly one, not at-least-one: a second provider nested below this would
    // start its own delay-coordination group, so tooltips in different islands
    // would stop sharing the skip-delay window that makes a row of them feel
    // like one surface.
    expect(findAll(tree, TooltipProvider)).toHaveLength(1);
  });

  it("mounts it ABOVE children, so every dashboard route inherits it", async () => {
    const tree = await DashboardLayout({
      children: React.createElement("div", null, CHILD_MARKER),
    });

    const [provider] = findAll(tree, TooltipProvider);
    expect(provider).toBeDefined();
    const marker = React.Children.toArray(
      (provider?.props as { children?: ReactNode } | undefined)?.children,
    );
    expect(JSON.stringify(marker)).toContain(CHILD_MARKER);
  });

  it("is what keeps a relative Time from throwing", () => {
    // The failure this whole arrangement exists to prevent. `Time` defaults its
    // tooltip on for `format="relative"`, and Radix's Root reads provider
    // context unconditionally — so without a provider it THROWS rather than
    // degrading to a plain timestamp, taking the surrounding page down with it.
    // If Radix ever softens that, this test fails and the layout mount stops
    // being load-bearing — which is worth being told about.
    const relativeTime = React.createElement(Time, {
      value: new Date(Date.UTC(2026, 0, 2, 3, 4, 5)),
      format: "relative",
    });

    expect(() => render(relativeTime)).toThrow(/TooltipProvider/);
    expect(() =>
      render(relativeTime, { wrapper: TooltipProvider }),
    ).not.toThrow();
  });
});
