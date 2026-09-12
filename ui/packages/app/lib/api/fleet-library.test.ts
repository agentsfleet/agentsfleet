import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { SOURCE_KIND_GITHUB } from "../types";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

const onboarded = {
  id: "tmpl_1",
  name: "GitHub PR reviewer",
  visibility: "tenant",
  content_hash: "sha256:abc",
  requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
};

describe("fleet template API client", () => {
  it("test_onboard_client_posts_tenant_endpoint", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 201, json: async () => onboarded });
    const { onboardWorkspaceFleetLibrary } = await import("./fleet-library");
    const body = { source_kind: SOURCE_KIND_GITHUB, source_ref: "owner/repo" };
    const res = await onboardWorkspaceFleetLibrary("ws_1", body, "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleet-libraries"),
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
        body: JSON.stringify(body),
      }),
    );
    expect(res).toEqual(onboarded);
  });

  // The regression this pins: a POST gets `maxAttempts: 1` and, with no signal
  // of its own, the transport's 10s default. GitHub serving the tarball slower
  // than that abandoned the import in the dialog while the daemon was still
  // fetching — the acceptance suite's own failure before this budget existed.
  it.each([
    [
      "tenant",
      async (mod: typeof import("./fleet-library")) =>
        mod.onboardWorkspaceFleetLibrary("ws_1", { source_kind: SOURCE_KIND_GITHUB, source_ref: "owner/repo" }, "tok"),
    ],
    [
      "platform",
      async (mod: typeof import("./fleet-library")) =>
        mod.onboardPlatformFleetLibrary({ source_kind: SOURCE_KIND_GITHUB, source_ref: "owner/repo" }, "tok"),
    ],
  ])("test_onboard_%s_carries_the_bundle_budget_not_the_transport_default", async (_tier, call) => {
    fetchMock.mockResolvedValue({ ok: true, status: 201, json: async () => onboarded });
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout");
    try {
      const mod = await import("./fleet-library");
      await call(mod);
      expect(timeoutSpy).toHaveBeenCalledWith(mod.ONBOARD_BUNDLE_TIMEOUT_MS);
      const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
      expect(init.signal).toBeInstanceOf(AbortSignal);
      expect(init.signal?.aborted).toBe(false);
    } finally {
      timeoutSpy.mockRestore();
    }
  });

  // The ordering the fix rests on, held by a pin rather than an import: the
  // dashboard's budget must outlast what the backend spends before it can
  // answer. `rustd/crates/afd_library/src/github.rs` gives the tarball fetch
  // CONNECT_TIMEOUT 5s plus REQUEST_TIMEOUT 60s; extraction, the object-store
  // write and the catalog upsert all land after that.
  it("test_onboard_budget_outlasts_the_backend_fetch_ceiling", async () => {
    const { ONBOARD_BUNDLE_TIMEOUT_MS } = await import("./fleet-library");
    // pin test: literal is the contract
    const BACKEND_FETCH_CEILING_MS = (5 + 60) * 1_000;
    expect(ONBOARD_BUNDLE_TIMEOUT_MS).toBeGreaterThan(BACKEND_FETCH_CEILING_MS);
  });

  it("test_onboard_action_maps_apierror_to_errorcode: throws ApiError on 403", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 403,
      json: async () => ({ detail: "forbidden", error_code: "UZ-AUTH-022" }),
    });
    const { onboardWorkspaceFleetLibrary } = await import("./fleet-library");
    const err = await onboardWorkspaceFleetLibrary(
      "ws_1",
      { source_kind: SOURCE_KIND_GITHUB, source_ref: "owner/repo" },
      "tok",
    ).catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(403);
    expect(err.code).toBe("UZ-AUTH-022");
  });
});
