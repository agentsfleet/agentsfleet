import { FLEET_ID } from "../helpers/draft-fixtures";
import { exhaustedBilling, happyBilling, mockFetchBilling } from "./harness";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { fetchMock, authMock as auth } from "../helpers/dashboard-mocks";
import { listSecretsMock } from "../helpers/dashboard-app-mocks";

describe("fleets routes — list and loading", () => {
  it("loading.tsx provides a stable accessible skeleton", async () => {
    const { default: Loading } = await import("../../app/(dashboard)/w/[workspaceId]/fleets/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status", { name: "Loading Fleets" });
    expect(el.querySelectorAll(".animate-pulse").length).toBeGreaterThan(0);
    expect(el.querySelector("[data-spinner-orbit]")).toBeNull();
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
    // skeleton in its place; the dashboard content waits for the fleet data.
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

  it("empty fleet dashboard offers chat creation without onboarding reads", async () => {
    listSecretsMock.mockResolvedValue({ secrets: [] });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [],
          total: null,
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
    expect(markup).toContain("What should your fleet do?");
    expect(markup).not.toContain("Your fleets");
    expect(markup).not.toContain("Getting started");
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(listSecretsMock).not.toHaveBeenCalled();
    // Creation starts in private chat without selecting a library revision.
    expect(markup).toContain('href="/w/ws_1/fleets/new"');
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
    expect(markup).toContain(`href="/w/ws_1/fleets/${FLEET_ID}"`);
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
          total: null,
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
    expect(markup).toContain("What should your fleet do?");
  });
});
