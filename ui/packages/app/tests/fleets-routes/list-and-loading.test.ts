import { exhaustedBilling, happyBilling, mockFetchBilling } from "./harness";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { fetchMock, authMock as auth } from "../helpers/dashboard-mocks";
import { listSecretsMock } from "../helpers/dashboard-app-mocks";

describe("fleets routes — list and loading", () => {
  it("loading.tsx renders a spinner with status role", async () => {
    const { default: Loading } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status");
    // Visible copy is a random waiting verb ("Brewing Fleets…"), so assert the
    // route name plus the stable accessible name rather than the old static
    // "Loading Fleets" — that string now lives only in aria-label.
    expect(el.textContent).toContain("Fleets");
    expect(el.getAttribute("aria-label")).toBe("Loading Fleets");
    // Branded WakePulse dot (data-live), not the off-system Loader2Icon spin.
    const dot = el.querySelector("[data-live]");
    expect(dot).toBeTruthy();
    expect(dot?.className).toContain("bg-pulse");
  });

  it("fleets list page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("fleets list page streams a skeleton before data", async () => {
    // FleetsData is an async child, so renderToStaticMarkup renders the Suspense
    // skeleton in its place; the header now lives inside FleetsData (it adapts to
    // Wall vs. checklist), so it too stays absent until the data streams in.
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    );
    expect(markup).toContain("animate-pulse"); // Skeleton fallback
    expect(markup).not.toContain("platform-ops"); // data not yet resolved
  });

  it("FleetsData returns null when the token is missing", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { FleetsData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    expect(await FleetsData({ workspaceId: "ws_1" })).toBeNull();
  });

  it("fleets list page renders the Getting Started checklist as its empty state (3.6), banner suppressed", async () => {
    // The onboarding gather reads secrets through the mocked client (the rest go
    // through fetch); an empty secret list keeps every step incomplete.
    listSecretsMock.mockResolvedValue({ secrets: [] });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      // Every onboarding read (fleets, secrets, events, provider, preferences)
      // returns empty — so the checklist renders with every step incomplete.
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [],
          total: 0,
          next_cursor: null,
          prefs: {},
        }),
      };
    });
    const { FleetsData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await FleetsData({ workspaceId: "ws_1" }),
      ),
    );
    // With zero fleets the Wall renders the checklist — not the old EmptyState.
    expect(markup).toContain("Getting started");
    expect(markup).toContain("Install a fleet");
    // The install step still routes to the library.
    expect(markup).toContain('href="/w/ws_1/fleets/new"');
    // The old empty-state affordances are gone.
    expect(markup).not.toContain("No fleets yet");
    expect(markup).not.toContain("credit balance is exhausted");
  });

  it("fleets list page renders populated list + exhaustion banner", async () => {
    mockFetchBilling(exhaustedBilling);
    const { FleetsData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await FleetsData({ workspaceId: "ws_1" }),
      ),
    );
    expect(markup).toContain('href="/w/ws_1/fleets/zom_1"');
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("credit balance is exhausted");
  });

  it("fleets list page swallows a failed billing fetch and still renders", async () => {
    listSecretsMock.mockResolvedValue({ secrets: [] });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return {
          ok: false,
          status: 500,
          statusText: "err",
          json: async () => ({}),
        };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [],
          total: 0,
          next_cursor: null,
          prefs: {},
        }),
      };
    });
    const { FleetsData } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await FleetsData({ workspaceId: "ws_1" }),
      ),
    );
    expect(markup).toContain("Getting started");
  });
});
