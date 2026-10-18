import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { deletePlatformFleetLibraryEntry, listPlatformFleetLibrary, patchPlatformFleetLibraryEntry, removeWorkspaceLibraryEntry, stagePlatformFleetLibrary } from "./fleet-library";
import { LIBRARY_CARD, PUBLICATION, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";

const OPERATION = "0199a801-1000-7000-8000-000000000009";
const TAG = '\"platform-one\"';
const platform = {card: {...LIBRARY_CARD, visibility: "platform_draft"}, published: false, updated_at: 0};
const fetchMock = vi.fn<typeof fetch>();
beforeEach(() => { fetchMock.mockReset(); vi.stubGlobal("fetch", fetchMock); });
afterEach(() => vi.unstubAllGlobals());

function call() {
  const [url, init] = fetchMock.mock.calls.at(-1) ?? [];
  return {url: typeof url === "string" ? url : url instanceof URL ? url.href : url?.url, method: init?.method, body: init?.body ? JSON.parse(typeof init.body === "string" ? init.body : "null") : undefined, headers: new Headers(init?.headers)};
}

describe("native Library transport", () => {
  it("removes only the encoded entry requested", async () => {
    fetchMock.mockResolvedValue(new Response(null, {status: 204}));
    await removeWorkspaceLibraryEntry(WORKSPACE, "../fleets", "session");
    expect(call()).toMatchObject({url: expect.stringContaining("/fleet-libraries/..%2Ffleets"), method: "DELETE"});
  });
  it("stages a reviewed candidate without publishing or source files", async () => {
    fetchMock.mockResolvedValue(response(PUBLICATION, 201));
    const body = {workspace_id: WORKSPACE, draft_candidate_id: OPERATION, label: "Reviewed"};
    expect(await stagePlatformFleetLibrary(body, OPERATION, "session")).toEqual(PUBLICATION);
    expect(call()).toMatchObject({url: expect.stringContaining("/v1/admin/fleet-libraries"), method: "POST", body});
    expect(call().headers.get("Idempotency-Key")).toBe(OPERATION); expect(fetchMock).toHaveBeenCalledTimes(1);
  });
  it("validates staged operator metadata", async () => {
    fetchMock.mockResolvedValue(response({entries: [platform]}));
    const result = await listPlatformFleetLibrary("session");
    expect(result.entries[0]).toMatchObject({id: LIBRARY_CARD.id, published: false, visibility: "platform_draft"});
  });
  it.each([TAG, `W/${TAG}`])("preserves the resource tag after conditional curation with response %s", async (etag) => {
    fetchMock.mockResolvedValue(response({...platform, card: {...platform.card, etag: TAG}}, 200, {etag}));
    const updated = await patchPlatformFleetLibraryEntry("recipe", {published: true}, TAG, "session");
    expect(updated.etag).toBe(TAG); expect(call().headers.get("If-Match")).toBe(TAG);
    expect(call().body).toEqual({published: true});
  });
  it.each([null, '\"other\"', 'W/\"other\"', `W/W/${TAG}`])("refuses an unconfirmed curation tag %s", async (etag) => {
    fetchMock.mockResolvedValue(response(platform, 200, etag ? {etag} : {}));
    await expect(patchPlatformFleetLibraryEntry("recipe", {published: true}, TAG, "session")).rejects.toThrow("ETag disagree");
  });
  it("deletes against the displayed operator revision", async () => {
    fetchMock.mockResolvedValue(new Response(null, {status: 204}));
    await deletePlatformFleetLibraryEntry("recipe", TAG, "session");
    expect(call().method).toBe("DELETE"); expect(call().headers.get("If-Match")).toBe(TAG);
  });
  it.each([401, 403, 409, 412])("surfaces refusal %i without retrying a mutation", async (status) => {
    fetchMock.mockResolvedValue(response({detail: "Refused", error_code: "UZ-AUTH-022"}, status));
    await expect(stagePlatformFleetLibrary({workspace_id: WORKSPACE, draft_candidate_id: OPERATION, label: "Reviewed"}, OPERATION, "session"))
      .rejects.toMatchObject({name: "ApiError", status, code: "UZ-AUTH-022"});
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
  it("refuses malformed native metadata instead of showing an empty catalog", async () => {
    fetchMock.mockResolvedValue(response({entries: [{id: "invalid-entry"}]}));
    await expect(listPlatformFleetLibrary("session")).rejects.not.toBeInstanceOf(ApiError);
  });
});
