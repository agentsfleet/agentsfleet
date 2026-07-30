import React, { type ReactElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { Time, TooltipProvider } from "@agentsfleet/design-system";

// The root layout is a Server Component that reads a cookie and mounts Clerk.
// Neither is what these tests are about, so both are stubbed down to the point
// where the returned element TREE is inspectable — the tree is the assertion.
vi.mock("next/headers", () => ({
  cookies: () => Promise.resolve({ get: () => undefined }),
}));
vi.mock("@/lib/auth/client", () => ({
  AuthProvider: ({ children }: { children: ReactNode }) => children,
  AuthSessionKeeper: () => null,
}));
vi.mock("@/components/analytics/AnalyticsBootstrap", () => ({
  default: () => null,
}));

import RootLayout from "./layout";

const CHILD_MARKER = "child-marker";

// Walk the returned tree and collect every node of a given component type.
// Rendering is not an option here: the layout returns <html><body>, which
// cannot be mounted into a jsdom body without React complaining about nesting.
function findAll(node: ReactNode, type: unknown): ReactElement[] {
  if (!React.isValidElement(node)) return [];
  const self = node.type === type ? [node] : [];
  const kids = (node.props as { children?: ReactNode }).children;
  const descendants = React.Children.toArray(kids).flatMap((child) =>
    findAll(child, type),
  );
  return [...self, ...descendants];
}

describe("root layout — the app's single tooltip provider", () => {
  it("mounts exactly one TooltipProvider", async () => {
    const tree = await RootLayout({
      children: React.createElement("div", null, CHILD_MARKER),
    });

    // Exactly one, not at-least-one: a second provider nested below this would
    // start its own delay-coordination group, so tooltips in different islands
    // would stop sharing the skip-delay window that makes a row of them feel
    // like one surface.
    expect(findAll(tree, TooltipProvider)).toHaveLength(1);
  });

  it("mounts it ABOVE children, so every route group inherits it", async () => {
    const tree = await RootLayout({
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
    // If Radix ever softens that, this test fails and the root mount stops
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
