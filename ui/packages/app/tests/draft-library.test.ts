// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { inspectLibrary, libraryHistory, libraryPage } from "@/lib/api/draft-library";
import { CANDIDATE_ID, LIBRARY, LIBRARY_CARD, REVISION_ID, WORKSPACE, response } from "./helpers/draft-fixtures";

afterEach(() => vi.unstubAllGlobals());
const signal = (): AbortSignal => new AbortController().signal;

describe("native Library reads", () => {
  it.each([false, true])("pages the selected gallery with a bound continuation; owned=%s", async (owned) => {
    const fetch = vi.fn().mockResolvedValue(response({items: [LIBRARY_CARD], next_cursor: "next"}));
    vi.stubGlobal("fetch", fetch);
    expect(await libraryPage(WORKSPACE, owned, "private+cursor", signal())).toMatchObject({items: [LIBRARY_CARD]});
    expect(String(fetch.mock.calls[0]?.[0])).toContain(`/fleet-libraries${owned ? "/owned" : ""}?limit=25&starting_after=private%2Bcursor`);
    expect(fetch.mock.calls[0]?.[1]).toMatchObject({method: "GET", credentials: "same-origin", cache: "no-store"});
  });
  it("inspects an explicit immutable revision of an opaque platform slug", async () => {
    const fetch = vi.fn().mockResolvedValue(response(LIBRARY)); vi.stubGlobal("fetch", fetch);
    expect(await inspectLibrary(WORKSPACE, LIBRARY_CARD, signal())).toEqual(LIBRARY);
    expect(String(fetch.mock.calls[0]?.[0])).toContain(`/fleet-libraries/${LIBRARY_CARD.id}?revision_id=${REVISION_ID}`);
  });
  it.each([
    {...LIBRARY, card: {...LIBRARY.card, id: "other"}},
    {...LIBRARY, head: {...LIBRARY.head, library_id: "other"}},
    {...LIBRARY, head: {...LIBRARY.head, id: CANDIDATE_ID}},
  ])("refuses an answer for a different entry or revision", async (answer) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response(answer)));
    await expect(inspectLibrary(WORKSPACE, LIBRARY_CARD, signal())).rejects.toMatchObject({status: 409});
  });
  it("reads content-free revision history and follows its cursor", async () => {
    const page = {items: [{id: REVISION_ID, revision: "1", canonical_digest: "digest", created_at: "1970-01-01T00:00:00Z"}], next_cursor: null};
    const fetch = vi.fn().mockResolvedValue(response(page)); vi.stubGlobal("fetch", fetch);
    expect(await libraryHistory(WORKSPACE, LIBRARY_CARD.id, "older+cursor", signal())).toEqual(page);
    expect(String(fetch.mock.calls[0]?.[0])).toContain("/revisions?limit=25&starting_after=older%2Bcursor");
  });
  it.each([401, 403, 404, 410])("preserves access loss %i for the UI to clear stale recipes", async (status) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({}, status)));
    await expect(inspectLibrary(WORKSPACE, LIBRARY_CARD, signal())).rejects.toMatchObject({status});
  });
  it("does not retain a cancelled late inspection", async () => {
    const controller = new AbortController();
    vi.stubGlobal("fetch", vi.fn(async () => { controller.abort(); return response(LIBRARY); }));
    await expect(inspectLibrary(WORKSPACE, LIBRARY_CARD, controller.signal)).rejects.toThrow();
  });
});
