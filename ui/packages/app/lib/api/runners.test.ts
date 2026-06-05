import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import {
  listRunners,
  createRunner,
  parseLabels,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
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
    requestMock.mockResolvedValueOnce({ runner_id: "r1", runner_token: "zrn_abc" });
    const body = { host_id: "web-prod-1", sandbox_tier: "landlock_full" as const, labels: ["gpu"] };
    await createRunner("tok", body);
    expect(requestMock).toHaveBeenCalledWith("/v1/runners", { method: "POST", body: JSON.stringify(body) }, "tok");
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

describe("wire-contract constants mirror the Zig enums", () => {
  it("carries the four liveness tags and four sandbox tiers verbatim", () => {
    expect(RUNNER_LIVENESS).toEqual(["registered", "busy", "online", "offline"]);
    expect(SANDBOX_TIERS).toEqual(["landlock_full", "container_nested", "macos_seatbelt", "dev_none"]);
  });
});
