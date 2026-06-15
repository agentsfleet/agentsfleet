import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import {
  listApiKeys,
  createApiKey,
  revokeApiKey,
  deleteApiKey,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  MAX_PAGE_SIZE,
} from "./api_keys";

beforeEach(() => {
  vi.clearAllMocks();
  requestMock.mockResolvedValue({ items: [], total: 0, page: 1, page_size: 25 });
});
afterEach(() => vi.resetAllMocks());

describe("listApiKeys", () => {
  it("defaults to page 1, the backend's page size, and newest-first sort", async () => {
    await listApiKeys("tok");
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/api-keys?page=1&page_size=${DEFAULT_PAGE_SIZE}&sort=${DEFAULT_SORT}`,
      { method: "GET" },
      "tok",
    );
  });

  it("never requests a page size above the backend maximum", async () => {
    // The UI fixes page_size to the default; the cap is the contract the
    // backend enforces (DEFAULT_PAGE_SIZE must stay <= MAX_PAGE_SIZE).
    expect(DEFAULT_PAGE_SIZE).toBeLessThanOrEqual(MAX_PAGE_SIZE);
    await listApiKeys("tok", { page: 3, sort: "key_name" });
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/api-keys?page=3&page_size=25&sort=key_name",
      { method: "GET" },
      "tok",
    );
  });
});

describe("createApiKey / revokeApiKey / deleteApiKey", () => {
  it("POSTs the create body verbatim", async () => {
    requestMock.mockResolvedValue({ id: "k", key_name: "ci", key: "agt_tx", created_at: 1 });
    await createApiKey("tok", { key_name: "ci", description: "runner" });
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/api-keys",
      { method: "POST", body: JSON.stringify({ key_name: "ci", description: "runner" }) },
      "tok",
    );
  });

  it("revoke PATCHes {active:false} and url-encodes the id", async () => {
    requestMock.mockResolvedValue({ id: "a", active: false, revoked_at: 1 });
    await revokeApiKey("tok", "a b/c");
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/api-keys/a%20b%2Fc",
      { method: "PATCH", body: JSON.stringify({ active: false }) },
      "tok",
    );
  });

  it("delete issues DELETE on the id", async () => {
    requestMock.mockResolvedValue(undefined);
    await deleteApiKey("tok", "id1");
    expect(requestMock).toHaveBeenCalledWith("/v1/api-keys/id1", { method: "DELETE" }, "tok");
  });

  it("propagates the request error (the action layer maps it to a toast)", async () => {
    requestMock.mockRejectedValue(new Error("boom"));
    await expect(revokeApiKey("tok", "id1")).rejects.toThrow("boom");
  });
});
