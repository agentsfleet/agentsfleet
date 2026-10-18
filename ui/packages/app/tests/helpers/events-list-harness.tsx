import React from "react";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

// ── Shared mocks ───────────────────────────────────────────────────────────

// Paging is URL state now: the pager navigates and the Server Component
// fetches the page the cursor names, so these assert the URL that gets
// pushed rather than a client-side fetch.
const { routerPushMock, searchParamsRef } = vi.hoisted(() => ({
  routerPushMock: vi.fn(),
  searchParamsRef: { current: new URLSearchParams() },
}));
export { routerPushMock, searchParamsRef };

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPushMock }),
  usePathname: () => "/w/ws_1/events",
  useSearchParams: () => searchParamsRef.current,
}));

beforeEach(() => {
  vi.clearAllMocks();
  searchParamsRef.current = new URLSearchParams();
});

afterEach(() => cleanup());

// ── EventsList ─────────────────────────────────────────────────────────────

import { EventsList } from "@/components/domain/EventsList";
import { type EventDetail, type EventsPage } from "@/lib/api/events";

// Opening a row fetches its bodies through the Server Action — the list row
// carries none. Fixtures here are detail-shaped, so the action serves back the
// row under inspection.
let servedDetail: EventDetail | null = null;
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  getFleetEventAction: () =>
    Promise.resolve(
      servedDetail === null
        ? { ok: false as const, error: "not found" }
        : { ok: true as const, data: servedDetail },
    ),
}));

export function row(over: Partial<EventDetail> = {}): EventDetail {
  const now = Date.UTC(2026, 3, 28, 10, 30, 0);
  return {
    event_id: "evt_1",
    fleet_id: "zomb_1234567890ab",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "hello world",
    tool_calls: null,
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
    resumes_event_id: null,
    created_at: new Date(now).toISOString(),
    updated_at: new Date(now).toISOString(),
    ...over,
  };
}

// The wrapper stands in for a real ancestor: the root layout mounts the app's
// one TooltipProvider, which the relative `Time` cells here read from.
export function renderList(initial: EventsPage, fleetId?: string) {
  // Whatever a test inspects, the action serves that row's own bodies back.
  // Every fixture in this file is built detail-shaped by `row()` above.
  servedDetail = (initial.items[0] as EventDetail | undefined) ?? null;
  return render(
    React.createElement(EventsList, { initial, pageSize: 25, fleetId }),
    { wrapper: TooltipProvider },
  );
}

