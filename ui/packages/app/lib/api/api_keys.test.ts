import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { requestMock } = vi.hoisted(() => ({ requestMock: vi.fn() }));
vi.mock("./client", () => ({ request: requestMock }));

import {
  listApiKeys,
  createApiKey,
  revokeApiKey,
  deleteApiKey,
  DEFAULT_SORT,
} from "./api_keys";

const keyRow = (id: string) => ({
  id,
  key_name: id,
  active: true,
  created_at: 1700000000000,
  last_used_at: null,
  revoked_at: null,
});

beforeEach(() => {
  vi.clearAllMocks();
  requestMock.mockResolvedValue({ items: [], total: 0, next_cursor: null });
});
afterEach(() => vi.resetAllMocks());

describe("listApiKeys", () => {
  it("sends no paging parameters and defaults to newest-first sort", async () => {
    await listApiKeys("tok");
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/api-keys?sort=${encodeURIComponent(DEFAULT_SORT)}`,
      { method: "GET" },
      "tok",
    );
  });

  it("test_api_key_list_view_walks_next_cursor_to_exhaustion", async () => {
    requestMock
      .mockResolvedValueOnce({ items: [keyRow("a")], total: 3, next_cursor: "cur_1" })
      .mockResolvedValueOnce({ items: [keyRow("b")], total: 3, next_cursor: "cur_2" })
      .mockResolvedValueOnce({ items: [keyRow("c")], total: 3, next_cursor: null });
    const res = await listApiKeys("tok", "key_name");
    expect(res.items.map((item) => item.id)).toEqual(["a", "b", "c"]);
    expect(res.next_cursor).toBeNull();
    expect(res.total).toBe(3);
    expect(requestMock).toHaveBeenNthCalledWith(
      2,
      "/v1/api-keys?sort=key_name&starting_after=cur_1",
      { method: "GET" },
      "tok",
    );
    expect(requestMock).toHaveBeenCalledTimes(3);
  });

  it("refuses a runaway cursor instead of walking forever", async () => {
    requestMock.mockResolvedValue({ items: [keyRow("x")], total: null, next_cursor: "same" });
    await expect(listApiKeys("tok")).rejects.toThrow(/did not end/);
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
