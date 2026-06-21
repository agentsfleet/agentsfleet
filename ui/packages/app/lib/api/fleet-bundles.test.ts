import { beforeEach, describe, expect, it, vi } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import { importBundleSnapshot, listFleetTemplates } from "./fleet-bundles";

beforeEach(() => vi.clearAllMocks());

describe("fleet-bundles API client", () => {
  it("listFleetTemplates GETs the first-party catalog with the token", async () => {
    requestMock.mockResolvedValue({ items: [{ id: "github-pr-reviewer" }] });
    const result = await listFleetTemplates("tok");
    expect(result).toEqual({ items: [{ id: "github-pr-reviewer" }] });
    expect(requestMock).toHaveBeenCalledWith("/v1/fleets/bundles", { method: "GET" }, "tok");
  });

  it("importBundleSnapshot POSTs the source to the workspace snapshot route", async () => {
    requestMock.mockResolvedValue({ bundle_id: "bnd_1" });
    const body = { source_kind: "github", source_ref: "acme/pr-reviewer" } as const;
    const result = await importBundleSnapshot("ws_1", body, "tok");
    expect(result).toEqual({ bundle_id: "bnd_1" });
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/workspaces/ws_1/fleets/bundles/snapshots",
      { method: "POST", body: JSON.stringify(body) },
      "tok",
    );
  });
});
