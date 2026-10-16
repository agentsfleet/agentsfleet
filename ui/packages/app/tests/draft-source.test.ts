// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { sourcePreview } from "@/lib/api/draft-source";
import { FLEET_DETAIL, FLEET_ID, LIBRARY, LIBRARY_CARD, REVISION_ID, REVIEW, WORKSPACE, response } from "./helpers/draft-fixtures";

const BASE = `/live/v1/workspaces/${WORKSPACE}`;
const signal = (): AbortSignal => new AbortController().signal;
afterEach(() => vi.unstubAllGlobals());

describe("read-only setup inspection before draft creation", () => {
  it("blank and inline sources allocate no resource", async () => {
    const fetch = vi.fn(); vi.stubGlobal("fetch", fetch);
    expect(await sourcePreview(BASE, {kind: "blank"}, signal())).toBeNull();
    expect(await sourcePreview(BASE, {kind: "definition", definition: REVIEW.effective.definition}, signal())).toMatchObject({definition: REVIEW.effective.definition});
    expect(fetch).not.toHaveBeenCalled();
  });
  it("Improving a fleet first reads exactly its pinned setup and instance name", async () => {
    const fetch = vi.fn().mockResolvedValue(response(FLEET_DETAIL)); vi.stubGlobal("fetch", fetch);
    expect(await sourcePreview(BASE, {kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}, signal())).toEqual({definition: FLEET_DETAIL.definition, instanceName: FLEET_DETAIL.name, revision: "1"});
    expect(fetch.mock.calls[0]?.[1]).toMatchObject({method: "GET"});
  });
  it.each([{...FLEET_DETAIL, setup_revision: "2"}, {...FLEET_DETAIL, id: REVISION_ID}])("refuses a changed Improve source", async (answer) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response(answer)));
    await expect(sourcePreview(BASE, {kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"}, signal())).rejects.toMatchObject({status: 409});
  });
  it("Library inspection uses the selected historical revision, independent of current head", async () => {
    const answer = {...LIBRARY, card: {...LIBRARY_CARD, head_revision_id: FLEET_ID}};
    const fetch = vi.fn().mockResolvedValue(response(answer)); vi.stubGlobal("fetch", fetch);
    expect(await sourcePreview(BASE, {kind: "library", library_id: LIBRARY_CARD.id, revision_id: REVISION_ID}, signal())).toMatchObject({definition: LIBRARY.head.definition, revision: REVISION_ID});
    expect(String(fetch.mock.calls[0]?.[0])).toContain(`revision_id=${REVISION_ID}`);
  });
  it.each([
    {...LIBRARY, head: {...LIBRARY.head, id: FLEET_ID}},
    {...LIBRARY, head: {...LIBRARY.head, library_id: "wrong"}},
    {...LIBRARY, card: {...LIBRARY_CARD, id: "wrong"}},
  ])("refuses a mismatched Library inspection", async (answer) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response(answer)));
    await expect(sourcePreview(BASE, {kind: "library", library_id: LIBRARY_CARD.id, revision_id: REVISION_ID}, signal())).rejects.toMatchObject({status: 409});
  });
});
