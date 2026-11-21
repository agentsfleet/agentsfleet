import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { fetchMock, resetCommonMocks } from "./helpers/dashboard-mocks";

// Shared dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.stubGlobal("fetch", fetchMock);
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("@phosphor-icons/react/ssr", async () => (await import("./helpers/dashboard-mocks")).phosphorMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemTabs() };
});

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/w/ws_1/fleets" });
});
afterEach(() => {
  cleanup();
  fetchMock.mockReset();
});

describe("credit exhaustion badge", () => {
  it("ExhaustionBadge renders with a timestamped tooltip when exhaustedAt set", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/usage/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: "2024-04-21T11:46:40Z" }));
    const el = screen.getByRole("status", { name: /balance exhausted/i });
    expect(el).toBeTruthy();
    expect(el.getAttribute("title")).toMatch(/^Exhausted since /);
  });

  it("ExhaustionBadge renders with generic title when exhaustedAt is null", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/usage/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: null }));
    expect(screen.getByRole("status").getAttribute("title")).toBe("Balance exhausted");
  });
});
