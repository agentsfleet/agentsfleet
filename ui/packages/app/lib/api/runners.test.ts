import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import {
  listRunners,
  createRunner,
  updateRunnerAdminState,
  listRunnerEvents,
  parseLabels,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  RUNNER_ADMIN_ACTIONS,
  RUNNER_ADMIN_STATES,
  RUNNER_EVENT_TYPES,
  RUNNER_LIVENESS,
  SANDBOX_TIERS,
} from "./runners";

beforeEach(() => {
  vi.clearAllMocks();
  requestMock.mockResolvedValue({ items: [], total: 0, page: 1, page_size: 25 });
});
afterEach(() => vi.resetAllMocks());

describe("listRunners", () => {
  it("reads the platform-admin operator-plane path with default paging", async () => {
    await listRunners("tok");
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/fleet/runners?page=1&page_size=${DEFAULT_PAGE_SIZE}&sort=${DEFAULT_SORT}`,
      { method: "GET" },
      "tok",
    );
  });

  it("passes through explicit page + sort", async () => {
    await listRunners("tok", { page: 2, sort: "host_id" });
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleet/runners?page=2&page_size=25&sort=host_id",
      { method: "GET" },
      "tok",
    );
  });
});

describe("createRunner", () => {
  it("mints against the enrollment endpoint with the host + tier + labels body", async () => {
    requestMock.mockResolvedValueOnce({ runner_id: "r1", runner_token: "agt_rabc" });
    const body = { host_id: "web-prod-1", sandbox_tier: "landlock_full" as const, labels: ["gpu"] };
    await createRunner("tok", body);
    expect(requestMock).toHaveBeenCalledWith("/v1/runners", { method: "POST", body: JSON.stringify(body) }, "tok");
  });
});

describe("updateRunnerAdminState", () => {
  it("PATCHes the operator-plane runner action body", async () => {
    requestMock.mockResolvedValueOnce({ id: "runner-1", admin_state: "cordoned" });
    await updateRunnerAdminState("tok", "runner-1", "cordon");
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleet/runners/runner-1",
      { method: "PATCH", body: JSON.stringify({ action: "cordon" }) },
      "tok",
    );
  });
});

describe("listRunnerEvents", () => {
  it("reads runner activity with default paging", async () => {
    await listRunnerEvents("tok", "runner-1");
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/fleet/runners/runner-1/events?page=1&page_size=${DEFAULT_PAGE_SIZE}`,
      { method: "GET" },
      "tok",
    );
  });

  it("passes through activity filters and explicit paging", async () => {
    await listRunnerEvents("tok", "runner-1", {
      page: 2,
      page_size: DEFAULT_PAGE_SIZE,
      event_type: "runner_online",
      since: 10,
      until: 20,
    });
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/fleet/runners/runner-1/events?page=2&page_size=${DEFAULT_PAGE_SIZE}&event_type=runner_online&since=10&until=20`,
      { method: "GET" },
      "tok",
    );
  });
});

describe("parseLabels", () => {
  it("trims, splits on comma, and drops empties", () => {
    expect(parseLabels(" gpu , us-east ,, ")).toEqual({ labels: ["gpu", "us-east"], error: null });
  });

  it("dedupes repeated labels", () => {
    expect(parseLabels("gpu, gpu, gpu")).toEqual({ labels: ["gpu"], error: null });
  });

  it("treats whitespace-only input as a valid empty set", () => {
    expect(parseLabels("   ")).toEqual({ labels: [], error: null });
  });

  it("rejects a label with illegal characters, naming the offender", () => {
    const r = parseLabels("gpu, bad label!");
    expect(r.labels).toEqual([]);
    expect(r.error).toContain("bad label!");
  });
});

describe("wire constants mirror the Zig enums", () => {
  it("carries the runner value sets verbatim", () => {
    expect(RUNNER_LIVENESS).toEqual(["registered", "busy", "online", "offline"]);
    expect(SANDBOX_TIERS).toEqual(["landlock_full", "container_nested", "macos_seatbelt", "dev_none"]);
    expect(RUNNER_ADMIN_STATES).toEqual(["active", "cordoned", "draining", "drained", "revoked"]);
    expect(RUNNER_ADMIN_ACTIONS).toEqual(["cordon", "drain", "revoke"]);
    expect(RUNNER_EVENT_TYPES).toEqual([
      "runner_registered",
      "runner_online",
      "runner_offline",
      "lease_acquired",
      "lease_released",
      "runner_cordoned",
      "runner_draining",
      "runner_drained",
      "runner_revoked",
    ]);
  });
});
