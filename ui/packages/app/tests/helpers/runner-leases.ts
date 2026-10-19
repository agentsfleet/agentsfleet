import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import type { RunnerLease } from "@/lib/api/runners";

const { goToPage, changePageSize, routerPush, leaseView } = vi.hoisted(() => ({
  goToPage: vi.fn(), changePageSize: vi.fn(), routerPush: vi.fn(),
  leaseView: { hasNext: false, search: "" },
}));
export { goToPage, changePageSize, routerPush, leaseView };
export const MOCK_PATHNAME = "/runners/runner-under-test";
export const LEASES_PAST_ONE_PAGE = 60;
export const OLDER_INSTANT_MS = 1_000;
export const NEWER_INSTANT_MS = 2_000;

vi.mock("@/lib/pagination/use-url-cursor-pages", () => ({
  useUrlCursorPages: () => ({page: 1, hasNext: leaseView.hasNext, isLoading: false, goToPage, changePageSize}),
}));
vi.mock("next/navigation", () => ({
  usePathname: () => MOCK_PATHNAME,
  useRouter: () => ({push: routerPush}),
  useSearchParams: () => new URLSearchParams(leaseView.search),
}));

afterEach(() => cleanup());
beforeEach(() => {
  leaseView.hasNext = false;
  leaseView.search = "";
  goToPage.mockReset(); changePageSize.mockReset(); routerPush.mockReset();
});

export function lease(overrides: Partial<RunnerLease>): RunnerLease {
  return {
    id: `lease-${Math.random().toString(36).slice(2)}`,
    fleet_id: "fleet-1",
    fleet_name: "Search Services",
    destination: "available",
    workspace_id: "ws-1",
    event_id: "evt-1",
    event_type: "index_build",
    actor: "system",
    outcome: "succeeded",
    failure_label: null,
    failure_detail: null,
    kind: "fresh",
    fencing_token: 1884,
    provider: "azure_openai",
    model: "gpt-4o-mini",
    posture: "metered",
    metered_input_tokens: 18204,
    metered_cached_tokens: 4096,
    metered_output_tokens: 2881,
    wall_ms: 242_000,
    lease_expires_at: new Date().toISOString(),
    created_at: new Date().toISOString(),
    ...overrides,
  };
}
