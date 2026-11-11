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

// ── ExhaustionBadge + ExhaustionBanner ─────────────────────────────────────

describe("exhaustion components", () => {
  it("ExhaustionBadge renders with a timestamped tooltip when exhaustedAt set", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: "2024-04-21T11:46:40Z" }));
    const el = screen.getByRole("status", { name: /balance exhausted/i });
    expect(el).toBeTruthy();
    expect(el.getAttribute("title")).toMatch(/^Exhausted since /);
  });

  it("ExhaustionBadge renders with generic title when exhaustedAt is null", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: null }));
    expect(screen.getByRole("status").getAttribute("title")).toBe("Balance exhausted");
  });

  it("ExhaustionBanner renders nothing when billing is null", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    const { container } = render(
      React.createElement(ExhaustionBanner, { billing: null }),
    );
    expect(container.innerHTML).toBe("");
  });

  it("ExhaustionBanner renders nothing when not exhausted", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    const { container } = render(
      React.createElement(ExhaustionBanner, {
        billing: {
          balance_nanos: "500000000",
          updated_at: "1970-01-01T00:00:00Z",
          is_exhausted: false,
          exhausted_at: null,
        },
      }),
    );
    expect(container.innerHTML).toBe("");
  });

  it("ExhaustionBanner renders destructive alert with timestamp when exhausted", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    render(
      React.createElement(ExhaustionBanner, {
        billing: {
          balance_nanos: "0",
          updated_at: "2024-04-21T11:46:40Z",
          is_exhausted: true,
          exhausted_at: "2024-04-21T11:53:20Z",
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).toContain("BALANCE_EXHAUSTED_POLICY");
    expect(alert.querySelector("a[href^='mailto:']")).toBeTruthy();
    expect(alert.textContent).toMatch(/Exhausted since /);
  });

  it("ExhaustionBanner omits timestamp text when exhausted_at is null", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    render(
      React.createElement(ExhaustionBanner, {
        billing: {
          balance_nanos: "0",
          updated_at: "1970-01-01T00:00:00Z",
          is_exhausted: true,
          exhausted_at: null,
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).not.toMatch(/Exhausted since /);
  });
});
