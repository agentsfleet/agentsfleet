import { agentDisplayName } from "@/lib/fleets/agent-label";
import { FLEET_ID } from "../helpers/draft-fixtures";
import { detailResponse, exhaustedBilling, happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { render, screen } from "@testing-library/react";
import { fetchMock, authMock as auth } from "../helpers/dashboard-mocks";

describe("fleets routes — detail views", () => {
  it("fleets detail page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it.each([400, 404])("fleets detail uses notFound for a malformed or missing id (%i)", async (status) => {
    mockFetchBilling(happyBilling, {missingStatus: status});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "missing" }) }),
    ).rejects.toThrow("notFound");
  });

  it("fleets detail page rethrows a server failure instead of rendering notFound", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith(`/fleets/${FLEET_ID}`)) {
        return {
          ok: false,
          status: 500,
          headers: { get: () => null },
          json: async () => ({
            error_code: "UZ-INTERNAL-001",
            detail: "Fleet read failed",
          }),
        };
      }
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/memories")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], total: 0, next_cursor: null }),
        };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], next_cursor: null }),
      };
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }) }),
    ).rejects.toThrow("Fleet read failed");
  });

  it("fleet Activity view loads the URL-selected standard-table window", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        searchParams: Promise.resolve({ view: "events", ps: "50" }),
      }),
    );
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls).toContainEqual(
      expect.stringContaining(`/fleets/${FLEET_ID}/events?limit=50`),
    );
    expect(urls.some((url) => url.includes("since="))).toBe(false);
    expect(markup).toContain(
      `href="/w/ws_1/fleets/${FLEET_ID}?view=events" aria-current="page"`,
    );
  });

  it("fleet Activity view threads the URL cursor into the events fetch", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        // The cursor rides the same URL as the open view; the server fetches
        // that page rather than resetting to the newest.
        searchParams: Promise.resolve({
          view: "events",
          c: "tok_fleet",
          cps: "25",
        }),
      }),
    );
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls.some((url) => url.includes("cursor=tok_fleet"))).toBe(true);
  });

  it("fleet Activity view propagates failed history to the retry boundary", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/events")) throw new Error("history down");
      return detailResponse();
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(Page({
      params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      searchParams: Promise.resolve({ view: "events" }),
    })).rejects.toThrow("history down");
  });

  it("fleet Memory view renders stored entries and a fetch failure separately", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const emptyMarkup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        searchParams: Promise.resolve({ view: "memory" }),
      }),
    );
    expect(emptyMarkup).toContain("Nothing learned yet");

    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/memories")) throw new Error("memory down");
      return detailResponse();
    });
    const unavailableMarkup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        searchParams: Promise.resolve({ view: "memory" }),
      }),
    );
    expect(unavailableMarkup).toContain("Memory is temporarily unavailable");
  });

  it("fleet Chat remains available when the thread read fails, and never reads the inbox", async () => {
    const asked: string[] = [];
    fetchMock.mockImplementation(async (url: string) => {
      asked.push(url);
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      // `/messages` is the chat's thread read — the one request that replaced
      // the events-list-plus-per-turn-detail fan-out this view used to issue.
      if (url.includes("/messages") || url.includes("/events")) {
        throw new Error("summary down");
      }
      return detailResponse({ pending_approvals: 2 });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).toContain("Latest outcome");
    expect(markup).toContain("Latest data unavailable.");
    expect(markup).not.toContain("No outcome recorded yet.");
    // The pending count comes off the fleet detail the page already holds:
    // no approvals read is issued to render it.
    expect(markup).toContain("2 approvals waiting");
    expect(asked.some((url) => url.includes("/approvals"))).toBe(false);
    expect(markup).toContain("Chat");
  });

  it("fleets detail page renders panels + exhaustion badge when tenant is exhausted", async () => {
    mockFetchBilling(exhaustedBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).toContain(agentDisplayName(FLEET_ID));
    expect(markup).toContain("Balance exhausted");
  });

  it("renders fleet-local navigation with Chat as the focused default", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).toContain('aria-label="Fleet sections"');
    expect(markup).toContain(`href="/w/ws_1/fleets/${FLEET_ID}" aria-current="page"`);
    expect(markup).toContain("Chat");
    expect(markup).toContain("Activity");
    expect(markup).toContain("Memory");
    expect(markup).toContain("Setup");
    expect(markup).not.toContain("?view=trigger");
    expect(markup).not.toContain("Settings");
    // The panel paints in the same pass as the tabs. A skeleton between them
    // stays up at least 300 ms once shown, and made Chat land later than its reads.
    expect(markup).toContain('aria-label="Fleet summary"');
    expect(markup).toContain('aria-label="Fleet lifecycle actions"');
    expect(markup).not.toContain("fleet-header-alignment-spacer");
    expect(markup).not.toContain("lg:w-56");
    expect(markup).toContain("Stop");
    expect(markup).toContain("Kill");
    expect(markup).not.toContain("What it knows");
  });

  it("fleets detail page renders without badge when not exhausted", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).not.toContain("Balance exhausted");
  });

  it("fleet Setup view shows saved instructions and links to private improvement", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    render(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        searchParams: Promise.resolve({ view: "setup" }),
      }),
    );
    expect(await screen.findByText("Investigate failures.")).not.toBeNull();
    expect(screen.getByRole("heading", {name: "What this fleet does"})).not.toBeNull();
    expect(screen.getByRole("link", {name: "Edit fleet"}).getAttribute("href")).toBe(`/w/ws_1/fleets/new?fleet=${FLEET_ID}&setup_revision=1`);
    expect(screen.queryByText("Skill source")).toBeNull();
    expect(screen.queryByText("Latest outcome")).toBeNull();
  });
});
