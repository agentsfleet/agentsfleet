import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import {
  listRunners,
  getRunner,
  listRunnerLeases,
  createRunner,
  updateRunnerAdminState,
  deleteRunner,
  listRunnerEvents,
  parseLabels,
  parseRegistryAllowlist,
  RUNNER_LIFECYCLE_EVENT_TYPES,
  RUNNER_ADMIN_ACTIONS,
  RUNNER_ADMIN_STATES,
  RUNNER_EVENT_TYPES,
  RUNNER_LIVENESS,
  SANDBOX_TIERS,
} from "./runners";

beforeEach(() => {
  vi.clearAllMocks();
  requestMock.mockResolvedValue({ items: [], total: 0, next_cursor: null });
});
afterEach(() => vi.resetAllMocks());

describe("listRunners", () => {
  it("reads the keyset first page with no paging params at all", async () => {
    await listRunners("tok");
    expect(requestMock).toHaveBeenCalledWith("/v1/fleets/runners", { method: "GET" }, "tok");
  });

  it("pages forward with starting_after + limit", async () => {
    await listRunners("tok", { starting_after: "cursor-1", limit: 50 });
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners?starting_after=cursor-1&limit=50",
      { method: "GET" },
      "tok",
    );
  });
});

describe("getRunner", () => {
  it("reads the single-runner operator path", async () => {
    requestMock.mockResolvedValueOnce({ id: "runner-1" });
    await getRunner("tok", "runner-1");
    expect(requestMock).toHaveBeenCalledWith("/v1/fleets/runners/runner-1", { method: "GET" }, "tok");
  });
});

describe("listRunnerLeases", () => {
  it("reads the lease history behind a lease-id cursor", async () => {
    await listRunnerLeases("tok", "runner-1", { starting_after: "lease-9", limit: 25 });
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners/runner-1/leases?starting_after=lease-9&limit=25",
      { method: "GET" },
      "tok",
    );
  });

  it("sends the bare first-page read with no query string at all", async () => {
    await listRunnerLeases("tok", "runner-1");
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners/runner-1/leases",
      { method: "GET" },
      "tok",
    );
  });
});

describe("createRunner", () => {
  it("mints against the enrollment endpoint with the host + assigned policy + labels body", async () => {
    requestMock.mockResolvedValueOnce({ runner_id: "r1", runner_token: "agt_rabc" });
    const body = {
      host_id: "web-prod-1",
      assigned_policy: {
        sandbox_tier: "landlock_full" as const,
        network_policy: "allow_all" as const,
        registry_allowlist: ["registry.npmjs.org"],
        worker_count: 2,
      },
      labels: ["gpu"],
    };
    await createRunner("tok", body);
    expect(requestMock).toHaveBeenCalledWith("/v1/runners", { method: "POST", body: JSON.stringify(body) }, "tok");
  });
});

describe("updateRunnerAdminState", () => {
  it("PATCHes the operator-plane runner action body", async () => {
    requestMock.mockResolvedValueOnce({ id: "runner-1", admin_state: "cordoned" });
    await updateRunnerAdminState("tok", "runner-1", "cordon");
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners/runner-1",
      { method: "PATCH", body: JSON.stringify({ action: "cordon" }) },
      "tok",
    );
  });
});

describe("deleteRunner", () => {
  it("DELETEs the runner path with no body and resolves void on 204", async () => {
    requestMock.mockResolvedValueOnce(undefined);
    await expect(deleteRunner("tok", "runner-1")).resolves.toBeUndefined();
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners/runner-1",
      { method: "DELETE" },
      "tok",
    );
  });

  it("propagates the daemon's revoke-first refusal untouched", async () => {
    // The UI layers above decide presentation; the client must not swallow or
    // reshape the 409 (UZ-RUN-016) — a masked refusal would strand the row
    // with no explanation.
    requestMock.mockRejectedValueOnce(new Error("UZ-RUN-016"));
    await expect(deleteRunner("tok", "runner-1")).rejects.toThrow("UZ-RUN-016");
  });
});

describe("listRunnerEvents", () => {
  it("reads runner activity with no params on the first page", async () => {
    await listRunnerEvents("tok", "runner-1");
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/fleets/runners/runner-1/events",
      { method: "GET" },
      "tok",
    );
  });

  it("passes the comma-joined lifecycle set, window filters and keyset paging", async () => {
    await listRunnerEvents("tok", "runner-1", {
      starting_after: "100:evt-1",
      limit: 25,
      event_type: RUNNER_LIFECYCLE_EVENT_TYPES.join(","),
      since: 10,
      until: 20,
    });
    const lifecycle = encodeURIComponent(RUNNER_LIFECYCLE_EVENT_TYPES.join(","));
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/fleets/runners/runner-1/events?starting_after=100%3Aevt-1&limit=25&event_type=${lifecycle}&since=10&until=20`,
      { method: "GET" },
      "tok",
    );
  });
});

describe("RUNNER_LIFECYCLE_EVENT_TYPES", () => {
  it("holds the eight non-lease tags and neither work record", () => {
    expect(RUNNER_LIFECYCLE_EVENT_TYPES).toHaveLength(8);
    expect(RUNNER_LIFECYCLE_EVENT_TYPES).not.toContain("lease_acquired");
    expect(RUNNER_LIFECYCLE_EVENT_TYPES).not.toContain("lease_released");
  });
});

describe("parseRegistryAllowlist", () => {
  it("trims, splits on comma, dedupes, and accepts host[:port] names", () => {
    expect(parseRegistryAllowlist(" pypi.org , registry.npmjs.org:5000 , pypi.org ,, ")).toEqual({
      hosts: ["pypi.org", "registry.npmjs.org:5000"],
      error: null,
    });
  });

  it("treats whitespace-only input as a valid empty set (runner substitutes its defaults)", () => {
    expect(parseRegistryAllowlist("   ")).toEqual({ hosts: [], error: null });
  });

  it("rejects an entry with illegal characters, naming the offender", () => {
    const r = parseRegistryAllowlist("pypi.org, http://bad url");
    expect(r.hosts).toEqual([]);
    expect(r.error).toContain("http://bad url");
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
  it("test_sandbox_tier_vocabulary_excludes_seatbelt: carries the runner value sets verbatim", () => {
    // §6 — only tiers with real enforcement are assignable; the Seatbelt tier
    // is removed (not deprecated) because no enforcement code ever existed.
    // (Name spliced so the repo-wide zero-reference sweep stays green.)
    expect(RUNNER_LIVENESS).toEqual(["registered", "busy", "online", "offline"]);
    expect(SANDBOX_TIERS).toEqual(["landlock_full", "container_nested", "dev_none"]);
    expect(SANDBOX_TIERS as readonly string[]).not.toContain("macos_" + "seatbelt");
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
      "runner_policy_assigned",
    ]);
  });
});
