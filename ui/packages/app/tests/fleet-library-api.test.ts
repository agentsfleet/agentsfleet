import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Pure API-client tests for lib/api/fleet-library against a stubbed fetch. The
// dashboard/install component + route tests mock this module, so these cover the
// real transport (GET/POST + the cache() wrapper) directly.
const fetchMock = vi.fn();
const CATALOG_ETAG = '"catalog-v1"';
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  fetchMock.mockReset();
});

describe("lib/api/fleet-library", () => {
  it("listWorkspaceFleetLibrary sends GET with bearer to the gallery path", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "tmpl_1", name: "T", visibility: "platform" }], total: 1 }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibrary("ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleet-libraries"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.items[0]?.id).toBe("tmpl_1");
  });

  it("the cached reader forwards to the same GET", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ items: [] }) });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibraryCached("ws_2", "tkn2");
    expect(res.items).toHaveLength(0);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_2/fleet-libraries"),
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("onboardWorkspaceFleetLibrary POSTs the body to the gallery path", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ id: "tmpl_new", name: "N", visibility: "tenant", content_hash: "sha256:x" }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.onboardWorkspaceFleetLibrary("ws_1", { source_kind: "github", source_ref: "acme/fleet" }, "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleet-libraries"),
      expect.objectContaining({ method: "POST" }),
    );
    expect(res.id).toBe("tmpl_new");
  });

  it("onboardPlatformFleetLibrary POSTs to the admin path — no workspace segment", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({
        id: "platform-ops",
        name: "Platform operations diagnostician",
        visibility: "platform",
        content_hash: "sha256:y",
      }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.onboardPlatformFleetLibrary(
      { source_kind: "github", source_ref: "agentsfleet/platform-ops" },
      "operator-tkn",
    );

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    // The platform tier is workspace-less: a workspace segment here would mean
    // the operator surface had been pointed at the tenant route.
    expect(url).toContain("/v1/admin/fleet-libraries");
    expect(url).not.toContain("/v1/workspaces/");
    expect(init).toMatchObject({ method: "POST" });
    expect(init.headers).toMatchObject({ Authorization: "Bearer operator-tkn" });
    expect(JSON.parse(init.body as string)).toEqual({
      source_kind: "github",
      source_ref: "agentsfleet/platform-ops",
    });

    // The catalog id comes back from the bundle, not from the repository path.
    expect(res.id).toBe("platform-ops");
    expect(res.visibility).toBe("platform");
  });

  // ── The platform catalog (M128) ────────────────────────────────────────────

  it("listPlatformFleetLibrary GETs the admin path with the operator's token", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ entries: [{ id: "platform-ops", visibility: "draft" }] }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listPlatformFleetLibrary("operator-tkn");

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toContain("/v1/admin/fleet-libraries");
    expect(url).not.toContain("/v1/workspaces/");
    expect(init).toMatchObject({ method: "GET" });
    expect(init.headers).toMatchObject({ Authorization: "Bearer operator-tkn" });
    expect(res.entries[0]?.id).toBe("platform-ops");
  });

  it("patchPlatformFleetLibraryEntry sends the current ETag with the partial body", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ id: "platform-ops", visibility: "public" }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.patchPlatformFleetLibraryEntry(
      "platform-ops",
      { published: true },
      CATALOG_ETAG,
      "operator-tkn",
    );

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toContain("/v1/admin/fleet-libraries/platform-ops");
    expect(init).toMatchObject({ method: "PATCH" });
    expect(init.headers).toMatchObject({ "If-Match": CATALOG_ETAG });
    expect(JSON.parse(init.body as string)).toEqual({ published: true });
    expect(res.visibility).toBe("public");
  });

  // A catalog id is a slug from bundle frontmatter, not a value this client gets
  // to trust — anything path-significant in it must be escaped, not interpolated.
  it("encodes the catalog id into the path", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: async () => ({}) });
    const mod = await import("../lib/api/fleet-library");
    await mod.deletePlatformFleetLibraryEntry("weird/id", "operator-tkn");

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toContain("/v1/admin/fleet-libraries/weird%2Fid");
    expect(init).toMatchObject({ method: "DELETE" });
    expect(init.headers).toMatchObject({ Authorization: "Bearer operator-tkn" });
  });
  // The gallery is a keyset page, so the client walks it. Both halves of that walk
  // need a test: the resume (a second request carrying starting_after) and the
  // bound, which is the only path that throws.
  // The exhaustive walk was replaced by one page per call. These three cases
  // pin what replaced it: a single request, the cursor forwarded verbatim, and
  // the disclosure fields surfaced rather than swallowed — the last is what
  // keeps paging from hiding entries the way a bare single-page read would.
  it("issues exactly one request per call and does not follow next_cursor", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "a" }], total: 7, next_cursor: "cur-1" }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibrary("ws_1", "tkn");

    // The walk would have issued a second request on seeing "cur-1".
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(res.items.map((i) => i.id)).toEqual(["a"]);
    expect(String(fetchMock.mock.calls[0]?.[0])).not.toContain("starting_after");
  });

  it("returns next_cursor and total so the caller can disclose what it has not loaded", async () => {
    // Invariant 5. Without these a gallery could render one card and have no
    // way to say seven entries exist — the silent truncation the walk existed
    // to prevent, reintroduced by the fix for it.
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "a" }], total: 7, next_cursor: "cur-1" }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibrary("ws_1", "tkn");

    expect(res.next_cursor).toBe("cur-1");
    expect(res.total).toBe(7);
  });

  it("forwards starting_after when asked for a later page", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "b" }], total: 7, next_cursor: null }),
    });
    const mod = await import("../lib/api/fleet-library");
    const res = await mod.listWorkspaceFleetLibrary("ws_1", "tkn", "cur-1");

    expect(String(fetchMock.mock.calls[0]?.[0])).toContain("starting_after=cur-1");
    // A null next_cursor is the server saying this is the last page.
    expect(res.next_cursor).toBeNull();
  });
});
