import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { removeWorkspaceLibraryEntry } from "./fleet-library";
import { WORKSPACE } from "@/tests/helpers/draft-fixtures";

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
});
