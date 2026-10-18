import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";
import { SCOPE, expandScopes } from "@/lib/auth/scopes";
import { LIBRARY_CARD, PUBLICATION, WORKSPACE, response } from "./helpers/draft-fixtures";
import Page from "@/app/(dashboard)/admin/fleet-libraries/page";
import { deletePlatformLibraryAction, patchPlatformLibraryAction, stagePlatformLibraryAction } from "@/app/(dashboard)/admin/fleet-libraries/actions";

const {scope, credential, revalidate} = vi.hoisted(() => ({scope: vi.fn(), credential: vi.fn(), revalidate: vi.fn()}));
vi.mock("@/lib/auth/platform", () => ({hasScope: scope}));
vi.mock("@/lib/auth/credential", () => ({credential}));
vi.mock("next/cache", () => ({revalidatePath: revalidate}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh: vi.fn()}), redirect: (path: string) => {throw new Error(`redirect:${path}`);}}));
vi.mock("@/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView", () => ({default: () => "Native catalog"}));
const fetchMock = vi.fn<typeof fetch>();
const BODY = {workspace_id: WORKSPACE, draft_candidate_id: "0199a801-1000-7000-8000-000000000003", label: "Reviewed"};
const OPERATION = "0199a801-1000-7000-8000-000000000005";
const TAG = PUBLICATION.etag;
const ITEM = {card: LIBRARY_CARD, published: true, updated_at: "1970-01-01T00:00:00Z"};

beforeEach(() => {
  vi.clearAllMocks(); scope.mockResolvedValue(true); credential.mockResolvedValue("session");
  fetchMock.mockReset(); fetchMock.mockResolvedValue(response({entries: []})); vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

function lastWrite() {
  const [, init] = fetchMock.mock.calls.at(-1) ?? [];
  return {body: init?.body ? JSON.parse(typeof init.body === "string" ? init.body : "null") : undefined, headers: new Headers(init?.headers), method: init?.method};
}

describe("operator native Library page", () => {
  it("rejects a missing operator scope before reading private data", async () => {
    scope.mockResolvedValue(false);
    await expect(Page()).rejects.toThrow("fleet-libraries-platform-admin-only");
    expect(fetchMock).not.toHaveBeenCalled(); expect(credential).not.toHaveBeenCalled();
  });
  it("renders the native catalog only after scoped authentication", async () => {
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("Native catalog"); expect(scope).toHaveBeenCalledWith(SCOPE.PLATFORM_LIBRARY_WRITE);
    expect(new Headers(fetchMock.mock.calls[0]?.[1]?.headers).get("Authorization")).toBe("Bearer session");
  });
  it("shows a failed catalog read and its retry action", async () => {
    fetchMock.mockResolvedValue(response({detail: "Catalog unavailable", error_code: "UZ-AUTH-022"}, 403));
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("Retry fleet library"); expect(html).not.toContain("Native catalog");
  });
  it("offers sign-in for an expired session without showing an empty catalog", async () => {
    credential.mockResolvedValue(null);
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain('href="/sign-in"');
    expect(html).toContain("Your session has expired");
    expect(html).toContain('data-slot="unavailable-state"');
    expect(html).not.toContain("Native catalog");
  });
  it("other operator scopes never grant Library publication", () => {
    expect(expandScopes([SCOPE.MODEL_ADMIN, SCOPE.RUNNER_WRITE]).has(SCOPE.PLATFORM_LIBRARY_WRITE)).toBe(false);
  });
});

describe("operator native Library mutations", () => {
  it("preserves an unconfirmed deletion without invalidating the catalog", async () => {
    fetchMock.mockRejectedValue(new Error("Lost deletion answer"));
    expect(await deletePlatformLibraryAction(LIBRARY_CARD.id, TAG)).toMatchObject({ok: false, error: UNCONFIRMED_REQUEST_MESSAGE});
    expect(fetchMock).toHaveBeenCalledOnce(); expect(revalidate).not.toHaveBeenCalled();
  });
  it("stages with the chat client's retained key and exact candidate body", async () => {
    fetchMock.mockResolvedValue(response(PUBLICATION, 201));
    expect(await stagePlatformLibraryAction(BODY, OPERATION)).toEqual({ok: true, data: PUBLICATION});
    expect(lastWrite().body).toEqual(BODY); expect(lastWrite().headers.get("Idempotency-Key")).toBe(OPERATION);
    expect(revalidate).toHaveBeenCalledWith("/admin/fleet-libraries");
  });
  it("curates metadata with the displayed strong tag", async () => {
    fetchMock.mockResolvedValue(response(ITEM, 200, {etag: TAG}));
    const result = await patchPlatformLibraryAction(LIBRARY_CARD.id, {description: "Clearer"}, TAG);
    expect(result).toMatchObject({ok: true, data: {id: LIBRARY_CARD.id, etag: TAG}});
    expect(lastWrite()).toMatchObject({method: "PATCH", body: {description: "Clearer"}});
    expect(lastWrite().headers.get("If-Match")).toBe(TAG);
  });
  it("deletes only against the observed tag", async () => {
    fetchMock.mockResolvedValue(new Response(null, {status: 204}));
    expect(await deletePlatformLibraryAction(LIBRARY_CARD.id, TAG)).toEqual({ok: true, data: undefined});
    expect(lastWrite().method).toBe("DELETE"); expect(lastWrite().headers.get("If-Match")).toBe(TAG);
  });
  it.each(["stage", "patch", "delete"])("refuses %s before any mutation without its scope", async (kind) => {
    scope.mockResolvedValue(false);
    const result = kind === "stage" ? await stagePlatformLibraryAction(BODY, OPERATION)
      : kind === "patch" ? await patchPlatformLibraryAction(LIBRARY_CARD.id, {published: true}, TAG)
      : await deletePlatformLibraryAction(LIBRARY_CARD.id, TAG);
    expect(result).toMatchObject({ok: false, status: 403}); expect(fetchMock).not.toHaveBeenCalled(); expect(revalidate).not.toHaveBeenCalled();
  });
  it("preserves a stale revision refusal and never replaces its tag or retries", async () => {
    fetchMock.mockResolvedValue(response({detail: "Changed", error_code: "UZ-AUTH-022"}, 412));
    expect(await patchPlatformLibraryAction(LIBRARY_CARD.id, {published: true}, TAG)).toMatchObject({ok: false, status: 412, errorCode: "UZ-AUTH-022"});
    expect(fetchMock).toHaveBeenCalledOnce(); expect(revalidate).not.toHaveBeenCalled();
  });
  it("preserves an unknown staging outcome for the chat retry owner", async () => {
    fetchMock.mockRejectedValue(new Error("Lost answer"));
    expect(await stagePlatformLibraryAction(BODY, OPERATION)).toMatchObject({ok: false, error: UNCONFIRMED_REQUEST_MESSAGE});
    expect(fetchMock).toHaveBeenCalledOnce(); expect(revalidate).not.toHaveBeenCalled();
  });
});
