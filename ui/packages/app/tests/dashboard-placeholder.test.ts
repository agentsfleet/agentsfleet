import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { mockAuthOnce as mockAuth } from "./helpers/dashboard-mocks";
import {
  resetDashboardMocks,
  listWorkspaceEventsMock,
} from "./helpers/dashboard-app-mocks";

// Common dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.mock("next/navigation", async () =>
  (await import("./helpers/dashboard-mocks")).nextNavigationMock(),
);
vi.mock("next/link", async () =>
  (await import("./helpers/dashboard-mocks")).nextLinkMock(),
);
vi.mock("@clerk/nextjs", async () =>
  (await import("./helpers/dashboard-mocks")).clerkMock(),
);
vi.mock("@clerk/nextjs/server", async () =>
  (await import("./helpers/dashboard-mocks")).clerkServerMock(),
);
vi.mock("@/lib/workspace", async () =>
  (await import("./helpers/dashboard-mocks")).workspaceMock(),
);
vi.mock("lucide-react", async () =>
  (await import("./helpers/dashboard-mocks")).lucideMock(),
);
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return {
    ...h.designSystemCore(await orig<Record<string, unknown>>()),
    ...h.designSystemDropdown(),
  };
});

// App-specific dashboard mocks — see tests/helpers/dashboard-app-mocks.tsx.
vi.mock("@/lib/api/fleets", async () =>
  (await import("./helpers/dashboard-app-mocks")).fleetsApiMock(),
);
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () =>
  (await import("./helpers/dashboard-app-mocks")).fleetActionsMock(),
);
vi.mock("@/lib/api/model_library", () => ({
  getModelLibrary: vi.fn().mockResolvedValue({models: [], next_cursor: null}),
}));
vi.mock("@/lib/api/tenant_billing", async () =>
  (await import("./helpers/dashboard-app-mocks")).tenantBillingMock(),
);
vi.mock("@/lib/api/tenant_provider", async () =>
  (await import("./helpers/dashboard-app-mocks")).tenantProviderMock(),
);
vi.mock("@/lib/api/events", async () =>
  (await import("./helpers/dashboard-app-mocks")).eventsMock(),
);
vi.mock("@/app/(dashboard)/actions", async () =>
  (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock(),
);

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  cleanup();
});

describe("placeholder pages", () => {
  it("settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null, userId: null });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("settings defaults page renders the masked placeholder when authenticated", async () => {
    mockAuth({ token: "tkn" });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/settings/defaults/page");
    const m = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    );
    expect(m).toContain("Defaults");
  });

  it("settings defaults page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/settings/defaults/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("settings security page renders the masked placeholder when authenticated", async () => {
    mockAuth({ token: "tkn" });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/settings/security/page");
    const m = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    );
    expect(m).toContain("Security");
  });

  it("settings security page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/settings/security/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("events page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/events/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("events page shows the shared loader before data", async () => {
    mockAuth({ token: "token_abc" });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/events/page");
    const m = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    );
    expect(m).not.toContain("<h1");
    expect(m).toContain('aria-label="Loading page"');
    expect(m).not.toContain("No activity yet"); // data not yet resolved
  });

  it("EventsData returns null when the token is missing", async () => {
    mockAuth({ token: null });
    const { EventsData } =
      await import("../app/(dashboard)/w/[workspaceId]/events/page");
    expect(await EventsData({ workspaceId: "ws_1" })).toBeNull();
  });

  it("events page renders Workspace activity section with EventsList", async () => {
    // M118: EventsData reads `workspaceId` from the route param (its prop); the
    // no-workspace `notFound()` is gone (the `[workspaceId]` layout guards it).
    mockAuth({ token: "token_abc" });
    listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
    const { EventsData } =
      await import("../app/(dashboard)/w/[workspaceId]/events/page");
    const { WORKSPACE_EVENTS_LABEL } =
      await import("../components/domain/EventsList");
    const m = renderToStaticMarkup(await EventsData({ workspaceId: "ws_1" }));
    // The page's landmark region and the events table caption share one name —
    // this pin keeps the page's aria-label from drifting off the constant.
    expect(m).toContain(`aria-label="${WORKSPACE_EVENTS_LABEL}"`);
  });

  it("events page preserves failed reads instead of reporting no activity", async () => {
    mockAuth({ token: "token_abc" });
    listWorkspaceEventsMock.mockRejectedValue(new Error("boom"));
    const { EventsData } =
      await import("../app/(dashboard)/w/[workspaceId]/events/page");
    await expect(EventsData({ workspaceId: "ws_1" })).rejects.toThrow("boom");
  });

  it("events page reads the cursor and page size from the URL", async () => {
    mockAuth({ token: "token_abc" });
    listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
    const mod = await import("../app/(dashboard)/w/[workspaceId]/events/page");
    // The default page awaits searchParams and derives the cursor (the URL
    // branch); EventsData receives it and asks the API for that page.
    renderToStaticMarkup(
      await mod.default({
        params: Promise.resolve({ workspaceId: "ws_1" }),
        searchParams: Promise.resolve({ c: "tok_1", cps: "50", ps: "50" }),
      }),
    );
    renderToStaticMarkup(
      await mod.EventsData({
        workspaceId: "ws_1",
        cursor: "tok_1",
        pageSize: 50,
      }),
    );
    expect(listWorkspaceEventsMock).toHaveBeenLastCalledWith(
      "ws_1",
      "token_abc",
      expect.objectContaining({ cursor: "tok_1", limit: 50 }),
    );
  });

  it("models & keys settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } =
      await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("billing settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } =
      await import("../app/(dashboard)/settings/billing/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });


});
