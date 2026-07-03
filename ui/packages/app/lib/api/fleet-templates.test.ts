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
  support_files: [],
};

describe("fleet template API client", () => {
  it("test_onboard_client_posts_tenant_endpoint", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 201, json: async () => onboarded });
    const { onboardWorkspaceFleetTemplate } = await import("./fleet-templates");
    const body = { source_kind: SOURCE_KIND_GITHUB, source_ref: "owner/repo" };
    const res = await onboardWorkspaceFleetTemplate("ws_1", body, "tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleet-templates"),
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
        body: JSON.stringify(body),
      }),
    );
    expect(res).toEqual(onboarded);
  });

  it("test_onboard_action_maps_apierror_to_errorcode: throws ApiError on 403", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 403,
      json: async () => ({ detail: "forbidden", error_code: "UZ-AUTH-022" }),
    });
    const { onboardWorkspaceFleetTemplate } = await import("./fleet-templates");
    const err = await onboardWorkspaceFleetTemplate(
      "ws_1",
      { source_kind: SOURCE_KIND_GITHUB, source_ref: "owner/repo" },
      "tok",
    ).catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(403);
    expect(err.code).toBe("UZ-AUTH-022");
  });
});
