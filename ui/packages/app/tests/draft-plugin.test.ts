// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { inspectPlugin, pluginHistory, pluginPage } from "@/lib/api/draft-plugin";
import { CANDIDATE_ID, PLUGIN, PLUGIN_CARD, REVISION_ID, WORKSPACE, response } from "./helpers/draft-fixtures";

afterEach(() => vi.unstubAllGlobals());
const signal = (): AbortSignal => new AbortController().signal;

describe("native Plugin reads", () => {
  it("pages account plugins with a bound continuation", async () => {
    const fetch = vi.fn().mockResolvedValue(response({items: [PLUGIN_CARD], next_cursor: "next"}));
    vi.stubGlobal("fetch", fetch);
    expect(await pluginPage(WORKSPACE, "private+cursor", signal())).toMatchObject({items: [PLUGIN_CARD]});
    expect(String(fetch.mock.calls[0]?.[0])).toContain(`/plugins?limit=25&starting_after=private%2Bcursor`);
    expect(fetch.mock.calls[0]?.[1]).toMatchObject({method: "GET", credentials: "same-origin", cache: "no-store"});
  });
  it("inspects an explicit immutable revision of an account-owned Plugin", async () => {
    const fetch = vi.fn().mockResolvedValue(response(PLUGIN)); vi.stubGlobal("fetch", fetch);
    expect(await inspectPlugin(WORKSPACE, PLUGIN_CARD, signal())).toEqual(PLUGIN);
    expect(String(fetch.mock.calls[0]?.[0])).toContain(`/plugins/${PLUGIN_CARD.id}?revision_id=${REVISION_ID}`);
  });
  it.each([
    {...PLUGIN, card: {...PLUGIN.card, id: CANDIDATE_ID}},
    {...PLUGIN, head: {...PLUGIN.head, plugin_id: CANDIDATE_ID}},
    {...PLUGIN, head: {...PLUGIN.head, id: CANDIDATE_ID}},
  ])("refuses an answer for a different entry or revision", async (answer) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response(answer)));
    await expect(inspectPlugin(WORKSPACE, PLUGIN_CARD, signal())).rejects.toMatchObject({status: 409});
  });
  it("reads content-free revision history and follows its cursor", async () => {
    const page = {items: [{id: REVISION_ID, revision: "1", canonical_digest: "digest", created_at: "1970-01-01T00:00:00Z"}], next_cursor: null};
    const fetch = vi.fn().mockResolvedValue(response(page)); vi.stubGlobal("fetch", fetch);
    expect(await pluginHistory(WORKSPACE, PLUGIN_CARD.id, "older+cursor", signal())).toEqual(page);
    expect(String(fetch.mock.calls[0]?.[0])).toContain("/revisions?limit=25&starting_after=older%2Bcursor");
  });
  it.each([401, 403, 404, 410])("preserves access loss %i for the UI to clear stale recipes", async (status) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({}, status)));
    await expect(inspectPlugin(WORKSPACE, PLUGIN_CARD, signal())).rejects.toMatchObject({status});
  });
  it("does not retain a cancelled late inspection", async () => {
    const controller = new AbortController();
    vi.stubGlobal("fetch", vi.fn(async () => { controller.abort(); return response(PLUGIN); }));
    await expect(inspectPlugin(WORKSPACE, PLUGIN_CARD, controller.signal)).rejects.toThrow();
  });
});
